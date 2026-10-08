use super::*;
use crate::player_death::{
    DeathCoinSource, DeathDestroyedReceipt, DeathDropOrigin, DeathDropReceipt,
    DeathInventoryTranscript,
};
use bace_interactions::{
    PlayerDeathKind, classify_player_death, death_item_count, select_death_items,
};
use bace_inventory::{InventoryItem, InventoryView, ItemChange, ItemPlace};
impl Kernel {
    pub(super) fn death_inventory_plan(
        &self,
        prepared: &crate::PreparedPlayerDeath,
        kind: PlayerDeathKind,
        level: u32,
    ) -> Result<
        (
            bace_inventory::InventoryProposal,
            Vec<EntityId>,
            Option<DeathInventoryTranscript>,
        ),
        E,
    > {
        let actor = prepared.actor;
        if prepared.possessions.len() > 1024
            || prepared.fresh_stacks.len() > 1024
            || prepared.coin_stacks.len() > 1024
            || prepared.equipped_health.len() > 1024
        {
            return Err(E::Capacity);
        }
        let owned: Vec<_> = self
            .inventory
            .items()
            .filter(|i| self.inventory.owned(actor, i.id))
            .collect();
        if owned.len()!=prepared.possessions.len()||prepared.possessions.iter().any(|p|owned.iter().filter(|i|i.id==p.id&&i.template==p.template&&i.stack==p.stack&&matches!(i.place,ItemPlace::Contained{equipped,..} if (equipped!=0)==p.wielded)).count()!=1) {return Err(E::Stale);}
        let equipped: Vec<_> = owned
            .iter()
            .filter(|i| matches!(i.place,ItemPlace::Contained{equipped,..} if equipped!=0))
            .map(|i| i.id)
            .collect();
        if prepared.equipped_health.len() != equipped.len()
            || equipped.iter().any(|id| {
                prepared
                    .equipped_health
                    .iter()
                    .filter(|(candidate, _)| candidate == id)
                    .count()
                    != 1
            })
        {
            return Err(E::MissingAssets);
        }
        let killer = self
            .player_deaths
            .pending
            .get(&actor)
            .and_then(|p| p.killer);
        if self.olthoi_death_kind(actor, killer).is_some() || prepared.olthoi.is_some() {
            return self
                .olthoi_death_inventory(prepared)
                .map(|(proposal, dropped)| (proposal, dropped, None));
        }
        let mut fresh_ids = std::collections::BTreeSet::new();
        fresh_ids.insert(prepared.corpse.id);
        let mut sources = std::collections::BTreeSet::new();
        for (source, item) in &prepared.fresh_stacks {
            if !sources.insert(*source)
                || !owned.iter().any(|i| i.id == *source)
                || !fresh_ids.insert(item.id)
                || self.inventory.item(item.id).is_some()
                || self.world.contains_identity(item.id)
            {
                return Err(E::Invalid);
            }
        }
        for item in &prepared.coin_stacks {
            if !fresh_ids.insert(item.id)
                || self.inventory.item(item.id).is_some()
                || self.world.contains_identity(item.id)
            {
                return Err(E::Invalid);
            }
        }
        let block = self.world.actor_state(actor).map_err(|_| E::Invalid)?.0;
        let suppress =
            kind == PlayerDeathKind::Pkl || self.world_policies.prevents_death_item_loss(block.0);
        let root = self.player_deaths.random.as_ref().ok_or(E::MissingAssets)?;
        let mut event = [0; 16];
        event[..8].copy_from_slice(&self.player_deaths.epoch.to_le_bytes());
        event[8..].copy_from_slice(&prepared.operation.to_le_bytes());
        let mut random = root
            .event_stream(event, bace_random::Domain::PlayerDeath)
            .and_then(|r| r.fork(b"character", u64::from(actor.0)))
            .map_err(|_| E::Invalid)?;
        let draw = if suppress || level <= 10 {
            0
        } else {
            random
                .below(if level <= 20 { 2 } else { 3 })
                .map_err(|_| E::Invalid)? as u32
        };
        let aug = self.death_int(actor, 231).unwrap_or(0).clamp(0, 3) as u32;
        let count = death_item_count(level, draw, aug, kind == PlayerDeathKind::Pk)
            .map_err(|_| E::Invalid)?;
        let coins = owned
            .iter()
            .filter(|i| i.template == 273)
            .try_fold(0u32, |n, i| n.checked_add(i.stack))
            .ok_or(E::Invalid)?;
        let plan = select_death_items(&prepared.possessions, level, count, coins, suppress, || {
            random
                .next_u64()
                .map(|v| (-0.1 + ((v >> 11) as f64 / 9007199254740992.) * 0.2) as f32)
                .map_err(|_| bace_interactions::DeathItemError::Random)
        })
        .map_err(|_| E::Invalid)?;
        let mut transcript = DeathInventoryTranscript {
            drops: Vec::with_capacity(plan.drops.len()),
            destroyed: plan
                .destroyed
                .iter()
                .map(|&item| DeathDestroyedReceipt {
                    item,
                    burden_after: 0,
                })
                .collect(),
            coin_sources: Vec::new(),
            coin_drops: Vec::new(),
            coin_amount: plan.coins,
            pyreals_destroyed: self.death_policy.destroy_pyreals,
        };
        let mut changes = std::collections::BTreeMap::<EntityId, ItemChange>::new();
        let mut dropped = Vec::new();
        // ACE adds collected pyreals before the selected/slippery items. Reserve
        // their corpse slots before placing those later selections.
        let mut slots = [0u32; 2];
        if !self.death_policy.destroy_pyreals {
            let mut coin_preview = plan.coins;
            for source in owned.iter().copied().filter(|i| i.template == 273) {
                if coin_preview == 0 {
                    break;
                }
                let amount = coin_preview.min(source.stack);
                coin_preview -= amount;
                let lane = if amount == source.stack {
                    source.pack_slot
                } else {
                    prepared
                        .coin_stacks
                        .first()
                        .ok_or(E::MissingAssets)?
                        .pack_slot
                };
                slots[usize::from(lane)] =
                    slots[usize::from(lane)].checked_add(1).ok_or(E::Capacity)?;
            }
        }
        {
            let mut place = |item: &mut InventoryItem| {
                let lane = usize::from(item.pack_slot);
                item.place = ItemPlace::Contained {
                    container: prepared.corpse.id,
                    slot: slots[lane],
                    equipped: 0,
                };
                slots[lane] += 1;
                dropped.push(item.id);
            };
            for drop in &plan.drops {
                let before = self.inventory.item(drop.item).ok_or(E::Stale)?.clone();
                let mut after = before.clone();
                let equipped =
                    matches!(before.place, ItemPlace::Contained { equipped, .. } if equipped != 0);
                let slippery = prepared
                    .possessions
                    .iter()
                    .any(|p| p.id == before.id && p.bonded == -1);
                let origin = if drop.fresh_template {
                    DeathDropOrigin::Split
                } else if slippery && equipped {
                    DeathDropOrigin::SlipperyWield
                } else if slippery {
                    DeathDropOrigin::SlipperyPack
                } else if equipped {
                    DeathDropOrigin::Wield
                } else {
                    DeathDropOrigin::Pack
                };
                let mut dropped_id = before.id;
                if drop.fresh_template {
                    let mut fresh = prepared
                        .fresh_stacks
                        .iter()
                        .find(|(source, _)| *source == before.id)
                        .ok_or(E::MissingAssets)?
                        .1
                        .clone();
                    if fresh.template != before.template
                        || fresh.id == before.id
                        || fresh.stack != drop.amount
                        || fresh.revision != 0
                        || self.world.contains_identity(fresh.id)
                        || self.inventory.item(fresh.id).is_some()
                    {
                        return Err(E::Invalid);
                    }
                    place(&mut fresh);
                    dropped_id = fresh.id;
                    changes.insert(
                        fresh.id,
                        ItemChange {
                            before: None,
                            after: fresh,
                        },
                    );
                    after.stack = after.stack.checked_sub(drop.amount).ok_or(E::Invalid)?;
                } else {
                    place(&mut after);
                }
                transcript.drops.push(DeathDropReceipt {
                    source: before.id,
                    dropped: dropped_id,
                    amount: drop.amount,
                    origin,
                    wielded_location: match before.place {
                        ItemPlace::Contained { equipped, .. } => equipped,
                        _ => return Err(E::Invalid),
                    },
                    burden_after: 0,
                });
                changes.insert(
                    before.id,
                    ItemChange {
                        before: Some(before),
                        after,
                    },
                );
            }
        }
        for id in plan.destroyed {
            let before = self.inventory.item(id).ok_or(E::Stale)?.clone();
            let mut after = before.clone();
            after.place = ItemPlace::Removed;
            after.stack = 0;
            changes.insert(
                id,
                ItemChange {
                    before: Some(before),
                    after,
                },
            );
        }
        let mut remaining = plan.coins;
        let mut partial_coins = 0u32;
        let mut coin_slots = [0u32; 2];
        for before in owned.iter().copied().filter(|i| i.template == 273) {
            if remaining == 0 {
                break;
            }
            let amount = remaining.min(before.stack);
            remaining -= amount;
            let mut after = before.clone();
            let whole = amount == before.stack;
            if whole {
                if self.death_policy.destroy_pyreals {
                    after.stack = 0;
                    after.place = ItemPlace::Removed;
                } else {
                    let lane = usize::from(after.pack_slot);
                    after.place = ItemPlace::Contained {
                        container: prepared.corpse.id,
                        slot: coin_slots[lane],
                        equipped: 0,
                    };
                    coin_slots[lane] = coin_slots[lane].checked_add(1).ok_or(E::Capacity)?;
                    transcript.coin_drops.push(after.id);
                }
            } else {
                after.stack -= amount;
                partial_coins = amount;
            }
            transcript.coin_sources.push(DeathCoinSource {
                source: before.id,
                before: before.stack,
                after: after.stack,
                whole,
                burden_after: 0,
                coin_value_after: 0,
            });
            changes.insert(
                before.id,
                ItemChange {
                    before: Some(before.clone()),
                    after,
                },
            );
        }
        if remaining != 0 {
            return Err(E::Invalid);
        }
        if !self.death_policy.destroy_pyreals && partial_coins > 0 {
            let mut remaining = partial_coins;
            for fresh in &prepared.coin_stacks {
                if remaining == 0 {
                    break;
                }
                let mut fresh = fresh.clone();
                if fresh.template != 273
                    || fresh.revision != 0
                    || fresh.maximum_stack == 0
                    || self.inventory.item(fresh.id).is_some()
                    || self.world.contains_identity(fresh.id)
                {
                    return Err(E::Invalid);
                }
                fresh.stack = remaining.min(fresh.maximum_stack);
                remaining -= fresh.stack;
                let lane = usize::from(fresh.pack_slot);
                fresh.place = ItemPlace::Contained {
                    container: prepared.corpse.id,
                    slot: coin_slots[lane],
                    equipped: 0,
                };
                coin_slots[lane] = coin_slots[lane].checked_add(1).ok_or(E::Capacity)?;
                transcript.coin_drops.push(fresh.id);
                changes.insert(
                    fresh.id,
                    ItemChange {
                        before: None,
                        after: fresh,
                    },
                );
            }
            if remaining != 0 {
                return Err(E::MissingAssets);
            }
        }
        let mut ordered = transcript.coin_drops.clone();
        ordered.extend(dropped);
        let dropped = ordered;
        if changes
            .insert(
                prepared.corpse_item.id,
                ItemChange {
                    before: None,
                    after: prepared.corpse_item.clone(),
                },
            )
            .is_some()
        {
            return Err(E::Invalid);
        }
        let items: Vec<_> = self.inventory.items().cloned().collect();
        let mut containers: Vec<_> = self.inventory.containers().copied().collect();
        containers.push(prepared.corpse_container);
        let proposal = bace_inventory::propose_item_changes(
            actor,
            changes.into_values().collect(),
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .map_err(|_| E::Invalid)?;
        freeze_transcript_totals(
            actor,
            coins,
            &items,
            &containers,
            &proposal,
            &mut transcript,
        )?;
        Ok((proposal, dropped, Some(transcript)))
    }
    pub(super) fn death_int(&self, actor: EntityId, property: u32) -> Option<i32> {
        match self
            .world
            .properties(actor)?
            .get(bace_entity::PropertyFamily::Int, property)?
        {
            bace_entity::PropertyValue::Int(v) => Some(*v),
            _ => None,
        }
    }
    pub(super) fn death_kind(
        &self,
        actor: EntityId,
        killer: Option<EntityId>,
    ) -> Result<PlayerDeathKind, E> {
        let state = self
            .player_deaths
            .states
            .get(&actor)
            .ok_or(E::MissingAssets)?;
        Ok(classify_player_death(
            actor.0,
            state.pk_status,
            killer.map(|id| id.0),
        ))
    }
}

fn freeze_transcript_totals(
    actor: EntityId,
    starting_coins: u32,
    baseline: &[InventoryItem],
    containers: &[bace_inventory::InventoryContainer],
    proposal: &bace_inventory::InventoryProposal,
    transcript: &mut DeathInventoryTranscript,
) -> Result<(), E> {
    let mut working = baseline.to_vec();
    let mut coins = starting_coins;
    let mut apply = |id: EntityId| -> Result<u64, E> {
        let after = proposal
            .changes
            .iter()
            .find(|c| c.after.id == id)
            .ok_or(E::Invalid)?
            .after
            .clone();
        let target = working
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(E::Stale)?;
        *target = after;
        let burden = InventoryView {
            items: &working,
            containers,
        }
        .actor_burden(actor)
        .map_err(|_| E::Invalid)?;
        if burden > i32::MAX as u64 {
            return Err(E::Capacity);
        }
        Ok(burden)
    };
    // HandleDestroyBonded executes before SpendCurrency and ordered drops.
    for destroyed in &mut transcript.destroyed {
        destroyed.burden_after = apply(destroyed.item)?;
    }
    // CollectCurrencyStacks adjusts the one partial source while collecting;
    // SpendCurrency subsequently removes whole source stacks in source order.
    for source in transcript.coin_sources.iter_mut().filter(|s| !s.whole) {
        source.burden_after = apply(source.source)?;
        coins = coins
            .checked_sub(source.before - source.after)
            .ok_or(E::Invalid)?;
        source.coin_value_after = coins;
    }
    for source in transcript.coin_sources.iter_mut().filter(|s| s.whole) {
        source.burden_after = apply(source.source)?;
        coins = coins.checked_sub(source.before).ok_or(E::Invalid)?;
        source.coin_value_after = coins;
    }
    for drop in &mut transcript.drops {
        drop.burden_after = apply(drop.source)?;
    }
    let final_burden = InventoryView {
        items: &working,
        containers,
    }
    .actor_burden(actor)
    .map_err(|_| E::Invalid)?;
    if final_burden != proposal.actor_burden
        || coins
            != starting_coins
                .checked_sub(transcript.coin_amount)
                .ok_or(E::Invalid)?
    {
        return Err(E::Invalid);
    }
    Ok(())
}
