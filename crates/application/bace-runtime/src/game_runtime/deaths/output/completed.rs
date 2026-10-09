//! Source-ordered private inventory loss after the exact death checkpoint.
//! Unsupported source branches retain the Completed obligation for a later
//! projector, rather than publishing a plausible but incomplete transcript.
use super::*;
use bace_replication::InventoryProjection as P;
use bace_simulation::DeathDropOrigin;
use bace_storage_codec::ItemSaveV5;
use bace_wire::PropertyValue;
use std::collections::{BTreeMap, BTreeSet};

impl GameRuntime {
    pub(super) fn project_death_completed_inventory(
        &mut self,
        operation: u64,
    ) -> Result<bool, String> {
        let Some(DeathDelivery {
            work: DeathDeliveryWork::Completed { completion, .. },
            ..
        }) = self.deaths.deliveries.front()
        else {
            return Err("death completed presentation owner absent".into());
        };
        let ticket = &completion.work.ticket;
        if ticket.operation != operation || ticket.actor != completion.work.binding.actor {
            return Err("death completed presentation identity mismatch".into());
        }
        let Some(transcript) = ticket.inventory_transcript.as_ref() else {
            if let Some(plan) = ticket.no_corpse.as_ref() {
                if !supported_no_corpse_pack_completion(
                    ticket.actor,
                    plan,
                    &ticket.inventory.proposal.changes,
                    &completion.committed,
                )? {
                    return self.project_equipped_no_corpse_completion(operation);
                }
                if ticket.inventory.proposal.changes.iter().any(|change| {
                    change.before.is_some()
                        && change.after.place == bace_inventory::ItemPlace::World
                }) {
                    return self.project_no_corpse_burden(
                        ticket.actor,
                        completion.work.binding,
                        ticket.inventory.proposal.actor_burden,
                    );
                }
                return Ok(true);
            }
            // Pinned ACE Player_Death.CalculateDeathItems_Olthoi (1065-1105)
            // only creates corpse contents and cantrip logs. It never moves
            // the victim's possessions or emits a private loss transcript.
            // The later Corpse delivery owns their visibility.
            if ticket.olthoi.is_none()
                || ticket.inventory.proposal.changes.iter().any(|change| {
                    change.before.is_some()
                        || change.after.id != ticket.corpse
                            && !matches!(
                                change.after.place,
                                bace_inventory::ItemPlace::Contained { .. }
                            )
                })
            {
                return Ok(false);
            }
            return Ok(true);
        };
        if transcript.drops.iter().any(|drop| {
            drop.wielded_location != 0
                || matches!(
                    drop.origin,
                    DeathDropOrigin::Wield | DeathDropOrigin::SlipperyWield
                )
        }) {
            return self.project_equipped_corpse_completion(operation);
        }
        let actor = ticket.actor;
        let binding = completion.work.binding;
        if self.network_output.len() >= self.limits.messages || !self.observer_room(1, 1024 * 1024)
        {
            return Ok(false);
        }
        let Some(replica) = self.players.replication(actor) else {
            return Ok(false);
        };
        if replica.binding != binding
            || self
                .sessions
                .get(&replica.key)
                .is_none_or(|session| session.disconnected)
        {
            return Ok(false);
        }
        let mut names = BTreeMap::new();
        if transcript.pyreals_destroyed && !transcript.coin_drops.is_empty() {
            return Err("destroyed death coins cannot have corpse drops".into());
        }
        let needed = transcript
            .coin_drops
            .iter()
            .chain(
                transcript
                    .coin_sources
                    .iter()
                    .filter(|_| transcript.pyreals_destroyed)
                    .map(|source| &source.source),
            )
            .chain(transcript.drops.iter().map(|drop| &drop.dropped))
            .chain(transcript.destroyed.iter().map(|destroyed| &destroyed.item))
            .copied()
            .collect::<BTreeSet<_>>();
        for row in &completion.committed {
            let id = EntityId(row.object_id);
            if !needed.contains(&id) {
                continue;
            }
            let saved = ItemSaveV5::decode(&row.bytes)
                .map_err(|error| format!("death item name source: {error}"))?;
            if saved.entity.object_id != row.object_id
                || saved.entity.mutation_revision != row.mutation_revision
            {
                return Err("death item name receipt mismatch".into());
            }
            if transcript.pyreals_destroyed
                && let Some(source) = transcript
                    .coin_sources
                    .iter()
                    .find(|source| source.source == id)
            {
                let change = ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .find(|change| change.after.id == id)
                    .ok_or("destroyed coin durable change absent")?;
                validate_destroyed_coin_receipt(source, change, &saved)?;
            }
            let name = saved
                .entity
                .state
                .properties
                .strings
                .iter()
                .find(|p| p.id == 1);
            let plural = saved
                .entity
                .state
                .properties
                .strings
                .iter()
                .find(|p| p.id == 20);
            let Some(name) = name else {
                return Ok(false);
            };
            if name.value.len() > 4096
                || names
                    .insert(id, (name.value.clone(), plural.map(|p| p.value.clone())))
                    .is_some()
            {
                return Err("death item name capacity or duplicate".into());
            }
        }
        if names.len() != needed.len() {
            return Ok(false);
        }
        let mut destroyed_amounts = BTreeMap::new();
        for destroyed in &transcript.destroyed {
            let change = ticket
                .inventory
                .proposal
                .changes
                .iter()
                .find(|change| change.after.id == destroyed.item)
                .ok_or("death destroyed item change absent")?;
            let before = change
                .before
                .as_ref()
                .ok_or("death destroyed item source absent")?;
            destroyed_amounts.insert(destroyed.item, before.stack);
        }
        let Some(chat) = source_drop_message(transcript, &destroyed_amounts, &names)? else {
            return Ok(false);
        };
        let post_corpse = if ticket.olthoi.is_none() {
            Some(super::corpse_location::CorpseLocationOutput::prepare(
                ticket,
                &completion.committed,
            )?)
        } else {
            None
        };
        let mut steps = Vec::new();
        let mut public = Vec::new();
        let mut removed = Vec::new();
        let mut required_sequences = Vec::new();
        // HandleDestroyBonded calls TryConsumeFromInventoryWithNetworking
        // before SpendCurrency or selected item removal in pinned ACE. An
        // equipped or container destruction needs its own source branch.
        for destroyed in &transcript.destroyed {
            let change = ticket
                .inventory
                .proposal
                .changes
                .iter()
                .find(|change| change.after.id == destroyed.item)
                .ok_or("death destroyed item change absent")?;
            let before = change
                .before
                .as_ref()
                .ok_or("death destroyed item source absent")?;
            if before.is_container
                || before.template == 273
                || !matches!(
                    before.place,
                    bace_inventory::ItemPlace::Contained { equipped: 0, .. }
                )
                || change.after.place != bace_inventory::ItemPlace::Removed
                || change.after.stack != 0
            {
                return Ok(false);
            }
            steps.push(P::Remove(destroyed.item));
            steps.push(P::PrivateProperty {
                property: 5,
                value: PropertyValue::Int(burden(destroyed.burden_after)?),
            });
            removed.push(destroyed.item);
        }
        for source in transcript
            .coin_sources
            .iter()
            .filter(|source| !source.whole)
        {
            let change = ticket
                .inventory
                .proposal
                .changes
                .iter()
                .find(|change| change.after.id == source.source)
                .ok_or("death partial coin change absent")?;
            if change.before.as_ref().is_none_or(|before| {
                before.stack != source.before
                    || before.template != 273
                    || before.place != change.after.place
            }) || change.after.template != 273
                || change.after.stack != source.after
            {
                return Err("death partial coin receipt mismatch".into());
            }
            steps.push(P::Stack {
                item: source.source,
                quantity: source.after,
                value: source
                    .after
                    .checked_mul(change.after.unit_value)
                    .ok_or("death coin stack value overflow")?,
            });
            steps.push(P::PrivateProperty {
                property: 5,
                value: PropertyValue::Int(burden(source.burden_after)?),
            });
        }
        for source in transcript.coin_sources.iter().filter(|source| source.whole) {
            let change = ticket
                .inventory
                .proposal
                .changes
                .iter()
                .find(|change| change.after.id == source.source)
                .ok_or("death whole coin change absent")?;
            if change
                .before
                .as_ref()
                .is_none_or(|before| before.stack != source.before || before.template != 273)
                || change.after.template != 273
                || if transcript.pyreals_destroyed {
                    change.after.place != bace_inventory::ItemPlace::Removed
                        || change.after.stack != 0
                } else {
                    !matches!(change.after.place, bace_inventory::ItemPlace::Contained { container, .. } if container == ticket.corpse)
                }
            {
                return Err("death whole coin receipt mismatch".into());
            }
            public.push(steps.len());
            steps.push(P::Container {
                item: source.source,
                value: 0,
            });
            steps.push(P::Remove(source.source));
            steps.push(P::PrivateProperty {
                property: 5,
                value: PropertyValue::Int(burden(source.burden_after)?),
            });
            steps.push(P::PrivateProperty {
                property: 20,
                value: PropertyValue::Int(
                    i32::try_from(source.coin_value_after)
                        .map_err(|_| "death coin value overflow")?,
                ),
            });
            removed.push(source.source);
            required_sequences.push(source.source);
        }
        if transcript.coin_sources.iter().any(|source| !source.whole) {
            // The fresh partial stack was never in the player's inventory, so
            // SpendCurrency's failed removal recalculates CoinValue once,
            // including when whole stacks were removed earlier in its loop.
            let value = transcript
                .coin_sources
                .iter()
                .map(|source| source.coin_value_after)
                .min()
                .ok_or("death partial coin source absent")?;
            steps.push(P::PrivateProperty {
                property: 20,
                value: PropertyValue::Int(
                    i32::try_from(value).map_err(|_| "death coin value overflow")?,
                ),
            });
        }
        for drop in &transcript.drops {
            let change = ticket
                .inventory
                .proposal
                .changes
                .iter()
                .find(|change| change.after.id == drop.source)
                .ok_or("death selected item change absent")?;
            if change.before.is_none() || drop.wielded_location != 0 {
                return Err("death selected item origin mismatch".into());
            }
            match drop.origin {
                DeathDropOrigin::Split => {
                    if change.after.stack.checked_add(drop.amount)
                        != change.before.as_ref().map(|before| before.stack)
                    {
                        return Err("death selected split receipt mismatch".into());
                    }
                    steps.push(P::Stack {
                        item: drop.source,
                        quantity: change.after.stack,
                        value: change
                            .after
                            .stack
                            .checked_mul(change.after.unit_value)
                            .ok_or("death split value overflow")?,
                    });
                }
                DeathDropOrigin::Pack | DeathDropOrigin::SlipperyPack => {
                    if drop.source != drop.dropped
                        || !matches!(change.after.place, bace_inventory::ItemPlace::Contained { container, .. } if container == ticket.corpse)
                    {
                        return Err("death selected pack receipt mismatch".into());
                    }
                    public.push(steps.len());
                    steps.push(P::Container {
                        item: drop.source,
                        value: 0,
                    });
                    steps.push(P::Remove(drop.source));
                    steps.push(P::PrivateProperty {
                        property: 5,
                        value: PropertyValue::Int(burden(drop.burden_after)?),
                    });
                    removed.push(drop.source);
                    required_sequences.push(drop.source);
                }
                DeathDropOrigin::Wield | DeathDropOrigin::SlipperyWield => {
                    unreachable!("preflighted held branch")
                }
            }
        }
        if !chat.is_empty() {
            steps.push(P::System {
                text: &chat,
                chat_type: 0,
            });
        }
        if let Some(post_corpse) = &post_corpse {
            post_corpse.append(&mut steps);
        }
        if steps.is_empty() {
            return Ok(true);
        }
        if steps.len() > 1024 {
            return Err("death inventory message capacity".into());
        }
        if required_sequences
            .iter()
            .any(|id| !replica.item_properties.contains_key(id))
        {
            return Err("death removed item sequence owner absent".into());
        }
        let key = replica.key;
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: max,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                bace_replication::BatchLimits {
                    max_messages: steps.len(),
                    max_bytes: 1024 * 1024,
                    max_message_bytes: max,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("death inventory projection: {error:?}"))?;
        let observer_messages: Vec<bace_replication::ReplicationMessage> = public
            .into_iter()
            .map(|index| batch.messages[index].clone())
            .collect();
        for id in removed {
            replica.item_properties.remove(&id);
        }
        if !observer_messages.is_empty() {
            self.retain_observer_messages(vec![(actor, observer_messages)])?;
        }
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(true)
    }
}

