//! Source-order private Buy publication after the exact durable receipt.
use super::*;
use std::collections::BTreeMap;

impl GameRuntime {
    pub(super) fn preflight_vendor_buy_output(&mut self, p: &Pending) -> Result<(), String> {
        let prepared = p
            .prepared
            .as_ref()
            .ok_or("vendor Buy preflight source missing")?;
        let ticket = p
            .ticket
            .as_ref()
            .ok_or("vendor Buy preflight ticket missing")?;
        let binding = CharacterBinding {
            actor: p.context.actor,
            account: p.context.account,
            session: p.context.session,
        };
        let loading = self
            .sessions
            .get(&p.key)
            .and_then(|session| session.loading.as_ref())
            .ok_or("vendor Buy preflight entry missing")?;
        let chargen = loading
            .character_assets
            .as_ref()
            .ok_or("vendor Buy preflight DAT missing")?
            .char_gen();
        let replica = self
            .players
            .replication(binding.actor)
            .ok_or("vendor Buy preflight replication missing")?;
        if replica.key != p.key || replica.binding != binding {
            return Err("vendor Buy preflight binding".into());
        }
        let mut events =
            bace_replication::EventSequencer::new(binding, replica.events.next_sequence());
        let mut actor = replica.properties.proposal_copy();
        let mut items = replica
            .item_properties
            .iter()
            .map(|(&id, owner)| (id, owner.proposal_copy()))
            .collect();
        let _ = project_committed(
            p,
            prepared,
            ticket,
            chargen,
            Projection {
                events: &mut events,
                actor: &mut actor,
                items: &mut items,
                max: self.limits.message_bytes,
            },
        )?;
        Ok(())
    }

    pub(in crate::game_runtime) fn project_vendor_buy_output(&mut self) -> Result<(), String> {
        const PREFIX: u64 = 0x5700_0000_0000_0000;
        const MASK: u64 = 0xff00_0000_0000_0000;
        let Some(mut p) = self.vendors.buy.take() else {
            return Ok(());
        };
        let result = (|| -> Result<bool, String> {
            if let Some(index) = self
                .reliable_admissions
                .iter()
                .position(|(_, id, _)| id & MASK == PREFIX)
            {
                let (key, id, accepted) = self.reliable_admissions[index];
                if !matches!(p.phase, Phase::Publishing(expected) if expected == id) || p.key != key
                {
                    return Err("vendor Buy publication receipt mismatch".into());
                }
                self.reliable_admissions.remove(index);
                if !accepted && let Some(session) = self.sessions.get_mut(&key) {
                    session.terminated = true;
                }
                return Ok(true);
            }
            if !matches!(p.phase, Phase::Output | Phase::Rejected) {
                return Ok(false);
            }
            if self.network_output.len() >= self.limits.messages {
                return Ok(false);
            }
            let committed = matches!(p.phase, Phase::Output);
            if committed && p.critical {
                let rows = p
                    .committed
                    .iter()
                    .filter(|row| row.object_id != p.vendor.0)
                    .cloned()
                    .collect::<Vec<_>>();
                self.online_saves
                    .finish_critical_for(&[p.context.actor.0], &rows)?;
                p.critical = false;
            }
            if self
                .sessions
                .get(&p.key)
                .is_none_or(|session| session.disconnected || session.terminated)
            {
                return Ok(true);
            }
            let binding = CharacterBinding {
                actor: p.context.actor,
                account: p.context.account,
                session: p.context.session,
            };
            let replica = self
                .players
                .replication(p.context.actor)
                .ok_or("vendor Buy replication owner missing")?;
            if replica.key != p.key || replica.binding != binding {
                return Err("vendor Buy replication binding".into());
            }
            let mut events =
                bace_replication::EventSequencer::new(binding, replica.events.next_sequence());
            let mut actor = replica.properties.proposal_copy();
            let mut items = replica
                .item_properties
                .iter()
                .map(|(&id, sequence)| (id, sequence.proposal_copy()))
                .collect::<BTreeMap<_, _>>();
            let batch = if committed {
                let prepared = p
                    .prepared
                    .as_ref()
                    .ok_or("vendor Buy prepared state missing")?;
                let ticket = p.ticket.as_ref().ok_or("vendor Buy ticket missing")?;
                let loading = self
                    .sessions
                    .get(&p.key)
                    .and_then(|session| session.loading.as_ref())
                    .ok_or("vendor Buy entry metadata missing")?;
                let chargen = loading
                    .character_assets
                    .as_ref()
                    .ok_or("vendor Buy character DAT missing")?
                    .char_gen();
                project_committed(
                    &p,
                    prepared,
                    ticket,
                    chargen,
                    Projection {
                        events: &mut events,
                        actor: &mut actor,
                        items: &mut items,
                        max: self.limits.message_bytes,
                    },
                )?
            } else {
                let steps = [
                    bace_replication::InventoryProjection::Event(
                        bace_wire::InventoryEvent::SaveFailed {
                            item_id: p.context.actor.0,
                            error: 0,
                        },
                    ),
                    bace_replication::InventoryProjection::Simple(
                        bace_wire::SimpleGameEvent::UseDone(0),
                    ),
                ];
                events
                    .project_inventory_with_actor(
                        binding,
                        &steps,
                        &mut items,
                        Some(&mut actor),
                        object_limits(self.limits.message_bytes),
                        batch_limits(self.limits.message_bytes),
                    )
                    .map_err(|e| format!("vendor Buy failure output: {e:?}"))?
            };
            if self.vendors.next_publication == !MASK {
                return Err("vendor Buy publication identity exhausted".into());
            }
            self.vendors.next_publication += 1;
            let correlation = PREFIX | self.vendors.next_publication;
            let key = p.key;
            replica.events = events;
            replica.properties = actor;
            replica.item_properties = items;
            self.network_output
                .push_back(NetworkCommand::SendReliableBatch {
                    key,
                    correlation,
                    messages: batch
                        .messages
                        .into_iter()
                        .map(|message| (message.queue, message.bytes))
                        .collect(),
                });
            p.phase = Phase::Publishing(correlation);
            Ok(false)
        })();
        match result {
            Ok(true) => Ok(()),
            Ok(false) => {
                self.vendors.buy = Some(p);
                Ok(())
            }
            Err(error) => {
                self.vendors.buy = Some(p);
                Err(error)
            }
        }
    }
}

