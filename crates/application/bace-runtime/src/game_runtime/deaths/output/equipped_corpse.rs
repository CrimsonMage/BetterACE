//! One source-complete ordinary corpse dequip. Broader equipment effects and
//! mixed loss transcripts remain retained until their full source order exists.
use super::*;
use crate::inventory_equipment_output::EquipmentVisibilityUpdate;
use bace_inventory::{ItemChange, ItemPlace};
use bace_replication::{InventoryProjection as P, SequenceKind};
use bace_simulation::{DeathDropOrigin, DeathDropReceipt};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV5};
use bace_wire::PropertyValue;
use std::collections::BTreeMap;

const SELECTABLE: u32 = 0x0370_0000; // pinned ACE EquipMask.Selectable

fn exact_selected_equipped(
    actor: EntityId,
    corpse: EntityId,
    drop: DeathDropReceipt,
    changes: &[ItemChange],
    committed: &[bace_persistence::SaveSnapshot],
) -> Result<Option<ItemSaveV5>, String> {
    if drop.source != drop.dropped
        || drop.amount != 1
        || drop.wielded_location == 0
        || !matches!(
            drop.origin,
            DeathDropOrigin::Wield | DeathDropOrigin::SlipperyWield
        )
    {
        return Ok(None);
    }
    let Some(change) = changes.iter().find(|c| c.after.id == drop.source) else {
        return Err("equipped death change absent".into());
    };
    let Some(before) = &change.before else {
        return Err("equipped death prior item absent".into());
    };
    if !matches!(before.place, ItemPlace::Contained { container, equipped, .. } if container == actor && equipped == drop.wielded_location)
        || !matches!(change.after.place, ItemPlace::Contained { container, equipped: 0, .. } if container == corpse)
        || before.id != drop.source
        || before.template != change.after.template
        || before.stack != 1
        || change.after.stack != 1
        || before.is_container
        || before.revision.checked_add(1) != Some(change.after.revision)
    {
        return Ok(None);
    }
    let row = committed
        .iter()
        .find(|row| row.object_id == drop.source.0)
        .ok_or("equipped death committed row absent")?;
    let saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
    let ItemPlace::Contained { slot, .. } = change.after.place else {
        unreachable!();
    };
    if row.expected_version <= 0
        || row.mutation_revision != change.after.revision
        || saved.entity.object_id != drop.source.0
        || saved.entity.mutation_revision != change.after.revision
        || saved.entity.state.weenie_id != before.template
        || saved.placement
            != (ItemPlacementV2::Contained {
                container: corpse.0,
                slot,
                pack_slot: change.after.pack_slot,
                equipped: 0,
            })
    {
        return Err("equipped death durable item identity/placement mismatch".into());
    }
    if !saved.entity.state.properties.spell_book.is_empty()
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
    Ok(Some(saved))
}

fn source_steps<'a>(
    actor: EntityId,
    item: EntityId,
    wielded_location: u32,
    model: &'a bace_wire::ObjectModel,
    burden: i32,
) -> (Vec<P<'a>>, Vec<usize>) {
    let mut steps = Vec::new();
    let mut public = Vec::new();
    // Creature_Equipment.TryDequipObjectWithBroadcasting precedes the
    // Player_Inventory private EnqueueSend list. Only its first sound and
    // ObjDesc are observer broadcasts; the later Unwield sound is private.
    if wielded_location & SELECTABLE != 0 {
        public.push(steps.len());
        steps.push(P::Effect(bace_wire::CombatEffect::Sound {
            object_id: actor.0,
            sound_id: 0x8d,
            volume: 1.0,
        }));
    }
    public.push(steps.len());
    steps.push(P::Appearance { item: actor, model });
    steps.push(P::Property {
        item,
        property: 3,
        value: PropertyValue::InstanceId(0),
    });
    steps.push(P::Property {
        item,
        property: 10,
        value: PropertyValue::Int(0),
    });
    steps.push(P::Pickup(item));
    steps.push(P::Effect(bace_wire::CombatEffect::Sound {
        object_id: actor.0,
        sound_id: 0x8d,
        volume: 1.0,
    }));
    steps.push(P::Delete(item));
    steps.push(P::PrivateProperty {
        property: 5,
        value: PropertyValue::Int(burden),
    });
    (steps, public)
}