impl GameRuntime {
    /// BetterACE's corrected single world owner lowers carried burden. The
    /// pinned ACE corpse-null branch retained the Inventory dictionary entry
    /// and therefore had no source burden packet. Publish the exact accepted
    /// post-operation value through the actor's canonical private sequencer.
    fn project_no_corpse_burden(
        &mut self,
        actor: EntityId,
        binding: bace_gameplay_api::CharacterBinding,
        burden_after: u64,
    ) -> Result<bool, String> {
        let value = burden(burden_after)?;
        let room = self.network_output.len() < self.limits.messages;
        let key = match self.death_private_admission(actor, Some(binding), room)? {
            super::protection::Admission::Detached => return Ok(true),
            super::protection::Admission::Retain => return Ok(false),
            super::protection::Admission::Publish(key) => key,
        };
        let Some(replica) = self.players.replication(actor) else {
            return Err("NoCorpse burden admitted replica disappeared".into());
        };
        if replica.binding != binding || replica.key != key {
            return Err("NoCorpse burden admitted binding changed".into());
        }
        let max = self.limits.message_bytes;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &[P::PrivateProperty {
                    property: 5,
                    value: PropertyValue::Int(value),
                }],
                &mut replica.item_properties,
                Some(&mut replica.properties),
                bace_wire::ObjectCodecLimits {
                    max_message_bytes: max,
                    max_model_entries: 255,
                    max_children: 128,
                    max_restrictions: 1024,
                    max_motion_commands: 32,
                    max_string_bytes: 4096,
                },
                bace_replication::BatchLimits {
                    max_messages: 1,
                    max_bytes: max,
                    max_message_bytes: max,
                    max_string_bytes: 4096,
                },
            )
            .map_err(|error| format!("NoCorpse burden projection: {error:?}"))?;
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(true)
    }
}