struct Projection<'a> {
    events: &'a mut bace_replication::EventSequencer,
    actor: &'a mut bace_replication::Sequences,
    items: &'a mut BTreeMap<EntityId, bace_replication::Sequences>,
    max: usize,
}

fn project_committed(
    pending: &Pending,
    prepared: &super::buy_preparation::Prepared,
    ticket: &VendorBuyReservation,
    chargen: &bace_dat::CharGen,
    projection: Projection<'_>,
) -> Result<bace_replication::SessionBatch, String> {
    use bace_replication::InventoryProjection as P;
    use bace_wire::{InventoryEvent, PropertyValue};
    let binding = CharacterBinding {
        actor: pending.context.actor,
        account: pending.context.account,
        session: pending.context.session,
    };
    if prepared.listing.vendor_id != pending.vendor.0 || ticket.inventory.actor != binding.actor {
        return Err("vendor Buy committed output identity".into());
    }
    let player = pending
        .committed
        .iter()
        .find(|row| row.object_id == binding.actor.0)
        .ok_or("vendor Buy committed player missing")?;
    let saved_player =
        bace_storage_codec::PlayerSaveV6::decode(&player.bytes).map_err(|e| e.to_string())?;
    let final_coin = saved_player
        .player
        .entity
        .state
        .properties
        .ints
        .iter()
        .find(|p| p.id == 20)
        .map(|p| p.value)
        .ok_or("vendor Buy committed CoinValue missing")?;
    let mut coin = i64::from(final_coin)
        .checked_add(i64::from(ticket.quote.total_cost))
        .ok_or("vendor Buy starting coin overflow")?;
    let changes = &ticket.inventory.proposal.changes;
    let final_burden = i64::try_from(ticket.inventory.proposal.actor_burden)
        .map_err(|_| "vendor Buy burden overflow")?;
    let delta = changes
        .iter()
        .try_fold(0_i64, |sum, change| {
            let before = change.before.as_ref().map_or(0_i64, |item| {
                i64::from(item.stack) * i64::from(item.unit_burden)
            });
            let after = i64::from(change.after.stack) * i64::from(change.after.unit_burden);
            sum.checked_add(after - before)
        })
        .ok_or("vendor Buy burden delta overflow")?;
    let mut burden = final_burden
        .checked_sub(delta)
        .ok_or("vendor Buy starting burden overflow")?;
    let mut created = BTreeMap::new();
    let borrowed = prepared.appearance.borrowed(chargen);
    for change in changes.iter().filter(|change| change.before.is_none()) {
        let id = change.after.id;
        if projection.items.contains_key(&id) || projection.items.len() >= 1023 {
            return Err("vendor Buy fresh sequence collision/capacity".into());
        }
        let row = pending
            .committed
            .iter()
            .find(|row| row.object_id == id.0)
            .ok_or("vendor Buy committed grant missing")?;
        let saved =
            bace_storage_codec::ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
        let sequences = bace_replication::Sequences::new(256)
            .map_err(|e| format!("vendor Buy sequence: {e:?}"))?;
        let object = crate::player_entry::prepare_entry_object(
            id.0,
            &saved.entity.state,
            crate::player_entry::prepare_item_model(&saved.entity.state, &borrowed)?,
            crate::game_runtime::inventory::output::committed_state(&saved, &sequences)?,
        )?;
        projection.items.insert(id, sequences);
        created.insert(id, object);
    }
    let mut steps = Vec::with_capacity(changes.len() * 3 + 3);
    let mut removed = Vec::new();
    for change in changes {
        let after = &change.after;
        if let Some(before) = &change.before {
            if before.template != 273
                || before.id != after.id
                || after.stack >= before.stack
                || before.unit_value != 1
            {
                return Err("vendor Buy coin output identity".into());
            }
            let amount = before.stack - after.stack;
            if after.place == bace_inventory::ItemPlace::Removed {
                steps.push(P::Remove(after.id));
                removed.push(after.id);
            } else {
                steps.push(P::Stack {
                    item: after.id,
                    quantity: after.stack,
                    value: after
                        .stack
                        .checked_mul(after.unit_value)
                        .ok_or("vendor Buy stack value overflow")?,
                });
            }
            burden = burden
                .checked_sub(i64::from(amount) * i64::from(before.unit_burden))
                .ok_or("vendor Buy debit burden overflow")?;
            coin = coin
                .checked_sub(i64::from(amount))
                .ok_or("vendor Buy debit coin overflow")?;
            steps.push(P::PrivateProperty {
                property: 5,
                value: PropertyValue::Int(
                    i32::try_from(burden).map_err(|_| "vendor Buy burden range")?,
                ),
            });
            steps.push(P::PrivateProperty {
                property: 20,
                value: PropertyValue::Int(
                    i32::try_from(coin).map_err(|_| "vendor Buy coin range")?,
                ),
            });
        } else {
            let bace_inventory::ItemPlace::Contained {
                container,
                equipped: 0,
                ..
            } = after.place
            else {
                return Err("vendor Buy grant output placement".into());
            };
            steps.push(P::Create(
                created
                    .get(&after.id)
                    .ok_or("vendor Buy grant description missing")?,
            ));
            steps.push(P::Event(InventoryEvent::PutInContainer {
                object_id: after.id.0,
                container_id: container.0,
                placement: 0,
                container_type: if after.pack_slot { 2 } else { 0 },
            }));
            burden = burden
                .checked_add(i64::from(after.stack) * i64::from(after.unit_burden))
                .ok_or("vendor Buy grant burden overflow")?;
            steps.push(P::PrivateProperty {
                property: 5,
                value: PropertyValue::Int(
                    i32::try_from(burden).map_err(|_| "vendor Buy burden range")?,
                ),
            });
        }
    }
    if burden != final_burden || coin != i64::from(final_coin) {
        return Err("vendor Buy final output counters mismatch".into());
    }
    steps.push(P::Effect(bace_wire::CombatEffect::Sound {
        object_id: binding.actor.0,
        sound_id: 143,
        volume: 1.0,
    }));
    steps.push(P::VendorListing(&prepared.listing));
    steps.push(P::Simple(bace_wire::SimpleGameEvent::UseDone(0)));
    let batch = projection
        .events
        .project_inventory_with_actor(
            binding,
            &steps,
            projection.items,
            Some(projection.actor),
            object_limits(projection.max),
            batch_limits(projection.max),
        )
        .map_err(|e| format!("vendor Buy committed output: {e:?}"))?;
    for id in removed {
        projection.items.remove(&id);
    }
    Ok(batch)
}

