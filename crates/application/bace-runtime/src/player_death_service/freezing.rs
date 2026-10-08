use super::*;
use bace_persistence::WorldPlacementOperation;
use bace_storage_codec::{CorpseSaveV5, ItemSaveV2};

pub(super) struct Frozen {
    pub save: PendingPlacementSave,
    pub receipt: bace_simulation::PlayerDeathReceipt,
    pub expires_at: i64,
}
pub(super) fn freeze(
    work: &PlayerDeathWork,
    snapshot: &PlayerReadSnapshot,
    unix: u64,
    online: &OnlinePlayerSaveService,
) -> Result<Frozen, String> {
    let (base, version, lease) = online
        .baseline(work.binding.actor.0)
        .ok_or("missing death baseline")?;
    let before = crate::player_saves::freeze_player_operation_baseline(
        base,
        snapshot,
        PlayerSnapshotOperation::PlayerDeath(work.ticket.operation),
        work.ticket.before_revision,
        unix,
    )
    .map_err(|e| e.to_string())?;
    let mut items = online.operation_inventory_baselines(snapshot)?;
    let mut ids: std::collections::BTreeSet<_> = items.iter().map(|i| i.entity.object_id).collect();
    for fresh in &work.fresh_items {
        if fresh.persisted_version != 0
            || fresh.placement.is_some()
            || !ids.insert(fresh.entity.object_id)
        {
            return Err("invalid fresh death item baseline".into());
        }
        items.push(fresh.clone());
    }
    let frozen = crate::player_death_saves::freeze_player_death(
        crate::player_death_saves::PlayerDeathFreezeInput {
            epoch: work.epoch,
            ticket: &work.ticket,
            player: &before,
            persisted_version: version,
            lease,
            items: &items,
            positions: &work.positions,
            unix_seconds: i64::try_from(unix / 1000).map_err(|_| "death Unix clock overflow")?,
        },
    )?;
    let mut operation = frozen.operation;
    if let Some(plan) = &work.ticket.no_corpse {
        validate_no_corpse_descendants(plan, &items, snapshot.items(), &operation)?;
    }
    // A death checkpoint is also the barrier for already-dirty sibling timers.
    // The item loss proposal alone need not contain an otherwise unchanged item.
    let old = online.inventory_baselines(work.binding.actor.0);
    for item in &items {
        if item.persisted_version == 0
            || operation
                .snapshots
                .iter()
                .any(|s| s.object_id == item.entity.object_id)
        {
            continue;
        }
        let previous = old
            .iter()
            .find(|i| i.entity.object_id == item.entity.object_id)
            .ok_or("missing death item baseline")?;
        if previous.entity == item.entity && previous.enchantments == item.enchantments {
            continue;
        }
        let saved = bace_storage_codec::ItemSaveV5 {
            source_destination: item.source_destination,
            previous: bace_storage_codec::ItemSaveV4 {
                previous: bace_storage_codec::ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity: item.entity.clone(),
                        placement: item
                            .placement
                            .clone()
                            .ok_or("missing death item placement")?,
                    },
                    enchantments: item.enchantments.clone(),
                },
                construction: item.construction.clone(),
            },
        };
        operation.snapshots.push(SaveSnapshot {
            object_id: item.entity.object_id,
            mutation_revision: item.entity.mutation_revision,
            expected_version: item.persisted_version,
            bytes: saved.encode().map_err(|e| e.to_string())?,
        });
        operation.participants.push(item.entity.object_id);
    }
    operation.snapshots.sort_by_key(|s| s.object_id);
    operation.participants.sort_unstable();
    operation.participants.dedup();
    if operation.snapshots.len() > 1024 || operation.participants.len() > 1024 {
        return Err("death placement participant capacity".into());
    }
    Ok(Frozen {
        save: PendingPlacementSave::new_world(WorldPlacementOperation {
            world_epoch: work.epoch,
            inventory: operation,
        })
        .map_err(|e| e.to_string())?,
        receipt: frozen.receipt,
        expires_at: frozen.expires_at,
    })
}
fn validate_no_corpse_descendants(
    plan: &bace_simulation::PlayerNoCorpsePlan,
    items: &[FrozenInventoryItem],
    live_items: &[bace_inventory::InventoryItem],
    operation: &bace_persistence::PlacementOperation,
) -> Result<(), String> {
    let mut admitted = plan
        .world_roots
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    if admitted.len() != plan.world_roots.len() {
        return Err("NoCorpse descendant root identity".into());
    }
    for descendant in &plan.descendants {
        if !admitted.contains(&descendant.parent) || !admitted.insert(descendant.id) {
            return Err("NoCorpse descendant ancestry/identity".into());
        }
        let item = items
            .iter()
            .find(|item| item.entity.object_id == descendant.id.0)
            .ok_or("NoCorpse descendant baseline missing")?;
        let place = bace_storage_codec::ItemPlacementV2::Contained {
            container: descendant.parent.0,
            slot: descendant.slot,
            pack_slot: descendant.pack_slot,
            equipped: 0,
        };
        let live = live_items
            .iter()
            .find(|live| live.id == descendant.id)
            .ok_or("NoCorpse descendant snapshot missing")?;
        let row = operation
            .snapshots
            .iter()
            .find(|row| row.object_id == descendant.id.0)
            .ok_or("NoCorpse descendant receipt row missing")?;
        let saved = bace_storage_codec::ItemSaveV5::decode(&row.bytes)
            .map_err(|e| format!("NoCorpse descendant receipt decode: {e}"))?;
        if item.persisted_version <= 0
            || item.entity.mutation_revision.checked_add(1) != Some(descendant.revision)
            || item.placement.as_ref() != Some(&place)
            || live.revision != item.entity.mutation_revision
            || live.template != item.entity.state.weenie_id
            || live.place
                != (bace_inventory::ItemPlace::Contained {
                    container: descendant.parent,
                    slot: descendant.slot,
                    equipped: 0,
                })
            || live.pack_slot != descendant.pack_slot
            || !operation.participants.contains(&descendant.id.0)
            || row.expected_version != item.persisted_version
            || row.mutation_revision != descendant.revision
            || saved.entity.object_id != descendant.id.0
            || saved.entity.mutation_revision != descendant.revision
            || saved.entity.template_revision != item.entity.template_revision
            || saved.entity.state.weenie_id != item.entity.state.weenie_id
            || saved.enchantments != item.enchantments
            || saved.construction != item.construction
            || saved.placement != place
            || saved.source_destination != item.source_destination
        {
            return Err("NoCorpse descendant source mismatch".into());
        }
    }
    Ok(())
}
/// Only the online ownership graph sees this item-shaped corpse view. SQL keeps
/// the complete frozen CorpseSaveV5, including access and expiry metadata.
pub(super) fn online_rows(rows: &[SaveSnapshot], corpse: u32) -> Result<Vec<SaveSnapshot>, String> {
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            if row.object_id == corpse {
                let source = CorpseSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
                let item = bace_storage_codec::ItemSaveV5 {
                    source_destination: None,
                    previous: bace_storage_codec::ItemSaveV4 {
                        previous: bace_storage_codec::ItemSaveV3 {
                            previous: ItemSaveV2 {
                                entity: source.corpse.entity.clone(),
                                placement: source.placement.clone(),
                            },
                            enchantments: source.enchantments.clone(),
                        },
                        construction: None,
                    },
                };
                row.bytes = item.encode().map_err(|e| e.to_string())?;
            }
            Ok(row)
        })
        .collect()
}

