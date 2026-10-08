use super::*;
use crate::saves::{SaveBackend, SaveWorkerConfig, spawn_save_worker};
use bace_persistence::OperationOutcome;
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV2, PlayerSaveV1};
use bace_types::{AccountId, EntityId};
use std::sync::{Arc, Mutex};
fn saved(id: u32, revision: u64) -> PlayerSaveV6 {
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: entity(id, revision),
        account_id: 1,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
fn entity(id: u32, revision: u64) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: revision,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "routine_test".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn service() -> OnlinePlayerSaveService {
    let mut s =
        OnlinePlayerSaveService::new(4, 1024 * 1024, Instant::now() - Duration::from_secs(60))
            .unwrap();
    s.register(
        CharacterBinding {
            session: bace_gameplay_api::SessionId(1),
            account: AccountId(1),
            actor: EntityId(0x50000007),
        },
        CharacterLease {
            character_id: 0x50000007,
            epoch: 3,
            state: OwnershipState::Online,
        },
        saved(0x50000007, 1),
        1,
        Duration::ZERO,
    )
    .unwrap();
    s
}
fn snapshot(id: u32, revision: u64, version: i64) -> SaveSnapshot {
    SaveSnapshot {
        object_id: id,
        mutation_revision: revision,
        expected_version: version,
        bytes: saved(id, revision).encode().unwrap(),
    }
}
#[derive(Clone)]
struct Backend {
    seen: Arc<Mutex<Vec<OwnedSaveBatch>>>,
    gate: Arc<tokio::sync::Semaphore>,
}
impl SaveBackend for Backend {
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("unfenced save must never run")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("routine must not use critical lane")
    }
    async fn owned_batch(&self, batch: &OwnedSaveBatch) -> Result<Vec<SaveAck>, SaveFailure> {
        self.seen.lock().unwrap().push(batch.clone());
        self.gate.acquire().await.unwrap().forget();
        Ok(batch
            .snapshots
            .iter()
            .map(|s| SaveAck {
                object_id: s.object_id,
                mutation_revision: s.mutation_revision,
                persisted_version: s.expected_version + 1,
            })
            .collect())
    }
}
#[tokio::test]
async fn exact_owned_batch_blocks_critical_and_preserves_later_notice() {
    let mut s = service();
    s.dirty
        .mark_at(snapshot(0x50000007, 2, 1), Duration::ZERO)
        .unwrap();
    let backend = Backend {
        seen: Default::default(),
        gate: Arc::new(tokio::sync::Semaphore::new(0)),
    };
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    assert_eq!(
        s.submit_due(&worker.handle, Duration::from_secs(5), false, 1)
            .unwrap(),
        1
    );
    assert!(s.begin_critical(&[0x50000007]).is_err());
    s.mark_dirty(0x50000007, Duration::from_secs(5)).unwrap();
    backend.gate.add_permits(1);
    for _ in 0..100 {
        if s.poll_writes(1).unwrap() == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(s.baseline(0x50000007).unwrap().1, 2);
    assert!(
        s.requires_drain(),
        "newer notice survives older acknowledgment"
    );
    {
        let log = backend.seen.lock().unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].participants, vec![0x50000007]);
        assert_eq!(log[0].leases[0].epoch, 3);
    }
    worker.handle.close();
    worker.task.await.unwrap();
}
#[test]
fn mismatched_batch_ack_retains_every_dirty_row_and_critical_handoff_adds_items() {
    let mut s = service();
    let request = snapshot(0x50000007, 2, 1);
    s.dirty.mark_at(request.clone(), Duration::ZERO).unwrap();
    s.dirty.drain_ready();
    s.writes.insert(
        0x50000007,
        PendingWrite {
            batch: OwnedSaveBatch {
                snapshots: vec![request],
                participants: vec![0x50000007],
                leases: vec![s.players[&0x50000007].lease],
            },
            ticket: None,
            uncertain: true,
        },
    );
    assert!(
        s.accept_acks(
            0x50000007,
            &[SaveAck {
                object_id: 0x50000007,
                mutation_revision: 3,
                persisted_version: 2
            }]
        )
        .is_err()
    );
    assert_eq!(s.baseline(0x50000007).unwrap().1, 1);
    assert!(s.begin_critical(&[0x50000007]).is_err());
    s.accept_acks(
        0x50000007,
        &[SaveAck {
            object_id: 0x50000007,
            mutation_revision: 2,
            persisted_version: 2,
        }],
    )
    .unwrap();
    s.begin_critical(&[0x50000007]).unwrap();
    let mut bag = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: entity(0x80000008, 1),
        placement: ItemPlacementV2::Contained {
            container: 0x50000007,
            slot: 0,
            pack_slot: true,
            equipped: 0,
        },
    })
    .unwrap();
    bag.entity
        .state
        .properties
        .strings
        .push(bace_content::Property {
            id: 999,
            value: "unknown metadata retained".into(),
        });
    s.finish_critical(&[
        snapshot(0x50000007, 3, 3),
        SaveSnapshot {
            object_id: 0x80000008,
            mutation_revision: 1,
            expected_version: 1,
            bytes: bag.encode().unwrap(),
        },
    ])
    .unwrap();
    assert_eq!(s.items[&0x80000008].saved, bag);
    assert!(s.dirty.is_clean());
    s.begin_critical(&[0x50000007]).unwrap();
    let mut removed = bag;
    removed.entity.mutation_revision = 2;
    removed.placement = ItemPlacementV2::Removed;
    s.finish_critical(&[
        snapshot(0x50000007, 4, 4),
        SaveSnapshot {
            object_id: 0x80000008,
            mutation_revision: 2,
            expected_version: 2,
            bytes: removed.encode().unwrap(),
        },
    ])
    .unwrap();
    assert!(!s.items.contains_key(&0x80000008));
    assert!(s.dirty.is_clean());
}
#[test]
fn item_expiry_keeps_unknown_metadata_and_requires_current_dirty_revision() {
    let actor = 0x50000007;
    let id = 0x80000008;
    let mut s = service();
    let mut item = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: entity(id, 1),
        placement: ItemPlacementV2::Contained {
            container: actor,
            slot: 3,
            pack_slot: false,
            equipped: 0,
        },
    })
    .unwrap();
    item.entity
        .state
        .properties
        .strings
        .push(bace_content::Property {
            id: 999,
            value: "preserved unknown value".into(),
        });
    item.enchantments
        .push(bace_storage_codec::FrozenEnchantmentV1 {
            schema_version: 1,
            enchantment_category: 12,
            spell_id: 101,
            layer_id: 7,
            has_spell_set_id: false,
            spell_category: 99,
            power_level: 1,
            start_time: -15.,
            duration: 60.,
            caster_object_id: actor,
            degrade_modifier: 9.,
            degrade_limit: 8.,
            last_time_degraded: -7.,
            stat_mod_type: 0,
            stat_mod_key: 88,
            stat_mod_value: 12.5,
            spell_set_id: 42,
        });
    s.register_inventory(actor, vec![(item.clone(), 1)])
        .unwrap();
    let mut current = bace_inventory::InventoryItem {
        id: EntityId(id),
        revision: 2,
        template: 1,
        structure: None,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(actor),
            slot: 3,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 0,
        unit_value: 0,
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
    let empty = bace_magic::EnchantmentRegistry::new(4).unwrap();
    let registries = vec![(EntityId(id), empty)];
    let rows =
        inventory::freeze_item_views(actor, &s.items, std::slice::from_ref(&current), &registries)
            .unwrap();
    let after = bace_storage_codec::ItemSaveV5::decode(&rows[0].bytes).unwrap();
    assert!(after.enchantments.is_empty());
    assert_eq!(after.entity.state, item.entity.state);
    assert_eq!(after.entity.mutation_revision, 2);
    current.revision = 1;
    assert!(inventory::freeze_item_views(actor, &s.items, &[current], &registries).is_err());
    assert_eq!(
        s.items[&id].saved, item,
        "failed preparation preserves durable baseline"
    );
}

