//! Canonical private ViewContents/CloseGroundContainer after simulation adoption.
use super::*;
use crate::player_entry::{prepare_entry_object, prepare_item_model};
use bace_replication::{BatchLimits, InventoryProjection as P, Sequences};
use bace_storage_codec::{ItemSaveV2, ItemSaveV3, ItemSaveV4};
use bace_wire::{ContainerEntry, InventoryEvent, ObjectCodecLimits};
use std::collections::{BTreeMap, BTreeSet};

/// Pinned ACE Container.SendInventory sends CreateObject for every visible
/// direct child and one nested level on every Open. An already known object
/// keeps its existing canonical sequence owner; only new IDs need admission.
fn content_creation_plan(
    ids: &[EntityId],
    existing: &BTreeMap<EntityId, Sequences>,
) -> Result<Vec<(EntityId, bool)>, String> {
    if ids.len() > 1024 {
        return Err("corpse open content count".into());
    }
    let mut seen = BTreeSet::new();
    let mut plan = Vec::with_capacity(ids.len());
    let mut fresh = 0usize;
    for &id in ids {
        if id.0 == 0 || !seen.insert(id) {
            return Err("corpse open content identity".into());
        }
        let new = !existing.contains_key(&id);
        fresh += usize::from(new);
        plan.push((id, new));
    }
    if existing
        .len()
        .checked_add(fresh)
        .is_none_or(|count| count > 1023)
    {
        return Err("corpse child canonical sequence capacity".into());
    }
    Ok(plan)
}