#[cfg(test)]
mod no_corpse_tests {
    use super::*;
    use bace_inventory::{InventoryItem, ItemPlace};
    use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV5};
    use bace_types::EntityId;

    #[test]
    fn retained_container_child_has_same_operation_v5_cas_and_exact_slot() {
        let root = EntityId(0x80000011);
        let child = EntityId(0x80000012);
        let plan = bace_simulation::PlayerNoCorpsePlan {
            world_roots: vec![root],
            descendants: vec![bace_simulation::NoCorpseDescendant {
                id: child,
                parent: root,
                slot: 2,
                pack_slot: true,
                revision: 8,
            }],
            accepted_position: Default::default(),
        };
        let placement = ItemPlacementV2::Contained {
            container: root.0,
            slot: 2,
            pack_slot: true,
            equipped: 0,
        };
        let frozen = FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: Some(1),
            entity: EntitySaveV1 {
                object_id: child.0,
                template_revision: 1,
                mutation_revision: 7,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 100,
                    class_name: "held child".into(),
                    weenie_type: 1,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            placement: Some(placement.clone()),
            persisted_version: 4,
            enchantments: vec![],
        };
        let live = InventoryItem {
            id: child,
            revision: 7,
            template: 100,
            stack_key: 1,
            place: ItemPlace::Contained {
                container: root,
                slot: 2,
                equipped: 0,
            },
            stack: 1,
            maximum_stack: 1,
            unit_burden: 1,
            unit_value: 1,
            pack_slot: true,
            is_container: false,
            attuned: false,
            trade_reserved: false,
            active_pet: false,
            unique: false,
            quest_allowed: true,
            valid_wield: 0,
            incompatible_wield: 0,
            wield_requirements_met: false,
            structure: None,
        };
        let mut after = frozen.entity.clone();
        after.mutation_revision = 8;
        let saved = ItemSaveV5 {
            source_destination: frozen.source_destination,
            previous: bace_storage_codec::ItemSaveV4 {
                previous: bace_storage_codec::ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity: after,
                        placement: placement.clone(),
                    },
                    enchantments: vec![],
                },
                construction: None,
            },
        };
        let operation = || bace_persistence::PlacementOperation {
            operation_id: "player-no-corpse-container".into(),
            snapshots: vec![SaveSnapshot {
                object_id: child.0,
                mutation_revision: 8,
                expected_version: 4,
                bytes: saved.encode().unwrap(),
            }],
            participants: vec![child.0],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        };
        let accepted = operation();
        validate_no_corpse_descendants(
            &plan,
            std::slice::from_ref(&frozen),
            std::slice::from_ref(&live),
            &accepted,
        )
        .unwrap();
        assert_eq!(accepted.participants, vec![child.0]);
        assert_eq!(accepted.snapshots[0].expected_version, 4);
        assert_eq!(accepted.snapshots[0].mutation_revision, 8);
        assert_eq!(
            ItemSaveV5::decode(&accepted.snapshots[0].bytes).unwrap(),
            saved
        );
        let mut wrong_slot = live.clone();
        wrong_slot.place = ItemPlace::Contained {
            container: root,
            slot: 3,
            equipped: 0,
        };
        assert!(
            validate_no_corpse_descendants(&plan, &[frozen], &[wrong_slot], &accepted).is_err()
        );
    }
}