#[test]
fn routine_item_snapshot_joins_mana_and_deactivation_at_exact_revision() {
    let actor = 0x50000007;
    let id = 0x80000008;
    let mut service = service();
    let mut saved = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: entity(id, 1),
        placement: ItemPlacementV2::Contained {
            container: actor,
            slot: 0,
            pack_slot: false,
            equipped: 1,
        },
    })
    .unwrap();
    saved
        .entity
        .state
        .properties
        .ints
        .push(bace_content::Property { id: 107, value: 3 });
    saved
        .entity
        .state
        .properties
        .bools
        .push(bace_content::Property {
            id: 56,
            value: true,
        });
    service
        .register_inventory(actor, vec![(saved.clone(), 1)])
        .unwrap();
    let mut current = bace_inventory::InventoryItem {
        id: EntityId(id),
        revision: 2,
        template: 1,
        structure: None,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(actor),
            slot: 0,
            equipped: 1,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 0,
        unit_value: 0,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 1,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    let mana = bace_simulation::EquipmentManaRecovery {
        fresh: false,
        heartbeat: 5.,
        rating: 0,
        heartbeat_remaining: 3.,
        items: vec![bace_simulation::EquipmentManaItem {
            item: EntityId(id),
            name: "ring".into(),
            current: Some(0),
            maximum: Some(3),
            rate: Some(-1.),
            affecting: None,
            removals: vec![],
            accumulator: 2.,
            warned: true,
            removal_remaining: Some(1.),
        }],
    };
    let frozen = inventory::freeze_item_views_with_mana(
        actor,
        &service.items,
        std::slice::from_ref(&current),
        &[],
        Some(&mana),
    )
    .unwrap();
    assert_eq!(frozen.len(), 1);
    let after = bace_storage_codec::ItemSaveV5::decode(&frozen[0].bytes).unwrap();
    assert_eq!(after.entity.mutation_revision, 2);
    assert_eq!(
        after
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 107)
            .unwrap()
            .value,
        0
    );
    assert!(
        !after
            .entity
            .state
            .properties
            .bools
            .iter()
            .any(|p| p.id == 56)
    );
    assert_eq!(
        mana.items[0].accumulator, 2.,
        "snapshot cannot reset source transient owner"
    );
    assert_eq!(
        service.items[&id].saved, saved,
        "freezing cannot mark baseline clean"
    );
    current.revision = 1;
    assert!(
        inventory::freeze_item_views_with_mana(actor, &service.items, &[current], &[], Some(&mana))
            .is_err()
    );
}