/// The pinned corpse-null GenerateTreasure branch emits no private inventory
/// removal or slot update for a direct pack root. BetterACE sends only a
/// corrective private burden value for its single-owner transfer.
/// Landblock.AddWorldObject publishes the world object later. Equipped roots
/// use a separate broadcast/dequip transcript.
pub(super) fn supported_no_corpse_pack_completion(
    actor: EntityId,
    plan: &bace_simulation::PlayerNoCorpsePlan,
    changes: &[bace_inventory::ItemChange],
    committed: &[bace_persistence::SaveSnapshot],
) -> Result<bool, String> {
    let roots = plan.world_roots.iter().copied().collect::<BTreeSet<_>>();
    if roots.len() != plan.world_roots.len()
        || changes.len() > 1024
        || roots.len() + plan.descendants.len() > 1024
    {
        return Err("NoCorpse output root capacity/identity".into());
    }
    let mut admitted = roots.clone();
    for descendant in &plan.descendants {
        if !admitted.contains(&descendant.parent) || !admitted.insert(descendant.id) {
            return Err("NoCorpse output descendant ancestry/identity".into());
        }
        let change = changes
            .iter()
            .find(|change| change.after.id == descendant.id)
            .ok_or("NoCorpse output descendant change absent")?;
        let before = change
            .before
            .as_ref()
            .ok_or("NoCorpse output descendant source absent")?;
        let expected_place = bace_inventory::ItemPlace::Contained {
            container: descendant.parent,
            slot: descendant.slot,
            equipped: 0,
        };
        if before.place != expected_place
            || change.after.place != expected_place
            || before.pack_slot != descendant.pack_slot
            || change.after.pack_slot != descendant.pack_slot
            || before.revision.checked_add(1) != Some(descendant.revision)
            || change.after.revision != descendant.revision
        {
            return Err("NoCorpse output descendant change mismatch".into());
        }
        let row = committed
            .iter()
            .find(|row| row.object_id == descendant.id.0)
            .ok_or("NoCorpse output descendant durable row absent")?;
        let saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
        if row.mutation_revision != descendant.revision
            || saved.entity.object_id != descendant.id.0
            || saved.entity.mutation_revision != descendant.revision
            || saved.placement
                != (bace_storage_codec::ItemPlacementV2::Contained {
                    container: descendant.parent.0,
                    slot: descendant.slot,
                    pack_slot: descendant.pack_slot,
                    equipped: 0,
                })
        {
            return Err("NoCorpse output descendant durable mismatch".into());
        }
    }
    let removed_slots: Vec<_> = changes
        .iter()
        .filter(|change| roots.contains(&change.after.id))
        .filter_map(|change| {
            let before = change.before.as_ref()?;
            let bace_inventory::ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            } = before.place
            else {
                return None;
            };
            (container == actor).then_some((slot, before.pack_slot))
        })
        .collect();
    for change in changes {
        let Some(before) = &change.before else {
            continue;
        };
        if !matches!(before.place, bace_inventory::ItemPlace::Contained { container, equipped: 0, .. } if container == actor)
            && !plan
                .descendants
                .iter()
                .any(|descendant| descendant.id == change.after.id)
        {
            // A wielded root needs the exact ObjDesc, sound, tracked-child
            // retirement and Wielder IID output.
            return Ok(false);
        }
        let revision = before
            .revision
            .checked_add(1)
            .ok_or("NoCorpse pack revision overflow")?;
        let (expected_place, expected_placement) = if roots.contains(&change.after.id) {
            (
                bace_inventory::ItemPlace::World,
                bace_storage_codec::ItemPlacementV2::World(plan.accepted_position.clone()),
            )
        } else if let Some(descendant) = plan
            .descendants
            .iter()
            .find(|descendant| descendant.id == change.after.id)
        {
            (
                bace_inventory::ItemPlace::Contained {
                    container: descendant.parent,
                    slot: descendant.slot,
                    equipped: 0,
                },
                bace_storage_codec::ItemPlacementV2::Contained {
                    container: descendant.parent.0,
                    slot: descendant.slot,
                    pack_slot: descendant.pack_slot,
                    equipped: 0,
                },
            )
        } else {
            let bace_inventory::ItemPlace::Contained { slot, .. } = before.place else {
                return Ok(false);
            };
            let count = removed_slots
                .iter()
                .filter(|(removed, pack)| *pack == before.pack_slot && *removed < slot)
                .count();
            if count == 0 {
                return Ok(false);
            }
            let shifted = slot
                .checked_sub(u32::try_from(count).map_err(|_| "NoCorpse slot shift overflow")?)
                .ok_or("NoCorpse slot shift underflow")?;
            (
                bace_inventory::ItemPlace::Contained {
                    container: actor,
                    slot: shifted,
                    equipped: 0,
                },
                bace_storage_codec::ItemPlacementV2::Contained {
                    container: actor.0,
                    slot: shifted,
                    pack_slot: before.pack_slot,
                    equipped: 0,
                },
            )
        };
        let mut expected = before.clone();
        expected.place = expected_place;
        expected.revision = revision;
        if expected != change.after {
            return Err("NoCorpse pack-root/slot mutation mismatch".into());
        }
        let row = committed
            .iter()
            .find(|row| row.object_id == change.after.id.0)
            .ok_or("NoCorpse pack/slot durable row absent")?;
        let saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
        if saved.entity.object_id != row.object_id
            || saved.entity.mutation_revision != row.mutation_revision
            || saved.entity.mutation_revision != change.after.revision
            || roots.contains(&change.after.id)
                && saved.source_destination.is_none_or(|flags| flags & 10 == 0)
            || saved.placement != expected_placement
        {
            return Err("NoCorpse pack/slot receipt/source mismatch".into());
        }
    }
    Ok(true)
}