impl GameRuntime {
    pub(super) fn project_corpse_access(&mut self, pending: &Pending) -> Result<bool, String> {
        if let Some(rejection) = pending.rejection {
            let code = match rejection {
                bace_simulation::CorpseAccessError::OutOfRange => 61,
                bace_simulation::CorpseAccessError::Obstructed => 57,
                bace_simulation::CorpseAccessError::Missing => 55,
                bace_simulation::CorpseAccessError::Capacity => 29,
                bace_simulation::CorpseAccessError::Invalid
                | bace_simulation::CorpseAccessError::Stale
                | bace_simulation::CorpseAccessError::Ownership => 2,
            };
            let replica = self
                .players
                .replication(pending.binding.actor)
                .ok_or("corpse rejection canonical recipient missing")?;
            if replica.key != pending.key || replica.binding != pending.binding {
                return Err("corpse rejection binding mismatch".into());
            }
            let batch = replica
                .events
                .project_inventory_with_actor(
                    pending.binding,
                    &[P::Simple(bace_wire::SimpleGameEvent::WeenieError(code))],
                    &mut replica.item_properties,
                    Some(&mut replica.properties),
                    ObjectCodecLimits {
                        max_message_bytes: self.limits.message_bytes,
                        max_model_entries: 255,
                        max_children: 128,
                        max_restrictions: 1024,
                        max_motion_commands: 32,
                        max_string_bytes: 4096,
                    },
                    BatchLimits {
                        max_messages: 1,
                        max_bytes: self.limits.message_bytes,
                        max_message_bytes: self.limits.message_bytes,
                        max_string_bytes: 4096,
                    },
                )
                .map_err(|error| format!("corpse rejection projection: {error:?}"))?;
            self.network_output.push_back(
                crate::game_messages::session_batch_command(pending.key, batch)
                    .map_err(|error| error.to_string())?,
            );
            return Ok(true);
        }
        let decision = pending
            .decision
            .ok_or("corpse access output decision absent")?;
        let viewer = self.deaths.viewers.get(&pending.key).copied();
        match decision {
            CorpseAccessDecision::Open { .. }
                if viewer.is_some_and(|existing| existing != (pending.corpse, pending.binding)) =>
            {
                return Err("corpse open superseded viewer binding".into());
            }
            CorpseAccessDecision::Close { .. }
                if viewer != Some((pending.corpse, pending.binding)) =>
            {
                return Err("corpse close exact viewer binding missing".into());
            }
            _ => {}
        }
        if matches!(decision, CorpseAccessDecision::Open { .. })
            && (!pending.children.is_empty() || !pending.grandchildren.is_empty())
            && pending.appearance.is_none()
        {
            return Ok(false);
        }
        let Some(session) = self.sessions.get(&pending.key) else {
            return Err("corpse access output session missing".into());
        };
        if session.disconnected {
            // The detached viewer must be closed by the logout handoff before
            // expiry can proceed. Do not silently acknowledge its Open.
            return Err("corpse access viewer disconnected before output".into());
        }
        let loading = session
            .loading
            .as_ref()
            .ok_or("corpse access output character missing")?;
        let entries = pending
            .children
            .iter()
            .map(content_entry)
            .collect::<Vec<_>>();
        let subentries = pending
            .children
            .iter()
            .filter(|child| {
                matches!(child.item.entity.state.weenie_type, 14 | 20 | 21 | 56 | 57 | 58)
            })
            .map(|child| {
                let id = child.item.entity.object_id;
                (
                    id,
                    pending
                        .grandchildren
                        .iter()
                        .filter(|nested| {
                            matches!(nested.item.placement.as_ref(), Some(bace_storage_codec::ItemPlacementV2::Contained { container, .. }) if *container == id)
                        })
                        .map(content_entry)
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let mut visible = Vec::with_capacity(pending.children.len() + pending.grandchildren.len());
        for child in &pending.children {
            visible.push(child);
            visible.extend(pending.grandchildren.iter().filter(|nested| {
                matches!(nested.item.placement.as_ref(), Some(bace_storage_codec::ItemPlacementV2::Contained { container, .. }) if *container == child.item.entity.object_id)
            }));
        }
        let replica = self
            .players
            .replication(pending.binding.actor)
            .ok_or("corpse access canonical replication owner missing")?;
        if replica.key != pending.key || replica.binding != pending.binding {
            return Err("corpse access output binding mismatch".into());
        }
        let name = pending
            .source
            .item
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 1)
            .map(|p| p.value.as_str())
            .ok_or("corpse source name absent")?;
        let denial;
        let mut objects = Vec::new();
        let mut new_sequences = Vec::new();
        let mut steps = Vec::new();
        match decision {
            CorpseAccessDecision::Denied(reason) => {
                denial = match reason {
                    bace_simulation::CorpseAccessDenial::Rare => format!(
                        "You may not loot the {name} because the {name} has generated a rare item."
                    ),
                    bace_simulation::CorpseAccessDenial::PlayerKiller => format!(
                        "You may not loot the {name} because the death was caused by a player killer."
                    ),
                    bace_simulation::CorpseAccessDenial::Locked => {
                        format!("You do not yet have the right to loot the {name}.")
                    }
                    bace_simulation::CorpseAccessDenial::InUse => {
                        format!("The {name} is already in use by someone else!")
                    }
                };
                steps.push(P::System {
                    text: &denial,
                    chat_type: 0,
                });
            }
            CorpseAccessDecision::Close { .. } => {
                steps.push(P::Event(InventoryEvent::CloseContainer {
                    container_id: pending.corpse.0,
                }));
            }
            CorpseAccessDecision::Open { .. } => {
                let plan = content_creation_plan(
                    &visible
                        .iter()
                        .map(|child| EntityId(child.item.entity.object_id))
                        .collect::<Vec<_>>(),
                    &replica.item_properties,
                )?;
                if !visible.is_empty() {
                    let appearance = pending
                        .appearance
                        .as_ref()
                        .ok_or("corpse child DAT closure missing")?;
                    let character = loading
                        .character_assets
                        .as_ref()
                        .ok_or("corpse child chargen metadata missing")?;
                    let assets = appearance.borrowed(character.char_gen());
                    for (child, &(id, new)) in visible.iter().zip(&plan) {
                        let fresh = if new {
                            Some(
                                Sequences::new(256)
                                    .map_err(|e| format!("corpse sequence: {e:?}"))?,
                            )
                        } else {
                            None
                        };
                        let sequences = replica
                            .item_properties
                            .get(&id)
                            .or(fresh.as_ref())
                            .ok_or("corpse known sequence owner missing")?;
                        let saved = ItemSaveV4 {
                            previous: ItemSaveV3 {
                                previous: ItemSaveV2 {
                                    entity: child.item.entity.clone(),
                                    placement: child
                                        .item
                                        .placement
                                        .clone()
                                        .ok_or("corpse child placement absent")?,
                                },
                                enchantments: child.item.enchantments.clone(),
                            },
                            construction: child.item.construction.clone(),
                        };
                        let object = prepare_entry_object(
                            id.0,
                            &saved.entity.state,
                            prepare_item_model(&saved.entity.state, &assets)?,
                            crate::game_runtime::inventory::output::committed_state(
                                &saved, sequences,
                            )?,
                        )?;
                        objects.push(object);
                        if let Some(fresh) = fresh {
                            new_sequences.push((id, fresh));
                        }
                    }
                }
                steps.push(P::Event(InventoryEvent::ViewContents {
                    container_id: pending.corpse.0,
                    items: &entries,
                }));
                for (container, entries) in &subentries {
                    steps.push(P::Event(InventoryEvent::ViewContents {
                        container_id: *container,
                        items: entries,
                    }));
                }
                for object in &objects {
                    steps.push(P::Create(object));
                }
            }
        }
        let new_ids = new_sequences.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        for (id, sequences) in new_sequences {
            replica.item_properties.insert(id, sequences);
        }
        let max = self.limits.message_bytes;
        let result = replica.events.project_inventory_with_actor(
            pending.binding,
            &steps,
            &mut replica.item_properties,
            Some(&mut replica.properties),
            ObjectCodecLimits {
                max_message_bytes: max,
                max_model_entries: 255,
                max_children: 128,
                max_restrictions: 1024,
                max_motion_commands: 32,
                max_string_bytes: 4096,
            },
            BatchLimits {
                max_messages: 2049,
                max_bytes: 64 * 1024 * 1024,
                max_message_bytes: max,
                max_string_bytes: 4096,
            },
        );
        let batch = match result {
            Ok(batch) => batch,
            Err(error) => {
                for id in new_ids {
                    replica.item_properties.remove(&id);
                }
                return Err(format!("corpse access projection: {error:?}"));
            }
        };
        if !batch.messages.is_empty() {
            self.network_output.push_back(
                crate::game_messages::session_batch_command(pending.key, batch)
                    .map_err(|error| error.to_string())?,
            );
        }
        match decision {
            CorpseAccessDecision::Open { .. } => {
                self.deaths
                    .viewers
                    .insert(pending.key, (pending.corpse, pending.binding));
            }
            CorpseAccessDecision::Close { .. } => {
                self.deaths.viewers.remove(&pending.key);
            }
            CorpseAccessDecision::Denied(_) => {}
        }
        Ok(true)
    }
}

fn content_entry(child: &RegionItemSource) -> ContainerEntry {
    let source = &child.item.entity.state;
    ContainerEntry {
        object_id: child.item.entity.object_id,
        container_type: if source.weenie_type == 21 {
            1
        } else if source
            .properties
            .bools
            .iter()
            .any(|p| p.id == 81 && p.value)
        {
            2
        } else {
            0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Position, Property, WeenieV1};
    use bace_persistence::{
        DurableItemPlace, OperationOutcome, PlacementChange, PlacementOperation, SaveAck,
        SaveSnapshot, WorldPlacementOperation,
    };
    use bace_storage_codec::{
        CorpseSaveV1, CorpseSaveV2, CorpseSaveV3, CorpseSaveV4, CorpseSaveV5, EntitySaveV1,
        ItemPlacementV2,
    };

    fn corpse_source(corpse: EntityId, actor: EntityId) -> RegionItemSource {
        let mut state = WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "test_corpse".into(),
            weenie_type: 21,
            last_modified: None,
            properties: Default::default(),
        };
        state.properties.strings.push(Property {
            id: 1,
            value: "Corpse of Test Player".into(),
        });
        let entity = EntitySaveV1 {
            object_id: corpse.0,
            template_revision: 1,
            mutation_revision: 2,
            state,
        };
        let placement = ItemPlacementV2::World(Position {
            obj_cell_id: 0x1234_0001,
            position_x: 1.,
            position_y: 2.,
            position_z: 0.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        });
        let saved = CorpseSaveV5::migrate_v4(CorpseSaveV4 {
            previous: CorpseSaveV3 {
                previous: CorpseSaveV2 {
                    corpse: CorpseSaveV1 {
                        entity: entity.clone(),
                        owner: Some(actor.0),
                        death_operation: "death:7:77".into(),
                        expires_at: 12345,
                    },
                    placement: placement.clone(),
                },
                enchantments: vec![],
            },
            source: Some(actor.0),
            operation: Some(77),
        })
        .unwrap();
        RegionItemSource {
            item: crate::game_inventory::FrozenInventoryItem {
                corpse: Some(Box::new(saved.clone())),
                construction: None,
                source_destination: None,
                enchantments: vec![],
                entity,
                placement: Some(placement),
                persisted_version: 2,
            },
            corpse: Some(saved),
        }
    }

    fn present(
        key: SessionKey,
        binding: CharacterBinding,
        corpse: EntityId,
        source: RegionItemSource,
        decision: CorpseAccessDecision,
    ) -> Pending {
        Pending {
            key,
            binding,
            context: ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: 1,
            },
            corpse,
            source,
            children: vec![],
            grandchildren: vec![],
            appearance: None,
            appearance_job: None,
            source_refreshes: 0,
            inspection: 71,
            decision: Some(decision),
            has_loot_permit: false,
            rejection: None,
            phase: Phase::Present,
            detaching: false,
            followup: None,
        }
    }

    fn one_event(runtime: &mut GameRuntime, expected: u32, corpse: EntityId) {
        let crate::network::NetworkCommand::SendOrderedBatch { messages, .. } =
            runtime.network_output.pop_front().unwrap()
        else {
            panic!("corpse private owner batch");
        };
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, 9);
        assert_eq!(
            u32::from_le_bytes(messages[0].1[12..16].try_into().unwrap()),
            expected
        );
        assert_eq!(
            u32::from_le_bytes(messages[0].1[16..20].try_into().unwrap()),
            corpse.0
        );
    }

