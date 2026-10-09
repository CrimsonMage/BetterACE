//! The pinned ACE corpse-null branch dequips a selected wielded treasure item
//! before Landblock.AddObject publishes its world root. This narrow projector
//! admits one selected equipped root without item spells, sets or gear-health
//! effects, and preserves the other committed equipment in the visibility handoff.
use super::*;
use crate::inventory_equipment_output::EquipmentVisibilityUpdate;
use bace_inventory::{ItemChange, ItemPlace};
use bace_replication::{InventoryProjection as P, SequenceKind};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV5};
use bace_wire::PropertyValue;
use std::collections::{BTreeMap, BTreeSet};

const SELECTABLE: u32 = 0x0370_0000; // ACE EquipMask.Selectable

struct SelectedEquipped {
    item: EntityId,
    wielded_location: u32,
}

/// The committed roster must be exactly the pre-death equipped roster minus
/// the selected world root. Other worn items retain their authored order for
/// CalculateObjDesc and child attachment publication.
fn retained_equipment_matches(
    actor: EntityId,
    before: &[bace_inventory::InventoryItem],
    selected: EntityId,
    after: &[&bace_storage_codec::ItemSaveV4],
) -> bool {
    let prior = before
        .iter()
        .filter_map(|item| match item.place {
            ItemPlace::Contained {
                equipped,
                container,
                ..
            } if container == actor && equipped != 0 && item.id != selected => {
                Some((item.id.0, equipped))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let current = after
        .iter()
        .filter_map(|item| match item.placement {
            ItemPlacementV2::Contained {
                container,
                equipped,
                ..
            } if container == actor.0 && equipped != 0 => Some((item.entity.object_id, equipped)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    prior.len() == after.len() && current.len() == after.len() && prior == current
}

fn selected_equipped(
    actor: EntityId,
    plan: &bace_simulation::PlayerNoCorpsePlan,
    changes: &[ItemChange],
    committed: &[bace_persistence::SaveSnapshot],
    drop_plain_wield: bool,
) -> Result<Option<SelectedEquipped>, String> {
    let selected = changes
        .iter()
        .filter(|change| {
            change.before.as_ref().is_some_and(|before| {
                matches!(before.place, ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)
            }) && change.after.place == ItemPlace::World
        })
        .collect::<Vec<_>>();
    let [change] = selected.as_slice() else {
        return Ok(None);
    };
    let before = change.before.as_ref().expect("selected equipped before");
    let ItemPlace::Contained {
        equipped: wielded_location,
        ..
    } = before.place
    else {
        unreachable!();
    };
    if before.is_container
        || before.stack != 1
        || plan.descendants.iter().any(|d| d.parent == before.id)
    {
        return Ok(None);
    }
    if before.revision.checked_add(1) != Some(change.after.revision)
        || !plan.world_roots.contains(&before.id)
    {
        return Err("NoCorpse equipped selection revision/root mismatch".into());
    }
    let mut pack_plan = plan.clone();
    pack_plan.world_roots.retain(|id| *id != before.id);
    let pack_changes = changes
        .iter()
        .filter(|candidate| candidate.after.id != before.id)
        .cloned()
        .collect::<Vec<_>>();
    if !super::completed::supported_no_corpse_pack_completion(
        actor,
        &pack_plan,
        &pack_changes,
        committed,
    )? {
        return Ok(None);
    }
    let roots = plan.world_roots.iter().copied().collect::<BTreeSet<_>>();
    if roots.len() != plan.world_roots.len() || changes.len() > 1024 {
        return Err("NoCorpse equipped root identity/capacity".into());
    }
    let row = committed
        .iter()
        .find(|row| row.object_id == before.id.0)
        .ok_or("NoCorpse equipped durable row absent")?;
    let saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
    let flag = if drop_plain_wield { 10 } else { 8 };
    if row.expected_version <= 0
        || row.mutation_revision != change.after.revision
        || saved.entity.object_id != before.id.0
        || saved.entity.mutation_revision != change.after.revision
        || saved.entity.state.weenie_id != before.template
        || saved.placement != ItemPlacementV2::World(plan.accepted_position.clone())
        || saved
            .source_destination
            .is_some_and(|value| value & flag == 0)
    {
        return Err("NoCorpse equipped committed origin/pose/revision mismatch".into());
    }
    if saved.source_destination.is_none()
        || !saved.entity.state.properties.spell_book.is_empty()
        || !saved.enchantments.is_empty()
        || saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .any(|p| matches!(p.id, 265 | 379) && p.value != 0 || p.id == 33 && p.value == -2)
    {
        return Ok(None);
    }
    Ok(Some(SelectedEquipped {
        item: before.id,
        wielded_location,
    }))
}

fn source_steps<'a>(
    actor: EntityId,
    item: EntityId,
    mask: u32,
    model: &'a bace_wire::ObjectModel,
    burden: i32,
) -> (Vec<P<'a>>, Vec<usize>) {
    let mut steps = Vec::with_capacity(4);
    let mut public = Vec::with_capacity(3);
    // ACE Creature_Equipment.TryDequipObjectWithBroadcasting: sound (only
    // Selectable), ObjDesc, tracked child removal; Creature_Death follows with
    // Wielder IID=0. The death world's CreateObject is a later delivery.
    if mask & SELECTABLE != 0 {
        public.push(steps.len());
        steps.push(P::Effect(bace_wire::CombatEffect::Sound {
            object_id: actor.0,
            sound_id: 0x8d,
            volume: 1.0,
        }));
    }
    public.push(steps.len());
    steps.push(P::Appearance { item: actor, model });
    public.push(steps.len());
    steps.push(P::Property {
        item,
        property: 3,
        value: PropertyValue::InstanceId(0),
    });
    steps.push(P::PrivateProperty {
        property: 5,
        value: PropertyValue::Int(burden),
    });
    (steps, public)
}

impl GameRuntime {
    pub(super) fn project_equipped_no_corpse_completion(
        &mut self,
        operation: u64,
    ) -> Result<bool, String> {
        let Some(DeathDelivery {
            work:
                DeathDeliveryWork::Completed {
                    completion,
                    before,
                    appearance,
                    world_visibility,
                    ..
                },
            ..
        }) = self.deaths.deliveries.front()
        else {
            return Err("NoCorpse equipped completion owner absent".into());
        };
        let ticket = &completion.work.ticket;
        let Some(plan) = &ticket.no_corpse else {
            return Err("NoCorpse equipped plan absent".into());
        };
        if ticket.operation != operation || ticket.actor != completion.work.binding.actor {
            return Err("NoCorpse equipped completion identity".into());
        }
        let Some(selected) = selected_equipped(
            ticket.actor,
            plan,
            &ticket.inventory.proposal.changes,
            &completion.committed,
            self.bootstrap
                .config
                .world
                .death
                .creatures_drop_createlist_wield,
        )?
        else {
            return Ok(false);
        };
        if ticket
            .inventory
            .proposal
            .changes
            .iter()
            .find(|change| change.after.id == selected.item)
            .and_then(|change| change.before.as_ref())
            != before.items().iter().find(|item| item.id == selected.item)
            || ticket
                .before_enchantments
                .iter()
                .any(|entry| entry.caster == selected.item.0)
            || !world_visibility
                .iter()
                .any(|root| root.description.object_id == selected.item.0)
        {
            return Ok(false);
        }
        let actor = ticket.actor;
        let binding = completion.work.binding;
        let before_revision = ticket.before_revision;
        let after_revision = ticket.after_revision;
        let burden = i32::try_from(ticket.inventory.proposal.actor_burden)
            .map_err(|_| "NoCorpse equipped burden overflow")?;
        if self.network_output.len() >= self.limits.messages {
            return Ok(false);
        }
        let Some(replica) = self.players.replication(actor) else {
            return Ok(false);
        };
        let key = replica.key;
        if replica.binding != binding
            || self.sessions.get(&key).is_none_or(|s| s.disconnected)
            || !replica.item_properties.contains_key(&selected.item)
        {
            return Ok(false);
        }
        let instance_sequence = replica.properties.current(SequenceKind::ObjectInstance, 0);
        let player = self
            .online_saves
            .baseline(actor.0)
            .ok_or("NoCorpse equipped committed player absent")?
            .0;
        if player.player.entity.mutation_revision != after_revision {
            return Err("NoCorpse equipped committed player revision mismatch".into());
        }
        let items = self.online_saves.equipped_inventory_baselines(actor.0);
        if items.len() > 64
            || !retained_equipment_matches(actor, before.items(), selected.item, &items)
        {
            return Ok(false);
        }
        let equipment = items
            .iter()
            .map(|item| {
                (
                    item.entity.object_id,
                    &item.entity.state,
                    match item.placement {
                        ItemPlacementV2::Contained { equipped, .. } => equipped,
                        _ => 0,
                    },
                )
            })
            .collect::<Vec<_>>();
        let chargen = self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .and_then(|l| l.character_assets.as_ref())
            .ok_or("NoCorpse equipped chargen absent")?;
        let assets = appearance.borrowed(chargen.char_gen());
        let (children, attachments) =
            crate::player_entry::prepare_entry_attachments(actor.0, &equipment, false)?;
        let model = crate::player_entry::prepare_player_model(
            &player.player.entity.state,
            &equipment
                .iter()
                .map(|(_, state, _)| *state)
                .collect::<Vec<_>>(),
            crate::player_entry::PlayerAppearanceOptions {
                show_helm: player.player.metadata.options2 & 0x100000 != 0,
                show_cloak: player.player.metadata.options2 & 0x800000 != 0,
                default_hair_texture: player.player.metadata.default_hair_texture,
                hair_texture: player.player.metadata.hair_texture,
            },
            &assets,
        )?
        .model;
        let mut descriptions = Vec::new();
        for attachment in attachments
            .iter()
            .filter(|attachment| attachment.parent.is_some())
        {
            let source = &items
                .iter()
                .find(|item| item.entity.object_id == attachment.item)
                .ok_or("NoCorpse retained attachment source missing")?
                .entity
                .state;
            let sequences = replica
                .item_properties
                .get(&EntityId(attachment.item))
                .ok_or("NoCorpse retained attachment sequence missing")?;
            let mut state =
                crate::game_runtime::inventory::output::contained_state(source, sequences);
            state.parent = attachment.parent;
            state.movement = Some(bace_wire::PhysicsMovement::AnimationFrame(
                attachment.placement,
            ));
            descriptions.push(Arc::new(crate::player_entry::prepare_entry_object(
                attachment.item,
                source,
                crate::player_entry::prepare_item_model(source, &assets)?,
                state,
            )?));
        }
        let update = EquipmentVisibilityUpdate {
            actor,
            operation,
            before_revision,
            after_revision,
            incarnation: key.generation,
            instance_sequence,
            model,
            children,
            descriptions,
        };
        let (steps, public) = source_steps(
            actor,
            selected.item,
            selected.wielded_location,
            &update.model,
            burden,
        );
        let max = self.limits.message_bytes;
        let objects = bace_wire::ObjectCodecLimits {
            max_message_bytes: max,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 4096,
        };
        let limits = BatchLimits {
            max_messages: steps.len(),
            max_bytes: 1024 * 1024,
            max_message_bytes: max,
            max_string_bytes: 4096,
        };
        // Check every packet and canonical counter on proposal copies before
        // replacing the accepted appearance. A retained retry sees the same
        // visibility receipt even if later reliable capacity is unavailable.
        let replica = self
            .players
            .replication(actor)
            .ok_or("NoCorpse equipped replica disappeared")?;
        let mut proposed_items = replica
            .item_properties
            .iter()
            .map(|(id, sequences)| (*id, sequences.proposal_copy()))
            .collect();
        let mut proposed_actor = replica.properties.proposal_copy();
        let projected =
            bace_replication::EventSequencer::new(binding, replica.events.next_sequence())
                .project_inventory_with_actor(
                    binding,
                    &steps,
                    &mut proposed_items,
                    Some(&mut proposed_actor),
                    objects,
                    limits,
                )
                .map_err(|e| format!("NoCorpse equipped preflight retained: {e:?}"))?;
        let observer_bytes = public.iter().try_fold(0usize, |sum, index| {
            sum.checked_add(projected.messages[*index].bytes.len())
        });
        if observer_bytes.is_none_or(|bytes| !self.observer_room(1, bytes)) {
            return Ok(false);
        }
        match self.visibility.service.replace_equipment_blueprint(&update) {
            Ok(()) => {}
            Err(crate::visibility_service::VisibilityServiceError::Capacity) => return Ok(false),
            Err(error) => return Err(format!("NoCorpse equipped appearance retained: {error:?}")),
        }
        let replica = self
            .players
            .replication(actor)
            .ok_or("NoCorpse equipped replica disappeared")?;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                objects,
                limits,
            )
            .map_err(|e| format!("NoCorpse equipped projection retained: {e:?}"))?;
        let observers = public
            .into_iter()
            .map(|index| batch.messages[index].clone())
            .collect::<Vec<_>>();
        self.retain_observer_messages(vec![(actor, observers)])?;
        self.mark_last_observer_visibility_reset();
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Position, WeenieV1};
    use bace_inventory::InventoryItem;
    use bace_storage_codec::{EntitySaveV1, ItemSaveV2, ItemSaveV3, ItemSaveV4};

    fn equipped_fixture() -> (
        bace_simulation::PlayerNoCorpsePlan,
        Vec<ItemChange>,
        Vec<bace_persistence::SaveSnapshot>,
    ) {
        let actor = EntityId(0x5000_0001);
        let id = EntityId(0x8000_0021);
        let position = Position {
            obj_cell_id: 0x1234_0001,
            position_x: 12.5,
            position_y: -3.25,
            position_z: 0.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        };
        let before = InventoryItem {
            structure: None,
            id,
            revision: 1,
            template: 100,
            stack_key: 2,
            place: ItemPlace::Contained {
                container: actor,
                slot: 0,
                equipped: 0x0010_0000,
            },
            stack: 1,
            maximum_stack: 1,
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
        after.place = ItemPlace::World;
        after.revision = 2;
        let saved = ItemSaveV5 {
            source_destination: Some(8),
            previous: ItemSaveV4 {
                previous: ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity: EntitySaveV1 {
                            object_id: id.0,
                            template_revision: 1,
                            mutation_revision: 2,
                            state: WeenieV1 {
                                schema_version: 1,
                                weenie_id: 100,
                                class_name: "selected equipped treasure".into(),
                                weenie_type: 1,
                                last_modified: None,
                                properties: Default::default(),
                            },
                        },
                        placement: ItemPlacementV2::World(position.clone()),
                    },
                    enchantments: vec![],
                },
                construction: None,
            },
        };
        (
            bace_simulation::PlayerNoCorpsePlan {
                world_roots: vec![id],
                descendants: vec![],
                accepted_position: position,
            },
            vec![ItemChange {
                before: Some(before),
                after,
            }],
            vec![bace_persistence::SaveSnapshot {
                object_id: id.0,
                mutation_revision: 2,
                expected_version: 2,
                bytes: saved.encode().unwrap(),
            }],
        )
    }

    #[test]
    fn equipped_root_requires_exact_durable_origin_revision_and_copied_pose() {
        let actor = EntityId(0x5000_0001);
        let (plan, mut changes, mut committed) = equipped_fixture();
        assert_eq!(
            selected_equipped(actor, &plan, &changes, &committed, false)
                .unwrap()
                .unwrap()
                .item,
            EntityId(0x8000_0021)
        );
        changes[0].after.revision = 3;
        assert!(selected_equipped(actor, &plan, &changes, &committed, false).is_err());
        changes[0].after.revision = 2;
        let mut saved = ItemSaveV5::decode(&committed[0].bytes).unwrap();
        saved.source_destination = Some(2);
        committed[0].bytes = saved.encode().unwrap();
        assert!(selected_equipped(actor, &plan, &changes, &committed, false).is_err());
        saved.source_destination = Some(8);
        saved.placement = ItemPlacementV2::World(Position {
            position_x: 99.,
            ..plan.accepted_position.clone()
        });
        committed[0].bytes = saved.encode().unwrap();
        assert!(selected_equipped(actor, &plan, &changes, &committed, false).is_err());
        saved.placement = ItemPlacementV2::World(plan.accepted_position.clone());
        saved
            .entity
            .state
            .properties
            .spell_book
            .push(bace_content::Property { id: 123, value: 0. });
        committed[0].bytes = saved.encode().unwrap();
        assert!(
            selected_equipped(actor, &plan, &changes, &committed, false)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn selected_world_root_retains_other_equipment_and_its_child_attachment() {
        let actor = EntityId(0x5000_0001);
        let selected = EntityId(0x8000_0021);
        let (_, changes, committed) = equipped_fixture();
        let mut retained = changes[0].before.as_ref().unwrap().clone();
        retained.id = EntityId(0x8000_0022);
        retained.place = ItemPlace::Contained {
            container: actor,
            slot: 1,
            equipped: 0x0020_0000,
        };
        let retained_id = retained.id;
        let mut saved = ItemSaveV5::decode(&committed[0].bytes).unwrap().previous;
        saved.entity.object_id = retained.id.0;
        saved.placement = ItemPlacementV2::Contained {
            container: actor.0,
            slot: 1,
            pack_slot: false,
            equipped: 0x0020_0000,
        };
        let prior = vec![changes[0].before.as_ref().unwrap().clone(), retained];
        assert!(retained_equipment_matches(
            actor,
            &prior,
            selected,
            &[&saved]
        ));
        let equipment = [(retained_id.0, &saved.entity.state, 0x0020_0000)];
        let (children, attachments) =
            crate::player_entry::prepare_entry_attachments(actor.0, &equipment, false).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].object_id, retained_id.0);
        assert_eq!(attachments[0].parent.unwrap().object_id, actor.0);

        saved.placement = ItemPlacementV2::Contained {
            container: actor.0,
            slot: 1,
            pack_slot: false,
            equipped: 0x0010_0000,
        };
        assert!(!retained_equipment_matches(
            actor,
            &prior,
            selected,
            &[&saved]
        ));
        saved.entity.object_id = selected.0;
        assert!(!retained_equipment_matches(
            actor,
            &prior,
            selected,
            &[&saved]
        ));
        assert!(!retained_equipment_matches(actor, &prior, selected, &[]));
    }

    #[test]
    fn pinned_equipped_no_corpse_calls_precede_world_roots() {
        let row = include_str!("../../../../tests/fixtures/no_corpse.trace")
            .lines()
            .find(|line| line.starts_with("1|1|1|0|0|"))
            .expect("pinned equipped source row");
        let calls = row.split('|').nth(6).expect("source calls");
        assert!(calls.find("dequip:21;").unwrap() < calls.find("iid:21:Wielder:0;").unwrap());
        assert!(calls.find("iid:21:Wielder:0;").unwrap() < calls.find("world:21").unwrap());
        let model = bace_wire::ObjectModel::default();
        let (steps, public) = source_steps(EntityId(1), EntityId(21), SELECTABLE, &model, 0);
        assert_eq!(public, vec![0, 1, 2]);
        assert!(matches!(
            steps[0],
            P::Effect(bace_wire::CombatEffect::Sound { sound_id: 0x8d, .. })
        ));
        assert!(matches!(
            steps[1],
            P::Appearance {
                item: EntityId(1),
                ..
            }
        ));
        assert!(matches!(
            steps[2],
            P::Property {
                item: EntityId(21),
                property: 3,
                value: PropertyValue::InstanceId(0)
            }
        ));
        assert!(matches!(steps[3], P::PrivateProperty { property: 5, .. }));
        let binding = bace_gameplay_api::CharacterBinding {
            actor: EntityId(1),
            account: bace_types::AccountId(1),
            session: bace_gameplay_api::SessionId(1),
        };
        let mut events = bace_replication::EventSequencer::new(binding, 1);
        let mut items = std::collections::BTreeMap::from([(
            EntityId(21),
            bace_replication::Sequences::new(256).unwrap(),
        )]);
        let mut actor = bace_replication::Sequences::new(256).unwrap();
        let batch = events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut items,
                Some(&mut actor),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: 4096,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                BatchLimits {
                    max_messages: 4,
                    max_bytes: 16384,
                    max_message_bytes: 4096,
                    max_string_bytes: 4096,
                },
            )
            .unwrap();
        assert_eq!(
            batch
                .messages
                .iter()
                .map(|message| u32::from_le_bytes(message.bytes[..4].try_into().unwrap()))
                .collect::<Vec<_>>(),
            [0xf750, 0xf625, 0x02da, 0x02cd]
        );
        assert_eq!(
            batch
                .messages
                .iter()
                .map(|message| message.queue)
                .collect::<Vec<_>>(),
            [10, 10, 9, 9]
        );
    }
}