fn burden(value: u64) -> Result<i32, String> {
    i32::try_from(value).map_err(|_| "death burden overflow".into())
}

fn validate_destroyed_coin_receipt(
    source: &bace_simulation::DeathCoinSource,
    change: &bace_inventory::ItemChange,
    saved: &ItemSaveV5,
) -> Result<(), String> {
    let quantity = i32::try_from(source.after).map_err(|_| "destroyed coin stack overflow")?;
    let before = change
        .before
        .as_ref()
        .ok_or("destroyed coin source absent")?;
    let expected = match change.after.place {
        bace_inventory::ItemPlace::Removed if source.whole => {
            bace_storage_codec::ItemPlacementV2::Removed
        }
        bace_inventory::ItemPlace::Contained {
            container,
            slot,
            equipped: 0,
        } if !source.whole && before.place == change.after.place => {
            bace_storage_codec::ItemPlacementV2::Contained {
                container: container.0,
                slot,
                pack_slot: change.after.pack_slot,
                equipped: 0,
            }
        }
        _ => return Err("destroyed coin durable place invalid".into()),
    };
    if before.id != source.source
        || before.template != 273
        || before.stack != source.before
        || change.after.id != source.source
        || change.after.template != 273
        || change.after.stack != source.after
        || saved.entity.object_id != source.source.0
        || saved.entity.mutation_revision != change.after.revision
        || saved.entity.state.weenie_id != 273
        || saved.placement != expected
        || saved
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|property| property.id == 12)
            .is_none_or(|property| property.value != quantity)
    {
        return Err("destroyed coin durable receipt mismatch".into());
    }
    Ok(())
}