    async fn commit_access(
        runtime: &GameRuntime,
        operation: WorldPlacementOperation,
    ) -> Vec<SaveAck> {
        let mut pending =
            crate::placement_saves::PendingPlacementSave::new_world(operation).unwrap();
        pending.submit(&runtime.saves.handle).unwrap();
        let resolved = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                if let Some(result) = pending.poll() {
                    break result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let crate::placement_saves::PlacementResolution::Committed(acks) = resolved else {
            panic!("corpse access durable operation did not commit");
        };
        acks
    }

    #[tokio::test]
    async fn corpse_open_and_close_output_follow_exact_worker_and_postgres_receipts() {
        // The private event projector is fed after a real save-worker ACK,
        // exact PgStore load and the runtime's region-cache replacement.
        // Simulation adoption is exercised separately.
        let (_cluster, _directory, mut runtime, key, binding) =
            crate::game_runtime::portals::tests::output_runtime().await;
        let corpse = EntityId(0x8000_0042);
        let mut source = corpse_source(corpse, binding.actor);
        source.item.persisted_version = 0;
        let position = match source.item.placement.as_ref().unwrap() {
            ItemPlacementV2::World(position) => position,
            _ => panic!("world corpse fixture"),
        };
        let epoch = runtime.bootstrap.world_owner.epoch();
        let seed = WorldPlacementOperation {
            world_epoch: epoch,
            inventory: PlacementOperation {
                operation_id: format!("corpse-access-seed:{epoch}:{}", corpse.0),
                snapshots: vec![SaveSnapshot {
                    object_id: corpse.0,
                    mutation_revision: 2,
                    expected_version: 0,
                    bytes: source.corpse.as_ref().unwrap().encode().unwrap(),
                }],
                participants: vec![corpse.0],
                leases: vec![],
                changes: vec![PlacementChange {
                    item: corpse.0,
                    expected: None,
                    destination: DurableItemPlace::World {
                        cell: position.obj_cell_id,
                    },
                }],
                storage_views: vec![],
            },
        };
        let OperationOutcome::Committed(seed_ack) = runtime
            .bootstrap
            .store
            .world_placement_operation(&seed)
            .await
            .unwrap()
        else {
            panic!("corpse seed did not commit");
        };
        assert_eq!(
            seed_ack,
            [SaveAck {
                object_id: corpse.0,
                mutation_revision: 2,
                persisted_version: 1,
            }]
        );
        source.item.persisted_version = 1;
        runtime
            .world
            .as_mut()
            .unwrap()
            .regions
            .record_committed_corpse_sources(0x1234, vec![source.clone()])
            .unwrap();

        let (open_operation, opened) = crate::game_runtime::deaths::access_saves::freeze(
            epoch,
            81,
            corpse,
            binding.actor,
            CorpseAccessDecision::Open {
                consume_permit: true,
            },
            &source,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            commit_access(&runtime, open_operation).await,
            [SaveAck {
                object_id: corpse.0,
                mutation_revision: 3,
                persisted_version: 2,
            }]
        );
        let row = runtime
            .bootstrap
            .store
            .load(corpse.0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.persisted_version, 2);
        assert_eq!(CorpseSaveV5::decode(&row.bytes).unwrap(), opened);
        let mut open = present(
            key,
            binding,
            corpse,
            source.clone(),
            CorpseAccessDecision::Open {
                consume_permit: true,
            },
        );
        assert!(
            !runtime
                .world
                .as_ref()
                .unwrap()
                .regions
                .corpse_source(corpse)
                .unwrap()
                .corpse
                .as_ref()
                .unwrap()
                .access
                .permittees
                .contains(&binding.actor.0)
        );
        assert!(
            !runtime
                .advance_corpse_access(
                    &mut open,
                    Phase::Cache {
                        after: Box::new(opened),
                        version: row.persisted_version,
                    },
                    0,
                )
                .unwrap()
        );
        assert!(matches!(open.phase, Phase::Adopt));
        assert!(same_source_revision(
            &open.source,
            runtime
                .world
                .as_ref()
                .unwrap()
                .regions
                .corpse_source(corpse)
                .unwrap()
        ));
        source = open.source.clone();
        let first_sequence = runtime
            .players
            .replication(binding.actor)
            .unwrap()
            .events
            .next_sequence();
        assert!(runtime.project_corpse_access(&open).unwrap());
        one_event(&mut runtime, 0x0196, corpse);
        assert_eq!(runtime.deaths.viewers.get(&key), Some(&(corpse, binding)));

        let (close_operation, closed) = crate::game_runtime::deaths::access_saves::freeze(
            epoch,
            82,
            corpse,
            binding.actor,
            CorpseAccessDecision::Close { mark_looted: true },
            &source,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            commit_access(&runtime, close_operation).await,
            [SaveAck {
                object_id: corpse.0,
                mutation_revision: 4,
                persisted_version: 3,
            }]
        );
        let row = runtime
            .bootstrap
            .store
            .load(corpse.0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.persisted_version, 3);
        assert_eq!(CorpseSaveV5::decode(&row.bytes).unwrap(), closed);
        assert!(closed.access.looted);
        let mut close = present(
            key,
            binding,
            corpse,
            source,
            CorpseAccessDecision::Close { mark_looted: true },
        );
        assert!(
            !runtime
                .advance_corpse_access(
                    &mut close,
                    Phase::Cache {
                        after: Box::new(closed),
                        version: row.persisted_version,
                    },
                    0,
                )
                .unwrap()
        );
        assert!(matches!(close.phase, Phase::Adopt));
        assert!(same_source_revision(
            &close.source,
            runtime
                .world
                .as_ref()
                .unwrap()
                .regions
                .corpse_source(corpse)
                .unwrap()
        ));
        assert!(close.source.corpse.as_ref().unwrap().access.looted);
        assert!(runtime.project_corpse_access(&close).unwrap());
        one_event(&mut runtime, 0x0052, corpse);
        assert!(!runtime.deaths.viewers.contains_key(&key));
        assert_eq!(
            runtime
                .players
                .replication(binding.actor)
                .unwrap()
                .events
                .next_sequence(),
            first_sequence + 2
        );
    }

    #[tokio::test]
    async fn corpse_open_close_projection_requires_exact_viewer_and_frozen_v5_looted_row() {
        // ACE Container.SendInventory/CloseGroundContainer use the same
        // authenticated event owner. This constructs a post-ack cache source
        // from the exact frozen V5 row; the save worker/DB receipt is covered
        // separately. Close cannot retire a newer or different viewer.
        let (_cluster, _directory, mut runtime, key, binding) =
            crate::game_runtime::portals::tests::output_runtime().await;
        let corpse = EntityId(0x8000_0041);
        let source = corpse_source(corpse, binding.actor);
        let open = present(
            key,
            binding,
            corpse,
            source.clone(),
            CorpseAccessDecision::Open {
                consume_permit: false,
            },
        );
        assert!(runtime.project_corpse_access(&open).unwrap());
        one_event(&mut runtime, 0x0196, corpse);
        assert_eq!(runtime.deaths.viewers.get(&key), Some(&(corpse, binding)));

        let (operation, after) = crate::game_runtime::deaths::access_saves::freeze(
            7,
            71,
            corpse,
            binding.actor,
            CorpseAccessDecision::Close { mark_looted: true },
            &source,
        )
        .unwrap()
        .unwrap();
        let snapshot = &operation.inventory.snapshots[0];
        assert_eq!(snapshot.object_id, corpse.0);
        assert_eq!(snapshot.expected_version, source.item.persisted_version);
        let frozen = CorpseSaveV5::decode(&snapshot.bytes).unwrap();
        assert_eq!(frozen, after);
        assert!(frozen.access.looted);
        assert_eq!(frozen.corpse.entity.mutation_revision, 3);
        let mut close_source = source;
        close_source.item.persisted_version = 3;
        close_source.item.entity = after.corpse.entity.clone();
        close_source.item.corpse = Some(Box::new(after.clone()));
        close_source.corpse = Some(after);
        let mut close = present(
            key,
            binding,
            corpse,
            close_source,
            CorpseAccessDecision::Close { mark_looted: true },
        );
        let before_event = runtime
            .players
            .replication(binding.actor)
            .unwrap()
            .events
            .next_sequence();
        runtime
            .deaths
            .viewers
            .insert(key, (EntityId(corpse.0 + 1), binding));
        assert!(runtime.project_corpse_access(&close).is_err());
        assert!(runtime.network_output.is_empty());
        assert_eq!(
            runtime
                .players
                .replication(binding.actor)
                .unwrap()
                .events
                .next_sequence(),
            before_event
        );
        runtime.deaths.viewers.insert(key, (corpse, binding));
        close.binding.session.0 += 1;
        assert!(runtime.project_corpse_access(&close).is_err());
        assert_eq!(
            runtime
                .players
                .replication(binding.actor)
                .unwrap()
                .events
                .next_sequence(),
            before_event
        );
        assert_eq!(runtime.deaths.viewers.get(&key), Some(&(corpse, binding)));
        assert!(runtime.network_output.is_empty());
        let mut detach = present(
            key,
            binding,
            corpse,
            close.source.clone(),
            CorpseAccessDecision::Close { mark_looted: true },
        );
        detach.detaching = true;
        runtime
            .deaths
            .viewers
            .insert(key, (EntityId(corpse.0 + 1), binding));
        assert!(
            runtime
                .advance_corpse_access(&mut detach, Phase::Present, 0)
                .is_err()
        );
        assert_eq!(
            runtime.deaths.viewers.get(&key),
            Some(&(EntityId(corpse.0 + 1), binding))
        );
        runtime.deaths.viewers.insert(key, (corpse, binding));
        close.binding = binding;
        assert!(runtime.project_corpse_access(&close).unwrap());
        one_event(&mut runtime, 0x0052, corpse);
        assert!(!runtime.deaths.viewers.contains_key(&key));
    }

    #[test]
    fn pinned_repeat_open_recreates_known_child_with_same_sequence_owner() {
        // ACE 47edade Container.SendInventory sends ViewContents for the root
        // and subcontainers, then CreateObject for every child on each Open.
        let known = EntityId(0x8000_0001);
        let nested = EntityId(0x8000_0002);
        let existing = BTreeMap::from([(known, Sequences::with_instance(256, 17).unwrap())]);
        let plan = content_creation_plan(&[known, nested], &existing).unwrap();
        assert_eq!(plan, [(known, false), (nested, true)]);
        assert_eq!(
            existing[&known].current(bace_replication::SequenceKind::ObjectInstance, 0),
            17
        );
        assert!(content_creation_plan(&[known, known], &existing).is_err());
    }
}
