use super::*;
use bace_content::{Position, Property, WeenieV1};
use bace_inventory::*;
use bace_storage_codec::{
    CorpseSaveV1, CorpseSaveV2, CorpseSaveV3, CorpseSaveV4, CorpseSaveV5, EntitySaveV1,
    ItemPlacementV2,
};
fn source(id: u32) -> (bace_simulation::RegionUnloadItem, RegionItemSource) {
    let item = InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 2,
        template: 100,
        stack_key: 10,
        place: ItemPlace::World,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 5,
        unit_value: 7,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let position = Position {
        obj_cell_id: 0x12340001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    };
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "unload_fixture".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.positions.push(Property {
        id: 1,
        value: position.clone(),
    });
    state.properties.strings.push(Property {
        id: 999,
        value: "preserved".into(),
    });
    let entity = EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state,
    };
    (
        bace_simulation::RegionUnloadItem {
            item,
            transient: false,
            container: None,
            registry_revision: None,
            enchantments: vec![],
            position: Some(bace_interactions::PortalPosition {
                cell: 0x12340001,
                origin: [4., 5., 6.],
                rotation: [1., 0., 0., 0.],
            }),
            corpse: None,
        },
        RegionItemSource {
            item: FrozenInventoryItem {
                source_destination: None,
                corpse: None,
                construction: None,
                entity,
                enchantments: vec![],
                placement: Some(ItemPlacementV2::World(position)),
                persisted_version: 3,
            },
            corpse: None,
        },
    )
}
fn ticket(items: Vec<bace_simulation::RegionUnloadItem>) -> RegionUnloadTicket {
    RegionUnloadTicket {
        operation: 7,
        landblock: 0x1234,
        epoch: 4,
        items,
    }
}
#[test]
fn unload_preserves_corpse_expiry_and_metadata_with_current_accepted_position() {
    let (mut item, mut source) = source(0x80000001);
    item.corpse = Some(bace_world::CorpseState {
        operation: 5,
        source: EntityId(0x50000001),
        template: 100,
        owner: Some(EntityId(0x50000001)),
        items: vec![],
    });
    source.corpse = Some(
        CorpseSaveV5::migrate_v4(CorpseSaveV4 {
            source: Some(0x50000001),
            operation: Some(5),
            previous: CorpseSaveV3 {
                previous: CorpseSaveV2 {
                    corpse: CorpseSaveV1 {
                        entity: source.item.entity.clone(),
                        owner: Some(0x50000001),
                        death_operation: "death:3:5".into(),
                        expires_at: 12345,
                    },
                    placement: source.item.placement.clone().unwrap(),
                },
                enchantments: vec![],
            },
        })
        .unwrap(),
    );
    let t = ticket(vec![item]);
    let sources = [source];
    let pending = freeze_region_unload(9, &t, &sources).unwrap();
    assert_eq!(pending.batch_count(), 1);
    let op = pending.batches[0].operation();
    let saved = CorpseSaveV5::decode(&op.snapshots[0].bytes).unwrap();
    assert_eq!(saved.corpse.expires_at, 12345);
    assert_eq!(saved.corpse.death_operation, "death:3:5");
    assert_eq!(
        saved.corpse.entity.state.properties.strings[0].value,
        "preserved"
    );
    assert!(matches!(saved.placement,ItemPlacementV2::World(ref p) if p.position_x==4.));
    assert_eq!(op.snapshots[0].mutation_revision, 2);
    assert_eq!(op.snapshots[0].expected_version, 3);
    let mut invalid = t.clone();
    invalid.items[0].position.as_mut().unwrap().cell = 0xFFFF0001;
    assert!(freeze_region_unload(9, &invalid, &sources).is_err());
    invalid = t.clone();
    invalid.items[0].corpse = None;
    assert!(freeze_region_unload(9, &invalid, &sources).is_err());
}
#[derive(Clone, Default)]
struct Backend(std::sync::Arc<std::sync::Mutex<Vec<(u64, String)>>>);
impl crate::saves::SaveBackend for Backend {
    async fn routine(
        &self,
        _: &[bace_persistence::SaveSnapshot],
    ) -> Result<Vec<SaveAck>, crate::saves::SaveFailure> {
        unreachable!()
    }
    async fn valuable(
        &self,
        _: &str,
        _: &[bace_persistence::SaveSnapshot],
    ) -> Result<bace_persistence::OperationOutcome, crate::saves::SaveFailure> {
        unreachable!()
    }
    async fn world_placement(
        &self,
        op: &bace_persistence::WorldPlacementOperation,
    ) -> Result<bace_persistence::OperationOutcome, crate::saves::SaveFailure> {
        let mut calls = self.0.lock().unwrap();
        calls.push((op.world_epoch, op.inventory.operation_id.clone()));
        if calls.len() == 1 {
            Err(crate::saves::SaveFailure::Storage {
                message: "lost commit reply".into(),
                uncertain: true,
            })
        } else {
            Ok(bace_persistence::OperationOutcome::AlreadyCommitted)
        }
    }
}
#[tokio::test]
async fn unload_acknowledges_only_after_all_batches_and_retries_exact_uncertain_request() {
    let (items, sources): (Vec<_>, Vec<_>) = (0..1025).map(|i| source(0x80000001 + i)).unzip();
    let t = ticket(items);
    let mut pending = freeze_region_unload(9, &t, &sources).unwrap();
    assert_eq!(pending.batch_count(), 2);
    let backend = Backend::default();
    let worker = crate::saves::spawn_save_worker(backend.clone(), Default::default()).unwrap();
    for attempt in 0..3 {
        pending.submit(&worker.handle).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(r) = pending.poll() {
                    break r;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        match (attempt, result) {
            (0, RegionUnloadResolution::Uncertain(_)) => assert_eq!(pending.committed_batches(), 0),
            (1, RegionUnloadResolution::Progress) => assert_eq!(pending.committed_batches(), 1),
            (
                2,
                RegionUnloadResolution::Committed {
                    receipt,
                    acknowledgments,
                },
            ) => {
                assert_eq!(receipt.revisions.len(), 1025);
                assert_eq!(acknowledgments.len(), 1025);
            }
            (_, other) => panic!("unexpected region outcome: {other:?}"),
        }
    }
    let calls = backend.0.lock().unwrap().clone();
    assert_eq!(calls[0], calls[1]);
    assert_ne!(calls[1], calls[2]);
    assert!(calls.iter().all(|c| c.0 == 9));
    assert!(pending.submit(&worker.handle).is_err());
    drop(worker.handle);
    worker.task.await.unwrap();
}