pub(super) fn source_drop_message(
    transcript: &bace_simulation::DeathInventoryTranscript,
    destroyed_amounts: &BTreeMap<EntityId, u32>,
    names: &BTreeMap<EntityId, (String, Option<String>)>,
) -> Result<Option<String>, String> {
    let mut entries = Vec::new();
    if transcript.coin_amount != 0 {
        let id = if transcript.pyreals_destroyed {
            transcript.coin_sources.first().map(|source| &source.source)
        } else {
            transcript.coin_drops.first()
        };
        let Some(id) = id else {
            return Ok(None);
        };
        entries.push((*id, transcript.coin_amount, true));
    }
    entries.extend(
        transcript
            .drops
            .iter()
            .map(|drop| (drop.dropped, drop.amount, false)),
    );
    for destroyed in &transcript.destroyed {
        let amount = *destroyed_amounts
            .get(&destroyed.item)
            .ok_or("death destroyed item amount absent")?;
        entries.push((destroyed.item, amount, false));
    }
    if entries.len() > 1024 {
        return Err("death drop chat capacity".into());
    }
    let mut out = String::new();
    for (index, (id, amount, coin)) in entries.iter().enumerate() {
        let (name, plural) = names.get(id).ok_or("death drop name absent")?;
        if *coin && name != "Pyreal" {
            return Ok(None);
        }
        if index == 0 {
            out.push_str("You've lost ");
        } else {
            out.push_str(", ");
            if index + 1 == entries.len() {
                out.push_str("and ");
            }
        }
        if !coin {
            out.push_str("your ");
        }
        if *amount == 1 {
            out.push_str(name);
        } else {
            out.push_str(&source_number(*amount));
            out.push(' ');
            if let Some(plural) = plural {
                out.push_str(plural);
            } else {
                out.push_str(&source_plural(name));
            }
        }
    }
    if !out.is_empty() {
        out.push('!');
    }
    if out.len() > 4096 {
        return Err("death drop chat length".into());
    }
    Ok(Some(out))
}

