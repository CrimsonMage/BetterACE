use bace_content::{Position, Property, WeenieV1};
use bace_gameplay_api::*;
use bace_inventory::*;
use bace_persistence::{
    ConstructedCreaturePromotionOperation, OperationOutcome, SaveAck, SaveSnapshot,
    WorldPlacementOperation,
};
use bace_runtime::{game_inventory::*, generated_retirement::*, saves::*};
use bace_simulation::{GeneratedRetirementTicket, InventoryTicket};
use bace_storage_codec::{
    EntitySaveV1, FrozenCreatureConstructionV1, FrozenGeneratorConstructionOriginV1,
    ItemPlacementV2, ItemSaveV5,
};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
#[allow(dead_code)]
#[path = "generator_delivery/fixture.rs"]
mod generator_fixture;

#[derive(Clone, Default)]
struct Backend {
    requests: Arc<Mutex<Vec<WorldPlacementOperation>>>,
    constructed: Arc<Mutex<Vec<ConstructedCreaturePromotionOperation>>>,
    results: Arc<Mutex<VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn constructed_creature_promotion(
        &self,
        operation: &ConstructedCreaturePromotionOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.constructed.lock().unwrap().push(operation.clone());
        Ok(OperationOutcome::Committed(
            operation
                .inventory
                .snapshots
                .iter()
                .map(|snapshot| SaveAck {
                    object_id: snapshot.object_id,
                    mutation_revision: snapshot.mutation_revision,
                    persisted_version: snapshot.expected_version + 1,
                })
                .collect(),
        ))
    }
    async fn world_placement(
        &self,
        operation: &WorldPlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.requests.lock().unwrap().push(operation.clone());
        match self.results.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Ok(OperationOutcome::AlreadyCommitted),
            3 => Err(SaveFailure::Storage {
                message: "CAS rejected".into(),
                uncertain: false,
            }),
            4 => Ok(OperationOutcome::Committed(vec![])),
            _ => Ok(OperationOutcome::Committed(
                operation
                    .inventory
                    .snapshots
                    .iter()
                    .map(|s| SaveAck {
                        object_id: s.object_id,
                        mutation_revision: s.mutation_revision,
                        persisted_version: s.expected_version + 1,
                    })
                    .collect(),
            )),
        }
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("world tombstones must use epoch-fenced atomic operation")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("world tombstones must not use unfenced valuable")
    }
}
fn fixture() -> (GeneratedRetirementTicket, Vec<FrozenInventoryItem>) {
    let id = EntityId(0x80000901);
    let before = InventoryItem {
        structure: None,
        id,
        revision: 2,
        template: 100,
        stack_key: 0,
        place: ItemPlace::World,
        stack: 7,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
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
    let mut after = before.clone();
    after.place = ItemPlace::Removed;
    after.revision = 3;
    let ticket = GeneratedRetirementTicket {
        npc_source_ticket: None,
        positions: Default::default(),
        enchantments: Default::default(),
        registry_revisions: Default::default(),
        inventory: InventoryTicket {
            operation: 7,
            actor: EntityId(100),
            proposal: InventoryProposal {
                changes: vec![ItemChange {
                    before: Some(before),
                    after,
                }],
                participants: vec![(id, 2)],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
        effect: GeneratorLifecycleEffect::DestroyMember {
            generator: generator_fixture::definition().identity,
            member: GeneratorSpawnMember {
                entity: id,
                contribution: 1,
            },
            recursive: true,
            include_dead: true,
            from_unload: false,
        },
        transient: vec![],
    };
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "generated_tombstone".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.strings.push(Property {
        id: 16,
        value: "source description retained".into(),
    });
    state
        .properties
        .instance_ids
        .push(Property { id: 6, value: 100 });
    let items = vec![FrozenInventoryItem {
        source_destination: None,
        corpse: None,
        construction: None,
        enchantments: vec![],
        entity: EntitySaveV1 {
            object_id: id.0,
            template_revision: 5,
            mutation_revision: 2,
            state,
        },
        placement: Some(ItemPlacementV2::World(Position {
            obj_cell_id: 0x01010001,
            position_x: 1.,
            position_y: 2.,
            position_z: 3.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        })),
        persisted_version: 1,
    }];
    (ticket, items)
}
fn frozen() -> PendingGeneratedRetirement {
    let (ticket, items) = fixture();
    let proposal = ticket.inventory.proposal.clone();
    PendingGeneratedRetirement::freeze(
        ticket,
        InventoryFreezeInput {
            operation_id: "retire-world-44-op-7",
            proposal: &proposal,
            items: &items,
            other_snapshots: &[],
            leases: &[],
            storage_views: &[],
            admitted_positions: &BTreeMap::new(),
        },
        44,
    )
    .unwrap()
}
async fn resolution(pending: &mut PendingGeneratedRetirement) -> GeneratedRetirementResolution {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(result) = pending.poll() {
                return result;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn uncertain_tombstone_retries_exact_epoch_id_and_bytes_until_committed_journal() {
    let mut pending = frozen();
    let backend = Backend {
        results: Arc::new(Mutex::new(VecDeque::from([1, 3, 4, 2]))),
        ..Default::default()
    };
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    for _ in 0..3 {
        pending.submit(&worker.handle).unwrap();
        assert!(pending.submit(&worker.handle).is_err());
        assert!(matches!(
            resolution(&mut pending).await,
            GeneratedRetirementResolution::Uncertain { .. }
        ));
    }
    pending.submit(&worker.handle).unwrap();
    let GeneratedRetirementResolution::Committed {
        receipt,
        acknowledgments,
    } = resolution(&mut pending).await
    else {
        panic!("journal resolution");
    };
    assert_eq!(receipt.operation, 7);
    assert_eq!(receipt.revisions, vec![(EntityId(0x80000901), 3)]);
    assert_eq!(acknowledgments[0].persisted_version, 2);
    assert!(pending.submit(&worker.handle).is_err());
    assert!(pending.poll().is_none());
    {
        let requests = backend.requests.lock().unwrap();
        assert_eq!(requests.len(), 4);
        for request in requests.iter() {
            assert_eq!(request.world_epoch, requests[0].world_epoch);
            assert_eq!(
                request.inventory.operation_id,
                requests[0].inventory.operation_id
            );
            assert_eq!(request.inventory.snapshots, requests[0].inventory.snapshots);
            assert_eq!(
                request.inventory.participants,
                requests[0].inventory.participants
            );
            assert_eq!(request.inventory.leases, requests[0].inventory.leases);
            assert_eq!(request.inventory.changes, requests[0].inventory.changes);
        }
        assert_eq!(requests[0].world_epoch, 44);
        assert!(requests[0].inventory.leases.is_empty());
        assert_eq!(
            requests[0].inventory.changes[0].destination,
            bace_persistence::DurableItemPlace::Removed
        );
        let saved =
            ItemSaveV5::decode_or_migrate(&requests[0].inventory.snapshots[0].bytes, None).unwrap();
        assert_eq!(saved.placement, ItemPlacementV2::Removed);
        assert_eq!(
            saved.entity.state.properties.strings[0].value,
            "source description retained"
        );
    }
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn definite_tombstone_rejection_is_terminal_and_never_becomes_success() {
    let mut pending = frozen();
    let backend = Backend {
        results: Arc::new(Mutex::new(VecDeque::from([3]))),
        ..Default::default()
    };
    let worker = spawn_save_worker(backend, SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    assert!(matches!(
        resolution(&mut pending).await,
        GeneratedRetirementResolution::Rejected { operation: 7, .. }
    ));
    assert!(pending.submit(&worker.handle).is_err());
    assert!(pending.poll().is_none());
    worker.handle.close();
    worker.task.await.unwrap();
}
#[test]
fn tombstone_freeze_rejects_stale_snapshot_wrong_proposal_and_invalid_epoch() {
    let (ticket, items) = fixture();
    let positions = BTreeMap::new();
    let proposal = ticket.inventory.proposal.clone();
    let input = |items| InventoryFreezeInput {
        operation_id: "retire-test",
        proposal: &proposal,
        items,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: &positions,
    };
    assert!(PendingGeneratedRetirement::freeze(ticket.clone(), input(&items), 0).is_err());
    assert!(PendingGeneratedRetirement::freeze(ticket.clone(), input(&items), u64::MAX).is_err());
    let mut wrong = ticket.clone();
    wrong.inventory.proposal.changes[0].after.revision += 1;
    assert!(PendingGeneratedRetirement::freeze(wrong, input(&items), 44).is_err());
    let mut stale = items.clone();
    stale[0].entity.mutation_revision -= 1;
    assert!(PendingGeneratedRetirement::freeze(ticket, input(&stale), 44).is_err());
}
#[test]
fn generator_requests_survive_full_worker_lane_and_recover_exact_retry_identity() {
    use bace_runtime::simulation::{SimulationConfig, SimulationWorker};
    let mut kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    kernel
        .configure_generators(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
            true,
        )
        .unwrap();
    let mut definition = generator_fixture::definition();
    definition.identity.entity = EntityId(100);
    definition.initial_count = 5;
    definition.maximum_count = 5;
    definition.profiles[0].init_create = 5;
    definition.profiles[0].max_create = 5;
    definition.profiles[0].where_create = 8;
    kernel.register_generator(Arc::new(definition)).unwrap();
    for id in 0x80000a01..=0x80000a05 {
        kernel.supply_generator_id(EntityId(id)).unwrap();
    }
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 2,
            tick_limit: Some(20),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut exit = worker.wait_recover().unwrap();
    assert!(exit.failure.is_none());
    let mut requests = std::mem::take(&mut exit.undelivered_generator_requests);
    while let Some(request) = exit.kernel.take_generator_request() {
        requests.push(request);
    }
    assert_eq!(requests.len(), 5);
    let unique: std::collections::BTreeSet<_> = requests
        .iter()
        .flat_map(|request| request.entities.iter().copied())
        .collect();
    assert_eq!(unique.len(), 5);
    for request in requests {
        exit.kernel
            .retry_generator_request(request.intent.key)
            .unwrap();
        assert_eq!(exit.kernel.take_generator_request(), Some(request));
    }
}

#[tokio::test]
async fn generated_acquisition_keeps_exact_transient_set_through_uncertain_receipts() {
    use bace_runtime::generated_saves::{GeneratedSaveResolution, PendingGeneratedSave};
    let (retirement, mut items) = fixture();
    let mut ticket = retirement.inventory;
    ticket.actor = EntityId(1);
    ticket.proposal.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(1),
        slot: 0,
        equipped: 0,
    };
    items[0].persisted_version = 0;
    let proposal = ticket.proposal.clone();
    let leases = [bace_persistence::CharacterLease {
        character_id: 1,
        epoch: 3,
        state: bace_persistence::OwnershipState::Online,
    }];
    let transient = [EntityId(0x80000901)];
    let mut pending = PendingGeneratedSave::freeze(
        ticket,
        &transient,
        InventoryFreezeInput {
            operation_id: "acquire-world-44-op-7",
            proposal: &proposal,
            items: &items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &BTreeMap::new(),
        },
        44,
    )
    .unwrap();
    let backend = Backend {
        results: Arc::new(Mutex::new(VecDeque::from([4, 3, 2]))),
        ..Default::default()
    };
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    for attempt in 0..3 {
        pending.submit(&worker.handle).unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Some(result) = pending.poll() {
                    break result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        if attempt < 2 {
            assert!(matches!(result, GeneratedSaveResolution::Uncertain { .. }));
        } else {
            let GeneratedSaveResolution::Committed {
                receipt,
                transient: received,
                acknowledgments,
            } = result
            else {
                panic!("exact committed acquisition");
            };
            assert_eq!(receipt.revisions, vec![(transient[0], 3)]);
            assert_eq!(received, transient);
            assert_eq!(acknowledgments[0].persisted_version, 1);
        }
    }
    assert!(pending.submit(&worker.handle).is_err());
    {
        let requests = backend.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        for request in requests.iter() {
            assert_eq!(request.world_epoch, requests[0].world_epoch);
            assert_eq!(
                request.inventory.operation_id,
                requests[0].inventory.operation_id
            );
            assert_eq!(request.inventory.snapshots, requests[0].inventory.snapshots);
            assert_eq!(
                request.inventory.participants,
                requests[0].inventory.participants
            );
            assert_eq!(request.inventory.leases, requests[0].inventory.leases);
            assert_eq!(request.inventory.changes, requests[0].inventory.changes);
        }
        assert_eq!(requests[0].inventory.changes[0].expected, None);
        let saved =
            ItemSaveV5::decode_or_migrate(&requests[0].inventory.snapshots[0].bytes, None).unwrap();
        assert!(
            saved
                .entity
                .state
                .properties
                .instance_ids
                .iter()
                .all(|property| property.id != 6)
        );
    }
    worker.handle.close();
    worker.task.await.unwrap();
}

#[tokio::test]
async fn generated_constructed_creature_uses_graph_checked_save_lane() {
    use bace_runtime::generated_saves::{GeneratedSaveResolution, PendingGeneratedSave};
    let (retirement, mut items) = fixture();
    let mut ticket = retirement.inventory;
    ticket.actor = EntityId(1);
    ticket.proposal.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(1),
        slot: 0,
        equipped: 0,
    };
    items[0].persisted_version = 0;
    items[0].entity.state.weenie_type = 10;
    items[0].construction = Some(FrozenCreatureConstructionV1 {
        weenie_type: 10,
        origin: FrozenGeneratorConstructionOriginV1 {
            generator: 900,
            incarnation: 1,
            content_revision: 1,
            profile: 0,
            occurrence: 1,
            random_identity: [2; 16],
            random_key_version: 1,
        },
        equipment_order: vec![],
        death_roster: vec![],
    });
    let proposal = ticket.proposal.clone();
    let leases = [bace_persistence::CharacterLease {
        character_id: 1,
        epoch: 3,
        state: bace_persistence::OwnershipState::Online,
    }];
    let mut pending = PendingGeneratedSave::freeze(
        ticket,
        &[EntityId(0x80000901)],
        InventoryFreezeInput {
            operation_id: "constructed-promotion-44-op-7",
            proposal: &proposal,
            items: &items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &BTreeMap::new(),
        },
        44,
    )
    .unwrap();
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    pending.submit(&worker.handle).unwrap();
    let resolution = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(value) = pending.poll() {
                break value;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(
        resolution,
        GeneratedSaveResolution::Committed { .. }
    ));
    assert!(backend.requests.lock().unwrap().is_empty());
    {
        let constructed = backend.constructed.lock().unwrap();
        assert_eq!(constructed.len(), 1);
        assert_eq!(constructed[0].world_epoch, 44);
        assert_eq!(constructed[0].creature_roots, vec![0x80000901]);
        assert_eq!(
            constructed[0].inventory.operation_id,
            "constructed-promotion-44-op-7"
        );
    }
    worker.handle.close();
    worker.task.await.unwrap();
}

#[test]
fn generator_link_tracks_final_ownership_for_partial_split_durable_pickup_and_nested_bag() {
    let (retirement, mut items) = fixture();
    let mut proposal = retirement.inventory.proposal;
    let id = proposal.changes[0].after.id;
    let unowned = ItemPlace::Contained {
        container: EntityId(30),
        slot: 0,
        equipped: 0,
    };
    proposal.changes[0].before.as_mut().unwrap().place = unowned;
    proposal.changes[0].before.as_mut().unwrap().stack = 10;
    proposal.changes[0].after.place = unowned;
    items[0].placement = Some(ItemPlacementV2::Contained {
        container: 30,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    });
    items[0].persisted_version = 0;
    let leases = [bace_persistence::CharacterLease {
        character_id: 1,
        epoch: 3,
        state: bace_persistence::OwnershipState::Online,
    }];
    let positions = BTreeMap::new();
    let freeze = |proposal, items, transient: bool| {
        let input = InventoryFreezeInput {
            operation_id: "ancestry-test",
            proposal,
            items,
            other_snapshots: &[],
            leases: &leases,
            storage_views: &[],
            admitted_positions: &positions,
        };
        let operation = if transient {
            freeze_generated_inventory(input, &[id.0], 44)
                .unwrap()
                .inventory
        } else {
            freeze_inventory(input).unwrap()
        };
        ItemSaveV5::decode_or_migrate(&operation.snapshots[0].bytes, None).unwrap()
    };
    let remaining = freeze(&proposal, &items, true);
    assert!(
        remaining
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .any(|property| property.id == 6)
    );
    let mut pickup = proposal.clone();
    pickup.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(1),
        slot: 0,
        equipped: 0,
    };
    let mut durable = items.clone();
    durable[0].persisted_version = 1;
    let acquired = freeze(&pickup, &durable, false);
    assert!(
        acquired
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .all(|property| property.id != 6)
    );
    let bag = 0x80000902;
    let mut nested = pickup.clone();
    nested.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(bag),
        slot: 0,
        equipped: 0,
    };
    let mut ancestor = durable[0].clone();
    ancestor.entity.object_id = bag;
    ancestor.placement = Some(ItemPlacementV2::Contained {
        container: 1,
        slot: 0,
        pack_slot: true,
        equipped: 0,
    });
    let mut nested_items = durable.clone();
    nested_items.push(ancestor);
    let acquired = freeze(&nested, &nested_items, false);
    assert!(
        acquired
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .all(|property| property.id != 6)
    );
}

#[test]
fn fresh_split_item_cannot_inherit_source_generator_and_cyclic_ancestry_is_rejected() {
    let (retirement, mut items) = fixture();
    let mut proposal = retirement.inventory.proposal;
    proposal.changes[0].before = None;
    proposal.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(30),
        slot: 0,
        equipped: 0,
    };
    items[0].placement = None;
    items[0].persisted_version = 0;
    let positions = BTreeMap::new();
    let saved = freeze_inventory(InventoryFreezeInput {
        operation_id: "fresh-split",
        proposal: &proposal,
        items: &items,
        other_snapshots: &[],
        leases: &[],
        storage_views: &[],
        admitted_positions: &positions,
    })
    .unwrap();
    let saved = ItemSaveV5::decode_or_migrate(&saved.snapshots[0].bytes, None).unwrap();
    assert!(
        saved
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .all(|property| property.id != 6)
    );
    let (retirement, mut items) = fixture();
    let mut proposal = retirement.inventory.proposal;
    let id = proposal.changes[0].after.id;
    let bag = 0x80000902;
    proposal.changes[0].after.place = ItemPlace::Contained {
        container: EntityId(bag),
        slot: 0,
        equipped: 0,
    };
    let mut parent = items[0].clone();
    parent.entity.object_id = bag;
    parent.placement = Some(ItemPlacementV2::Contained {
        container: id.0,
        slot: 0,
        pack_slot: true,
        equipped: 0,
    });
    items.push(parent);
    assert!(
        freeze_inventory(InventoryFreezeInput {
            operation_id: "cycle",
            proposal: &proposal,
            items: &items,
            other_snapshots: &[],
            leases: &[],
            storage_views: &[],
            admitted_positions: &positions
        })
        .is_err()
    );
}

#[test]
fn supplemental_snapshots_reject_unacknowledgeable_versions_before_submission() {
    let (retirement, items) = fixture();
    let positions = BTreeMap::new();
    for expected_version in [-1, i64::MAX] {
        let supplemental = [SaveSnapshot {
            object_id: 1,
            mutation_revision: 3,
            expected_version,
            bytes: vec![],
        }];
        assert!(matches!(
            freeze_inventory(InventoryFreezeInput {
                operation_id: "invalid-supplemental-version",
                proposal: &retirement.inventory.proposal,
                items: &items,
                other_snapshots: &supplemental,
                leases: &[],
                storage_views: &[],
                admitted_positions: &positions,
            }),
            Err(InventoryFreezeError::Identity)
        ));
    }
}
