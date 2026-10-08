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