fn object_limits(max: usize) -> bace_wire::ObjectCodecLimits {
    bace_wire::ObjectCodecLimits {
        max_message_bytes: max,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 1024,
    }
}

fn batch_limits(max: usize) -> bace_replication::BatchLimits {
    bace_replication::BatchLimits {
        // NetworkThread::try_send admits at most 256 messages and one
        // configured message budget for an atomic reliable batch.
        max_messages: 256,
        max_bytes: max,
        max_message_bytes: max,
        max_string_bytes: 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_persistence::{CharacterLease, OwnershipState};
    use bace_types::AccountId;

    fn held(key: SessionKey, phase: Phase) -> Pending {
        Pending {
            key,
            context: ActionContext {
                actor: EntityId(0x5000_0001),
                account: AccountId(1),
                session: bace_gameplay_api::SessionId(key.generation),
                sequence: 1,
            },
            vendor: EntityId(0x8000_0001),
            correlation: 1,
            epoch: 1,
            requests: vec![],
            prepared: None,
            ticket: None,
            operation: None,
            committed: vec![],
            receipt: None,
            critical: false,
            cancel_requested: false,
            failures: 0,
            phase,
        }
    }

    #[tokio::test]
    async fn buy_private_publication_retains_pressure_and_drops_disconnected_generation() {
        let (_cluster, _directory, mut runtime) =
            crate::game_runtime::tests::fixture::fixture().await;
        let key = SessionKey {
            id: 91,
            generation: 1,
        };
        runtime.vendors.buy = Some(held(key, Phase::Rejected));
        for _ in 0..runtime.limits.messages {
            runtime
                .network_output
                .push_back(NetworkCommand::Terminate { key });
        }
        runtime.project_vendor_buy_output().unwrap();
        assert!(runtime.vendors.buy.is_some());
        assert_eq!(runtime.network_output.len(), runtime.limits.messages);
        runtime.network_output.clear();
        runtime.vendors.buy = Some(held(key, Phase::Output));
        runtime.project_vendor_buy_output().unwrap();
        assert!(runtime.vendors.buy.is_none());
        assert!(runtime.network_output.is_empty());
    }

    #[test]
    fn committed_buy_preflights_coin_grant_sound_listing_and_use_done_in_one_batch() {
        let (ticket, player, owned, fresh, state) = super::super::buy_freezing::tests::fixture();
        let operation =
            super::super::buy_freezing::freeze(super::super::buy_freezing::BuyFreezeInput {
                ticket: &ticket,
                player_before: &player,
                player_expected_version: 1,
                lease: CharacterLease {
                    character_id: ticket.inventory.actor.0,
                    epoch: 1,
                    state: OwnershipState::Online,
                },
                owned: &owned,
                fresh: &fresh,
                vendor_state: &state,
                world_epoch: 2,
            })
            .unwrap();
        let (receipt, committed) = super::super::buy_freezing::receipt(
            &ticket,
            &operation,
            OperationOutcome::AlreadyCommitted,
        )
        .unwrap();
        let listing = bace_wire::VendorListing {
            vendor_id: ticket.vendor.0,
            merchandise_types: 1,
            minimum_value: 0,
            maximum_value: 1000,
            deal_magical: false,
            buy_price: 0.5,
            sell_price: 1.0,
            alternate_currency: None,
            items: vec![],
        };
        let prepared = super::super::buy_preparation::Prepared {
            source: bace_simulation::VendorBuySource {
                vendor_expected_version: 1,
                revision: 7,
                hash: [9; 32],
                sell_rate: Some(1.0),
            },
            state,
            listing,
            inventory: vec![],
            fresh,
            appearance: Default::default(),
            operation_id: ticket.operation_id.clone(),
        };
        let context = ActionContext {
            actor: ticket.inventory.actor,
            account: AccountId(1),
            session: bace_gameplay_api::SessionId(1),
            sequence: 9,
        };
        let pending = Pending {
            key: SessionKey {
                id: 1,
                generation: 1,
            },
            context,
            vendor: ticket.vendor,
            correlation: 1,
            epoch: 2,
            requests: vec![],
            prepared: None,
            ticket: Some(ticket.clone()),
            operation: Some(operation),
            committed,
            receipt: Some(receipt),
            critical: false,
            cancel_requested: false,
            failures: 0,
            phase: Phase::Output,
        };
        let binding = CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        };
        let mut events = bace_replication::EventSequencer::new(binding, 100);
        let mut actor = bace_replication::Sequences::new(256).unwrap();
        let mut items = BTreeMap::from([(
            EntityId(owned[0].entity.object_id),
            bace_replication::Sequences::new(256).unwrap(),
        )]);
        let chargen = bace_dat::CharGen {
            reserved: 0,
            starter_areas: vec![],
            heritage_marker: 0,
            heritage_groups: BTreeMap::new(),
        };
        let batch = project_committed(
            &pending,
            &prepared,
            &ticket,
            &chargen,
            Projection {
                events: &mut events,
                actor: &mut actor,
                items: &mut items,
                max: 1 << 20,
            },
        )
        .unwrap();
        assert_eq!(batch.messages.len(), 9);
        assert_eq!(
            batch
                .messages
                .iter()
                .map(|message| message.queue)
                .collect::<Vec<_>>(),
            [9, 9, 9, 10, 9, 9, 10, 9, 9]
        );
        assert_eq!(events.next_sequence(), 103);
        assert!(items.contains_key(&ticket.inventory.proposal.changes[1].after.id));
        assert_eq!(batch.messages[6].bytes[0..4], 0x0000_f750_u32.to_le_bytes());
        assert_eq!(batch.messages[7].bytes[0..4], 0x0000_f7b0_u32.to_le_bytes());
    }
}