fn source_number(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index != 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn source_plural(name: &str) -> String {
    // ACE.Common.Extensions.StringExtensions.Pluralize at the pinned source.
    if name.ends_with("us") {
        format!("{name}s")
    } else if ["ch", "s", "sh", "x", "z"]
        .into_iter()
        .any(|ending| name.ends_with(ending))
    {
        format!("{name}es")
    } else if name.ends_with("th") {
        name.to_owned()
    } else {
        format!("{name}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Position, Property, WeenieV1};
    use bace_inventory::{InventoryItem, ItemChange, ItemPlace};
    use bace_simulation::{DeathDestroyedReceipt, DeathDropReceipt, DeathInventoryTranscript};
    use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV3, ItemSaveV4};

    #[test]
    fn corrective_no_corpse_burden_uses_private_canonical_property() {
        let binding = bace_gameplay_api::CharacterBinding {
            actor: EntityId(0x5000_0001),
            account: bace_types::AccountId(1),
            session: bace_gameplay_api::SessionId(1),
        };
        let mut events = bace_replication::EventSequencer::new(binding, 7);
        let mut actor = bace_replication::Sequences::new(10).unwrap();
        let mut items = BTreeMap::new();
        let batch = events
            .project_inventory_with_actor(
                binding,
                &[P::PrivateProperty {
                    property: 5,
                    value: PropertyValue::Int(burden(42).unwrap()),
                }],
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
                bace_replication::BatchLimits {
                    max_messages: 1,
                    max_bytes: 4096,
                    max_message_bytes: 4096,
                    max_string_bytes: 4096,
                },
            )
            .unwrap();
        assert_eq!(batch.messages.len(), 1);
        assert_eq!(batch.messages[0].queue, 9);
        assert_eq!(
            batch.messages[0].bytes,
            vec![0xcd, 0x02, 0, 0, 0, 5, 0, 0, 0, 42, 0, 0, 0]
        );
        assert!(burden(i32::MAX as u64 + 1).is_err());
    }

    #[tokio::test]
    async fn no_corpse_burden_waits_for_exact_detach_and_never_reaches_new_generation() {
        let (_cluster, _directory, mut runtime, key, binding) =
            crate::game_runtime::portals::tests::output_runtime().await;
        let actor = binding.actor;
        let next_event = runtime
            .players
            .replication(actor)
            .unwrap()
            .events
            .next_sequence();
        let limit = runtime.limits.messages;
        runtime.limits.messages = 0;
        assert!(
            !runtime
                .project_no_corpse_burden(actor, binding, 42)
                .unwrap()
        );
        runtime.limits.messages = limit;
        assert_eq!(
            runtime
                .players
                .replication(actor)
                .unwrap()
                .events
                .next_sequence(),
            next_event
        );

        runtime.sessions.get_mut(&key).unwrap().disconnected = true;
        assert!(
            !runtime
                .project_no_corpse_burden(actor, binding, 42)
                .unwrap()
        );
        assert!(runtime.network_output.is_empty());
        // A later canonical replica is generation-fenced even if it reuses
        // the same actor ID. The fixture injects only its binding change;
        // production replacement belongs to the login/detach service.
        let later_binding = bace_gameplay_api::CharacterBinding {
            session: bace_gameplay_api::SessionId(key.generation + 1),
            ..binding
        };
        runtime.players.replication(actor).unwrap().binding = later_binding;
        assert!(
            runtime
                .project_no_corpse_burden(actor, binding, 42)
                .unwrap()
        );
        assert!(runtime.network_output.is_empty());
        assert_eq!(
            runtime
                .players
                .replication(actor)
                .unwrap()
                .events
                .next_sequence(),
            next_event
        );
        runtime.players.replication(actor).unwrap().binding = binding;
        runtime
            .players
            .test_clear_replication(key, binding)
            .unwrap();
        assert!(
            runtime
                .project_no_corpse_burden(actor, binding, 42)
                .unwrap()
        );
        assert!(runtime.network_output.is_empty());
    }

    #[test]
    fn pinned_no_corpse_pack_root_has_world_create_without_dequip_packet() {
        let line = include_str!("../../../../tests/fixtures/no_corpse.trace")
            .lines()
            .find(|line| line.starts_with("1|1|0|1|0|"))
            .unwrap();
        assert!(line.contains(";world:22;"));
        assert!(!line.contains("dequip:22"));
        assert!(line.ends_with("|20,22,24|21,23"));
        let actor = EntityId(0x50000001);
        let id = EntityId(0x80000022);
        let position = Position {
            obj_cell_id: 0x12340001,
            position_x: 12.5,
            position_y: -3.25,
            position_z: 0.,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        };
        let before = InventoryItem {
            id,
            revision: 1,
            template: 100,
            stack_key: 2,
            place: ItemPlace::Contained {
                container: actor,
                slot: 0,
                equipped: 0,
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
            wield_requirements_met: false,
            structure: None,
        };
        let mut after = before.clone();
        after.place = ItemPlace::World;
        after.revision = 2;
        let plan = bace_simulation::PlayerNoCorpsePlan {
            world_roots: vec![id],
            descendants: vec![],
            accepted_position: position.clone(),
        };
        let mut saved = ItemSaveV5 {
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
                                class_name: "NoCorpse source fixture".into(),
                                weenie_type: 1,
                                last_modified: None,
                                properties: Default::default(),
                            },
                        },
                        placement: ItemPlacementV2::World(position),
                    },
                    enchantments: vec![],
                },
                construction: None,
            },
        };
        let row = |saved: &ItemSaveV5| bace_persistence::SaveSnapshot {
            object_id: saved.entity.object_id,
            mutation_revision: saved.entity.mutation_revision,
            expected_version: 1,
            bytes: saved.encode().unwrap(),
        };
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &plan,
                &[ItemChange {
                    before: Some(before.clone()),
                    after: after.clone()
                }],
                &[row(&saved)]
            )
            .unwrap()
        );
        let mut equipped = before.clone();
        equipped.place = ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: 1,
        };
        assert!(
            !supported_no_corpse_pack_completion(
                actor,
                &plan,
                &[ItemChange {
                    before: Some(equipped),
                    after: after.clone()
                }],
                &[row(&saved)]
            )
            .unwrap()
        );
        let sibling = EntityId(0x80000024);
        let mut sibling_before = before.clone();
        sibling_before.id = sibling;
        sibling_before.place = ItemPlace::Contained {
            container: actor,
            slot: 1,
            equipped: 0,
        };
        let mut sibling_after = sibling_before.clone();
        sibling_after.place = ItemPlace::Contained {
            container: actor,
            slot: 0,
            equipped: 0,
        };
        sibling_after.revision = 2;
        let mut sibling_saved = saved.clone();
        sibling_saved.entity.object_id = sibling.0;
        sibling_saved.placement = ItemPlacementV2::Contained {
            container: actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        };
        let shifted = || {
            vec![
                ItemChange {
                    before: Some(before.clone()),
                    after: after.clone(),
                },
                ItemChange {
                    before: Some(sibling_before.clone()),
                    after: sibling_after.clone(),
                },
            ]
        };
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &plan,
                &shifted(),
                &[row(&saved), row(&sibling_saved)]
            )
            .unwrap()
        );
        sibling_saved.placement = ItemPlacementV2::Contained {
            container: actor.0,
            slot: 1,
            pack_slot: false,
            equipped: 0,
        };
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &plan,
                &shifted(),
                &[row(&saved), row(&sibling_saved)]
            )
            .is_err()
        );
        let mut wrong_revision = shifted();
        wrong_revision[1].after.revision = 3;
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &plan,
                &wrong_revision,
                &[row(&saved), row(&sibling_saved)]
            )
            .is_err()
        );
        let mut container_plan = plan.clone();
        let mut container_before = before.clone();
        container_before.is_container = true;
        let mut container_after = after.clone();
        container_after.is_container = true;
        let child = EntityId(0x80000025);
        container_plan
            .descendants
            .push(bace_simulation::NoCorpseDescendant {
                id: child,
                parent: id,
                slot: 2,
                pack_slot: true,
                revision: 7,
            });
        let mut child_saved = saved.clone();
        child_saved.entity.object_id = child.0;
        child_saved.entity.mutation_revision = 7;
        child_saved.placement = ItemPlacementV2::Contained {
            container: id.0,
            slot: 2,
            pack_slot: true,
            equipped: 0,
        };
        let container_change = ItemChange {
            before: Some(container_before),
            after: container_after,
        };
        let mut child_before = before.clone();
        child_before.id = child;
        child_before.revision = 6;
        child_before.pack_slot = true;
        child_before.place = ItemPlace::Contained {
            container: id,
            slot: 2,
            equipped: 0,
        };
        let mut child_after = child_before.clone();
        child_after.revision = 7;
        let container_changes = || {
            vec![
                container_change.clone(),
                ItemChange {
                    before: Some(child_before.clone()),
                    after: child_after.clone(),
                },
            ]
        };
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &container_plan,
                &container_changes(),
                &[row(&saved), row(&child_saved)]
            )
            .unwrap()
        );
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &container_plan,
                &container_changes(),
                &[row(&saved)]
            )
            .is_err()
        );
        child_saved.placement = ItemPlacementV2::Contained {
            container: id.0,
            slot: 3,
            pack_slot: true,
            equipped: 0,
        };
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &container_plan,
                &container_changes(),
                &[row(&saved), row(&child_saved)]
            )
            .is_err()
        );
        saved.source_destination = None;
        assert!(
            supported_no_corpse_pack_completion(
                actor,
                &plan,
                &[ItemChange {
                    before: Some(before),
                    after
                }],
                &[row(&saved)]
            )
            .is_err()
        );
    }

    #[test]
    fn ace_death_drop_message_order_and_grouping() {
        // Player_Death.cs DropMessage: one coin entry precedes ordered selected
        // items; only the final list entry gains "and" after the comma.
        let transcript = DeathInventoryTranscript {
            drops: vec![
                DeathDropReceipt {
                    source: EntityId(2),
                    dropped: EntityId(2),
                    amount: 1,
                    origin: DeathDropOrigin::Pack,
                    wielded_location: 0,
                    burden_after: 0,
                },
                DeathDropReceipt {
                    source: EntityId(3),
                    dropped: EntityId(3),
                    amount: 2,
                    origin: DeathDropOrigin::Pack,
                    wielded_location: 0,
                    burden_after: 0,
                },
            ],
            destroyed: vec![],
            coin_sources: vec![],
            coin_drops: vec![EntityId(1)],
            coin_amount: 1200,
            pyreals_destroyed: false,
        };
        let names = BTreeMap::from([
            (EntityId(1), ("Pyreal".into(), Some("Pyreals".into()))),
            (EntityId(2), ("Sword".into(), Some("Swords".into()))),
            (EntityId(3), ("Arrow".into(), Some("Arrows".into()))),
        ]);
        assert_eq!(
            source_drop_message(&transcript, &BTreeMap::new(), &names)
                .unwrap()
                .as_deref(),
            Some("You've lost 1,200 Pyreals, your Sword, and your 2 Arrows!")
        );
    }

    #[test]
    fn destroyed_pyreals_still_lead_source_death_loss_chat() {
        // Player_Death.CalculateDeathItems uses SpendCurrency before its later
        // corpse_destroy_pyreals branch. It keeps those coins in dropItems for
        // DropMessage even though no coin enters the corpse.
        let transcript = DeathInventoryTranscript {
            drops: vec![],
            destroyed: vec![],
            coin_sources: vec![bace_simulation::DeathCoinSource {
                source: EntityId(1),
                before: 1200,
                after: 600,
                whole: false,
                burden_after: 0,
                coin_value_after: 600,
            }],
            coin_drops: vec![],
            coin_amount: 600,
            pyreals_destroyed: true,
        };
        let names = BTreeMap::from([(EntityId(1), ("Pyreal".into(), Some("Pyreals".into())))]);
        assert_eq!(
            source_drop_message(&transcript, &BTreeMap::new(), &names)
                .unwrap()
                .as_deref(),
            Some("You've lost 600 Pyreals!")
        );
        assert!(source_drop_message(&transcript, &BTreeMap::new(), &BTreeMap::new()).is_err());
    }

    #[test]
    fn destroyed_coin_output_requires_exact_removed_or_partial_v5_receipt() {
        let actor = EntityId(0x5000_0001);
        let id = EntityId(0x8000_0001);
        let before = InventoryItem {
            id,
            revision: 1,
            template: 273,
            stack_key: 0,
            place: ItemPlace::Contained {
                container: actor,
                slot: 0,
                equipped: 0,
            },
            stack: 2,
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
            wield_requirements_met: false,
            structure: None,
        };
        let mut after = before.clone();
        after.revision = 2;
        after.stack = 0;
        after.place = ItemPlace::Removed;
        let mut saved = ItemSaveV5 {
            source_destination: None,
            previous: ItemSaveV4 {
                previous: ItemSaveV3 {
                    previous: ItemSaveV2 {
                        entity: EntitySaveV1 {
                            object_id: id.0,
                            template_revision: 1,
                            mutation_revision: 2,
                            state: WeenieV1 {
                                schema_version: 1,
                                weenie_id: 273,
                                class_name: "pyreal".into(),
                                weenie_type: 1,
                                last_modified: None,
                                properties: Default::default(),
                            },
                        },
                        placement: ItemPlacementV2::Removed,
                    },
                    enchantments: vec![],
                },
                construction: None,
            },
        };
        saved
            .entity
            .state
            .properties
            .ints
            .push(Property { id: 12, value: 0 });
        let mut source = bace_simulation::DeathCoinSource {
            source: id,
            before: 2,
            after: 0,
            whole: true,
            burden_after: 1,
            coin_value_after: 0,
        };
        let mut change = ItemChange {
            before: Some(before.clone()),
            after: after.clone(),
        };
        assert!(validate_destroyed_coin_receipt(&source, &change, &saved).is_ok());
        saved.placement = ItemPlacementV2::Contained {
            container: actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        };
        assert!(validate_destroyed_coin_receipt(&source, &change, &saved).is_err());
        change.after.place = before.place;
        change.after.stack = 1;
        source.after = 1;
        source.whole = false;
        saved.entity.state.properties.ints[0].value = 1;
        assert!(validate_destroyed_coin_receipt(&source, &change, &saved).is_ok());
        saved.entity.mutation_revision = 3;
        assert!(validate_destroyed_coin_receipt(&source, &change, &saved).is_err());
    }

    #[test]
    fn ace_pluralize_source_branches_include_known_sarcophagus_discrepancy() {
        assert_eq!(source_plural("Sarcophagus"), "Sarcophaguss");
        assert_eq!(source_plural("Torch"), "Torches");
        assert_eq!(source_plural("Path"), "Path");
        assert_eq!(source_plural("Sword"), "Swords");
    }

    #[test]
    fn ace_destroyed_bonded_item_follows_selected_drop_in_chat() {
        // Player_Death.cs adds HandleDestroyBonded results to dropItems only
        // after the selected and slippery items, then calls DropMessage.
        let transcript = DeathInventoryTranscript {
            drops: vec![DeathDropReceipt {
                source: EntityId(2),
                dropped: EntityId(2),
                amount: 1,
                origin: DeathDropOrigin::Pack,
                wielded_location: 0,
                burden_after: 0,
            }],
            destroyed: vec![DeathDestroyedReceipt {
                item: EntityId(3),
                burden_after: 0,
            }],
            coin_sources: vec![],
            coin_drops: vec![],
            coin_amount: 0,
            pyreals_destroyed: false,
        };
        let names = BTreeMap::from([
            (EntityId(2), ("Sword".into(), Some("Swords".into()))),
            (EntityId(3), ("Arrow".into(), Some("Arrows".into()))),
        ]);
        let amounts = BTreeMap::from([(EntityId(3), 2)]);
        assert_eq!(
            source_drop_message(&transcript, &amounts, &names)
                .unwrap()
                .as_deref(),
            Some("You've lost your Sword, and your 2 Arrows!")
        );
    }
}