impl GameRuntime {
    pub(super) fn project_equipped_corpse_completion(
        &mut self,
        operation: u64,
    ) -> Result<bool, String> {
        let Some(DeathDelivery {
            work:
                DeathDeliveryWork::Completed {
                    completion,
                    before,
                    appearance,
                    ..
                },
            ..
        }) = self.deaths.deliveries.front()
        else {
            return Err("equipped death completion owner absent".into());
        };
        let ticket = &completion.work.ticket;
        let Some(transcript) = ticket.inventory_transcript.as_ref() else {
            return Err("equipped death transcript absent".into());
        };
        let [drop] = transcript.drops.as_slice() else {
            return Ok(false);
        };
        if ticket.operation != operation
            || ticket.actor != completion.work.binding.actor
            || ticket.no_corpse.is_some()
            || ticket.olthoi.is_some()
            || ticket.corpse.0 == 0
            || ticket.corpse_items != [drop.source]
            || !transcript.destroyed.is_empty()
            || !transcript.coin_sources.is_empty()
            || !transcript.coin_drops.is_empty()
            || transcript.coin_amount != 0
            || drop.burden_after != ticket.inventory.proposal.actor_burden
            || ticket
                .inventory
                .proposal
                .changes
                .iter()
                .any(|change| change.after.id != drop.source && change.after.id != ticket.corpse)
        {
            return Ok(false);
        }
        let prior_change = ticket
            .inventory
            .proposal
            .changes
            .iter()
            .find(|change| change.after.id == drop.source)
            .and_then(|change| change.before.as_ref());
        if prior_change != before.items().iter().find(|item| item.id == drop.source) {
            return Ok(false);
        }
        let Some(saved) = exact_selected_equipped(
            ticket.actor,
            ticket.corpse,
            *drop,
            &ticket.inventory.proposal.changes,
            &completion.committed,
        )?
        else {
            return Ok(false);
        };
        if before
            .items()
            .iter()
            .filter(
                |item| matches!(item.place, ItemPlace::Contained { equipped, .. } if equipped != 0),
            )
            .map(|item| item.id)
            .collect::<Vec<_>>()
            != [drop.source]
            || ticket
                .before_enchantments
                .iter()
                .any(|entry| entry.caster == drop.source.0)
        {
            return Ok(false);
        }
        let name = saved
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 1)
            .ok_or("equipped death item name absent")?;
        if name.value.len() > 4096 {
            return Err("equipped death item name capacity".into());
        }
        let plural = saved
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 20)
            .map(|p| p.value.clone());
        let names = BTreeMap::from([(drop.source, (name.value.clone(), plural))]);
        let Some(chat) = completed::source_drop_message(transcript, &BTreeMap::new(), &names)?
        else {
            return Ok(false);
        };
        let post_corpse =
            corpse_location::CorpseLocationOutput::prepare(ticket, &completion.committed)?;
        let actor = ticket.actor;
        let binding = completion.work.binding;
        let key = self
            .players
            .replication(actor)
            .ok_or("equipped death replica absent")?
            .key;
        if self.network_output.len() >= self.limits.messages
            || self.sessions.get(&key).is_none_or(|s| s.disconnected)
        {
            return Ok(false);
        }
        let replica = self
            .players
            .replication(actor)
            .ok_or("equipped death replica absent")?;
        if replica.binding != binding
            || key.generation != binding.session.0
            || !replica.item_properties.contains_key(&drop.source)
        {
            return Ok(false);
        }
        let instance_sequence = replica.properties.current(SequenceKind::ObjectInstance, 0);
        let player = self
            .online_saves
            .baseline(actor.0)
            .ok_or("equipped death committed player absent")?
            .0;
        if player.player.entity.mutation_revision != ticket.after_revision {
            return Err("equipped death committed player revision mismatch".into());
        }
        if !self
            .online_saves
            .equipped_inventory_baselines(actor.0)
            .is_empty()
        {
            return Ok(false);
        }
        let chargen = self
            .sessions
            .get(&key)
            .and_then(|s| s.loading.as_ref())
            .and_then(|l| l.character_assets.as_ref())
            .ok_or("equipped death chargen absent")?;
        let model = crate::player_entry::prepare_player_model(
            &player.player.entity.state,
            &[],
            crate::player_entry::PlayerAppearanceOptions {
                show_helm: player.player.metadata.options2 & 0x100000 != 0,
                show_cloak: player.player.metadata.options2 & 0x800000 != 0,
                default_hair_texture: player.player.metadata.default_hair_texture,
                hair_texture: player.player.metadata.hair_texture,
            },
            &appearance.borrowed(chargen.char_gen()),
        )?
        .model;
        let update = EquipmentVisibilityUpdate {
            actor,
            operation,
            before_revision: ticket.before_revision,
            after_revision: ticket.after_revision,
            incarnation: key.generation,
            instance_sequence,
            model,
            children: vec![],
            descriptions: vec![],
        };
        let burden =
            i32::try_from(drop.burden_after).map_err(|_| "equipped death burden overflow")?;
        let (mut steps, public) = source_steps(
            actor,
            drop.source,
            drop.wielded_location,
            &update.model,
            burden,
        );
        if !chat.is_empty() {
            steps.push(P::System {
                text: &chat,
                chat_type: 0,
            });
        }
        post_corpse.append(&mut steps);
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
        let replica = self
            .players
            .replication(actor)
            .ok_or("equipped death replica disappeared")?;
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
                .map_err(|e| format!("equipped death preflight retained: {e:?}"))?;
        let observer_bytes = public.iter().try_fold(0usize, |sum, index| {
            sum.checked_add(projected.messages[*index].bytes.len())
        });
        if observer_bytes.is_none_or(|bytes| !self.observer_room(1, bytes)) {
            return Ok(false);
        }
        match self.visibility.service.replace_equipment_blueprint(&update) {
            Ok(()) => {}
            Err(crate::visibility_service::VisibilityServiceError::Capacity) => return Ok(false),
            Err(error) => return Err(format!("equipped death appearance retained: {error:?}")),
        }
        let replica = self
            .players
            .replication(actor)
            .ok_or("equipped death replica disappeared")?;
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
            .map_err(|e| format!("equipped death projection retained: {e:?}"))?;
        let observers = public
            .into_iter()
            .map(|index| batch.messages[index].clone())
            .collect::<Vec<_>>();
        replica.item_properties.remove(&drop.source);
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
    use bace_content::WeenieV1;
    use bace_inventory::InventoryItem;
    use bace_storage_codec::{EntitySaveV1, ItemSaveV2, ItemSaveV3, ItemSaveV4};

    fn fixture() -> (
        EntityId,
        EntityId,
        DeathDropReceipt,
        Vec<ItemChange>,
        Vec<bace_persistence::SaveSnapshot>,
    ) {
        let actor = EntityId(0x5000_0001);
        let corpse = EntityId(0x8000_0010);
        let item = EntityId(0x8000_0021);
        let before = InventoryItem {
            structure: None,
            id: item,
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
        after.place = ItemPlace::Contained {
            container: corpse,
            slot: 1,
            equipped: 0,
        };
        after.revision = 2;
        let saved = ItemSaveV5 {
            source_destination: None,
            previous: ItemSaveV4 {
                previous: ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity: EntitySaveV1 {
                            object_id: item.0,
                            template_revision: 1,
                            mutation_revision: 2,
                            state: WeenieV1 {
                                schema_version: 1,
                                weenie_id: 100,
                                class_name: "equipped death item".into(),
                                weenie_type: 1,
                                last_modified: None,
                                properties: Default::default(),
                            },
                        },
                        placement: ItemPlacementV2::Contained {
                            container: corpse.0,
                            slot: 1,
                            pack_slot: false,
                            equipped: 0,
                        },
                    },
                    enchantments: vec![],
                },
                construction: None,
            },
        };
        (
            actor,
            corpse,
            DeathDropReceipt {
                source: item,
                dropped: item,
                amount: 1,
                origin: DeathDropOrigin::Wield,
                wielded_location: 0x0010_0000,
                burden_after: 1,
            },
            vec![ItemChange {
                before: Some(before),
                after,
            }],
            vec![bace_persistence::SaveSnapshot {
                object_id: item.0,
                mutation_revision: 2,
                expected_version: 1,
                bytes: saved.encode().unwrap(),
            }],
        )
    }

    #[test]
    fn selected_equipped_corpse_requires_exact_v5_owner_and_revision() {
        let (actor, corpse, drop, changes, mut rows) = fixture();
        assert!(
            exact_selected_equipped(actor, corpse, drop, &changes, &rows)
                .unwrap()
                .is_some()
        );
        rows[0].expected_version = 0;
        assert!(exact_selected_equipped(actor, corpse, drop, &changes, &rows).is_err());
        rows[0].expected_version = 1;
        let mut saved = ItemSaveV5::decode(&rows[0].bytes).unwrap();
        saved.placement = ItemPlacementV2::Contained {
            container: corpse.0,
            slot: 2,
            pack_slot: false,
            equipped: 0,
        };
        rows[0].bytes = saved.encode().unwrap();
        assert!(exact_selected_equipped(actor, corpse, drop, &changes, &rows).is_err());
        let mut wrong_mask = drop;
        wrong_mask.wielded_location = 0x0020_0000;
        assert!(
            exact_selected_equipped(actor, corpse, wrong_mask, &changes, &rows)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn pinned_corpse_dequip_broadcast_and_private_order() {
        // ACE 47edade Player_Death.CalculateDeathItems →
        // Creature_Equipment.TryDequipObjectWithBroadcasting →
        // Player_Inventory.TryDequipObjectWithNetworking(ToCorpseOnDeath).
        let model = bace_wire::ObjectModel::default();
        let (steps, public) = source_steps(
            EntityId(0x5000_0001),
            EntityId(0x8000_0021),
            0x0010_0000,
            &model,
            40,
        );
        let tags = steps
            .iter()
            .map(|step| match step {
                P::Effect(bace_wire::CombatEffect::Sound { .. }) => "sound",
                P::Appearance { .. } => "objdesc",
                P::Property { property: 3, .. } => "wielder",
                P::Property { property: 10, .. } => "wield_location",
                P::Pickup(_) => "pickup",
                P::Delete(_) => "delete",
                P::PrivateProperty { property: 5, .. } => "burden",
                _ => "unexpected",
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tags,
            [
                "sound",
                "objdesc",
                "wielder",
                "wield_location",
                "pickup",
                "sound",
                "delete",
                "burden",
            ]
        );
        assert_eq!(public, [0, 1]);
        let (nonselectable, public) =
            source_steps(EntityId(0x5000_0001), EntityId(0x8000_0021), 1, &model, 40);
        assert_eq!(nonselectable.len(), 7);
        assert_eq!(public, [0]);

        let binding = bace_gameplay_api::CharacterBinding {
            actor: EntityId(0x5000_0001),
            account: bace_types::AccountId(1),
            session: bace_gameplay_api::SessionId(1),
        };
        let mut events = bace_replication::EventSequencer::new(binding, 7);
        let mut actor = bace_replication::Sequences::new(10).unwrap();
        let item = EntityId(0x8000_0021);
        let mut items = BTreeMap::from([(item, bace_replication::Sequences::new(10).unwrap())]);
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
                    max_messages: steps.len(),
                    max_bytes: 32768,
                    max_message_bytes: 4096,
                    max_string_bytes: 4096,
                },
            )
            .unwrap();
        assert_eq!(batch.messages.len(), 8);
        assert_eq!(
            batch.messages.iter().map(|m| m.queue).collect::<Vec<_>>(),
            [10, 10, 9, 9, 10, 10, 10, 9]
        );
        assert_eq!(items[&item].current(SequenceKind::PropertyInstanceId, 3), 0);
        assert_eq!(items[&item].current(SequenceKind::PropertyInt, 10), 0);
        assert_eq!(items[&item].current(SequenceKind::ObjectPosition, 0), 1);
        assert_eq!(actor.current(SequenceKind::ObjectVisualDesc, 0), 1);
        assert_eq!(actor.current(SequenceKind::PropertyInt, 5), 0);
    }
}
