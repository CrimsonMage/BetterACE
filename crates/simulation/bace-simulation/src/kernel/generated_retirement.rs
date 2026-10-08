//! Durable destruction of generated objects. The lifecycle FIFO retains its
//! effect until an exact epoch-fenced tombstone receipt has been adopted.
use super::*;
use bace_gameplay_api::{GeneratorLifecycleEffect, InventoryRejection as E};
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedRetirementTicket {
    pub npc_source_ticket: Option<u64>,
    pub inventory: crate::InventoryTicket,
    pub effect: GeneratorLifecycleEffect,
    pub transient: Vec<EntityId>,
    /// Accepted physical poses captured before reserving retirement.
    pub positions: BTreeMap<EntityId, bace_gameplay_api::GeneratorLocation>,
    pub enchantments: BTreeMap<EntityId, Vec<bace_magic::EnchantmentEntry>>,
    pub registry_revisions: BTreeMap<EntityId, u64>,
}
pub(super) struct PendingRetirement {
    ticket: GeneratedRetirementTicket,
    submitted: bool,
}
impl Kernel {
    pub(in crate::kernel) fn claim_scripted_retirement(
        &mut self,
        effect: &GeneratorLifecycleEffect,
        npc_ticket: u64,
    ) -> Result<GeneratedRetirementTicket, crate::GeneratorServiceError> {
        if npc_ticket == 0 {
            return Err(crate::GeneratorServiceError::Invalid);
        }
        let source = self
            .npcs
            .pending_service(npc_ticket)
            .filter(|p| {
                matches!(
                    p.effect,
                    crate::NpcEffect::Service(bace_gameplay_api::NpcOperation::DeleteSelf)
                )
            })
            .ok_or(crate::GeneratorServiceError::Invalid)?
            .context
            .source;
        self.stage_generated_retirement_with_source(effect, Some(source))?;
        let pending = self
            .generated_retirements
            .values_mut()
            .find(|p| p.ticket.effect == *effect)
            .ok_or(crate::GeneratorServiceError::Missing)?;
        if pending.submitted {
            return Err(crate::GeneratorServiceError::Busy);
        }
        pending.ticket.npc_source_ticket = Some(npc_ticket);
        pending.submitted = true;
        Ok(pending.ticket.clone())
    }
    pub(super) fn stage_generated_retirement(
        &mut self,
        effect: &GeneratorLifecycleEffect,
    ) -> Result<(), crate::GeneratorServiceError> {
        self.stage_generated_retirement_with_source(effect, None)
    }
    fn stage_generated_retirement_with_source(
        &mut self,
        effect: &GeneratorLifecycleEffect,
        scripted_source: Option<EntityId>,
    ) -> Result<(), crate::GeneratorServiceError> {
        use crate::GeneratorServiceError as G;
        if self
            .generated_retirements
            .values()
            .any(|p| &p.ticket.effect == effect)
        {
            return Ok(());
        }
        if self.generated_retirements.len() >= self.outcome_capacity {
            return Err(G::Capacity);
        }
        let GeneratorLifecycleEffect::DestroyMember {
            generator, member, ..
        } = effect
        else {
            return Err(G::Invalid);
        };
        self.prepare_inventory_time().map_err(|_| G::Busy)?;
        let proposal = retirement_proposal(&self.inventory, member.entity).map_err(|e| {
            if e == E::Capacity {
                G::Capacity
            } else {
                G::Busy
            }
        })?;
        if scripted_source.is_some_and(|source| source != member.entity)
            || proposal.changes.iter().any(|change| {
                change.after.place == ItemPlace::Removed
                    && Some(change.after.id) != scripted_source
                    && !self.npcs.can_retire_idle_source(change.after.id)
            })
        {
            return Err(G::Busy);
        }
        let entry_count = proposal
            .changes
            .iter()
            .filter_map(|c| self.magic.registry(c.after.id))
            .try_fold(0usize, |n, r| n.checked_add(r.entries().len()))
            .ok_or(G::Capacity)?;
        if entry_count > 65536 {
            return Err(G::Capacity);
        }
        let mut enchantments = BTreeMap::new();
        let mut registry_revisions = BTreeMap::new();
        for change in &proposal.changes {
            if let Some(registry) = self.magic.registry(change.after.id) {
                enchantments.insert(change.after.id, registry.entries().to_vec());
                registry_revisions.insert(change.after.id, registry.revision());
            }
        }
        let mut positions = BTreeMap::new();
        for before in proposal.changes.iter().filter_map(|c| c.before.as_ref()) {
            if before.place == ItemPlace::World {
                let (cell, state) = self.world.actor_state(before.id).map_err(|_| G::Geometry)?;
                let p = state.position();
                let heading = state.heading_radians() * 0.5;
                positions.insert(
                    before.id,
                    bace_gameplay_api::GeneratorLocation {
                        cell: cell.0,
                        origin: [p.x, p.y, p.z],
                        rotation: [0., 0., heading.sin(), heading.cos()],
                    },
                );
            }
        }
        let operation = self
            .inventory
            .reserve(generator.entity, proposal)
            .map_err(|e| {
                if e == E::Capacity {
                    G::Capacity
                } else {
                    G::Busy
                }
            })?;
        if let Err(error) = self.reserve_inventory_registries(operation, &[]) {
            self.inventory.reject(operation).map_err(|_| G::Busy)?;
            return Err(if error == E::Capacity {
                G::Capacity
            } else {
                G::Busy
            });
        }
        self.inventory.claim(operation).map_err(|_| G::Busy)?;
        let transient = self
            .inventory
            .generated_changes(operation)
            .map_err(|_| G::Busy)?;
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .ok_or(G::Stale)?
            .clone();
        self.generated_retirements.insert(
            operation,
            PendingRetirement {
                ticket: GeneratedRetirementTicket {
                    npc_source_ticket: None,
                    inventory,
                    effect: effect.clone(),
                    transient,
                    positions,
                    enchantments,
                    registry_revisions,
                },
                submitted: false,
            },
        );
        Ok(())
    }
    pub fn take_generated_retirement(&mut self) -> Option<GeneratedRetirementTicket> {
        let pending = self
            .generated_retirements
            .values_mut()
            .find(|p| !p.submitted)?;
        pending.submitted = true;
        Some(pending.ticket.clone())
    }
    pub fn retry_generated_retirement(&mut self, operation: u64) -> Result<(), E> {
        if self.npcs.owns_generated_retirement(operation) {
            return Err(E::DurabilityPending);
        }
        let pending = self
            .generated_retirements
            .get_mut(&operation)
            .ok_or(E::InvalidState)?;
        if !pending.submitted {
            return Err(E::Busy);
        }
        pending.submitted = false;
        Ok(())
    }
    /// Only a definite database rollback authorizes dropping the frozen ticket.
    /// The retained lifecycle effect can prepare a fresh ticket on a later tick.
    pub fn reject_generated_retirement(&mut self, operation: u64) -> Result<(), E> {
        if self.npcs.owns_generated_retirement(operation) {
            return Err(E::DurabilityPending);
        }
        self.reject_generated_retirement_inner(operation)
    }
    pub(in crate::kernel) fn reject_generated_retirement_inner(
        &mut self,
        operation: u64,
    ) -> Result<(), E> {
        let pending = self
            .generated_retirements
            .get(&operation)
            .ok_or(E::InvalidState)?;
        if !pending.submitted {
            return Err(E::InvalidState);
        }
        self.preflight_inventory_registries(operation, false)?;
        self.inventory.reject(operation)?;
        self.release_inventory_registries(operation)?;
        self.generated_retirements.remove(&operation);
        Ok(())
    }
    pub fn confirm_generated_retirement(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<GeneratedRetirementTicket, E> {
        if self.npcs.owns_generated_retirement(receipt.operation) {
            return Err(E::DurabilityPending);
        }
        self.confirm_generated_retirement_inner(receipt)
    }
    pub(in crate::kernel) fn confirm_generated_retirement_inner(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<GeneratedRetirementTicket, E> {
        self.confirm_generated_retirement_with_hold(receipt, None)
    }
    pub(in crate::kernel) fn confirm_generated_retirement_with_hold(
        &mut self,
        receipt: &crate::InventoryReceipt,
        hold: Option<bace_world::WorldRetirementHold>,
    ) -> Result<GeneratedRetirementTicket, E> {
        let pending = self
            .generated_retirements
            .get(&receipt.operation)
            .ok_or(E::InvalidState)?;
        if !pending.submitted {
            return Err(E::InvalidState);
        }
        if let Some(hold) = hold {
            if pending.ticket.npc_source_ticket != Some(hold.operation)
                || self.world.retirement_hold(hold.actor) != Some(hold)
                || self
                    .world
                    .has_reserved_vitals_except(hold.actor, hold.vital_token())
                || !pending
                    .ticket
                    .inventory
                    .proposal
                    .changes
                    .iter()
                    .any(|c| c.after.id == hold.actor && c.after.place == ItemPlace::Removed)
            {
                return Err(E::DurabilityPending);
            }
        } else if pending.ticket.npc_source_ticket.is_some() {
            return Err(E::DurabilityPending);
        }
        if pending
            .ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|change| {
                change.after.place == ItemPlace::Removed
                    && self.world.contains_identity(change.after.id)
                    && hold.is_none_or(|h| h.actor != change.after.id)
                    && !self.npcs.can_retire_idle_source(change.after.id)
            })
        {
            return Err(E::DurabilityPending);
        }
        if pending
            .ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|change| {
                change.after.place == ItemPlace::Removed
                    && self.world.contains_identity(change.after.id)
                    && (self.world.health_observation_pending_for(change.after.id)
                        || hold.is_none_or(|h| h.actor != change.after.id)
                            && (self.world.has_reserved_vitals(change.after.id)
                                || self.world.retirement_hold(change.after.id).is_some()))
            })
        {
            return Err(E::DurabilityPending);
        }
        self.preflight_inventory_registries(receipt.operation, true)?;
        self.inventory.validate_receipt(receipt)?;
        let accepted = pending.ticket.clone();
        let ticket = self.inventory.confirm(receipt)?;
        self.retire_inventory_registries(&ticket)?;
        self.release_inventory_registries(receipt.operation)?;
        let removed: Vec<_> = ticket
            .proposal
            .changes
            .iter()
            .filter(|change| change.after.place == ItemPlace::Removed)
            .map(|change| change.after.id)
            .collect();
        self.inventory.retire_generated_metadata(&removed);
        self.inventory
            .adopt_generated_durability(&accepted.transient);
        for id in removed {
            if let Some(hold) = hold.filter(|h| h.actor == id) {
                self.world
                    .remove_retired(hold)
                    .map_err(|_| E::DurabilityPending)?;
            } else if self.world.remove(id).is_some() {
                self.npcs
                    .retire_idle_source(id)
                    .expect("preflighted idle script");
            }
        }
        self.generated_retirements.remove(&receipt.operation);
        Ok(accepted)
    }
}

// This autonomous removal has no character burden authority. Its only changes
// are a complete owned tree and exact sibling slot compaction; no item can be
// granted, moved to another owner, or destroyed inside a player's carried tree.
pub(in crate::kernel) fn retirement_proposal(
    inventory: &crate::inventory::Inventory,
    root: EntityId,
) -> Result<InventoryProposal, E> {
    let root_item = inventory.item(root).ok_or(E::MissingItem)?;
    let mut ancestry = BTreeMap::new();
    add_ancestors(inventory, root_item, &mut ancestry)?;
    let mut ids = BTreeSet::from([root]);
    for depth in 0..64 {
        let before = ids.len();
        for item in inventory.items() {
            if let ItemPlace::Contained { container, .. } = item.place
                && ids.contains(&container)
            {
                ids.insert(item.id);
            }
        }
        if ids.len() > 1024 {
            return Err(E::Capacity);
        }
        if ids.len() == before {
            break;
        }
        if depth == 63 {
            return Err(E::InvalidState);
        }
    }
    let mut changes = Vec::new();
    for before in inventory.items() {
        let mut after = before.clone();
        if ids.contains(&before.id) {
            after.place = ItemPlace::Removed;
        } else if let (
            ItemPlace::Contained {
                container: parent,
                slot: removed,
                equipped: 0,
            },
            ItemPlace::Contained {
                container,
                slot,
                equipped: 0,
            },
        ) = (root_item.place, before.place)
            && parent == container
            && before.pack_slot == root_item.pack_slot
            && slot > removed
        {
            after.place = ItemPlace::Contained {
                container,
                slot: slot - 1,
                equipped: 0,
            };
        } else {
            continue;
        }
        if before.trade_reserved || before.active_pet || inventory.reserved(before.id) {
            return Err(E::DurabilityPending);
        }
        after.revision = after.revision.checked_add(1).ok_or(E::Overflow)?;
        add_ancestors(inventory, before, &mut ancestry)?;
        changes.push(ItemChange {
            before: Some(before.clone()),
            after,
        });
        if changes.len() > 1024 || ancestry.len() > 1024 {
            return Err(E::Capacity);
        }
    }
    Ok(InventoryProposal {
        changes,
        participants: ancestry.into_iter().collect(),
        actor_burden: 0,
        requires_pickup_motion: false,
    })
}
fn add_ancestors(
    inventory: &crate::inventory::Inventory,
    item: &InventoryItem,
    participants: &mut BTreeMap<EntityId, u64>,
) -> Result<(), E> {
    participants.insert(item.id, item.revision);
    let mut place = item.place;
    let mut seen = BTreeSet::from([item.id]);
    for _ in 0..64 {
        let ItemPlace::Contained { container, .. } = place else {
            return Ok(());
        };
        if !seen.insert(container) {
            return Err(E::InvalidState);
        }
        let metadata = inventory.container(container).ok_or(E::MissingContainer)?;
        if metadata.root_owner.is_some() {
            return Err(E::AccessDenied);
        }
        if let Some(parent) = inventory.item(container) {
            participants.insert(container, parent.revision);
            place = parent.place;
        } else {
            participants.insert(container, metadata.revision);
            return Ok(());
        }
    }
    Err(E::InvalidState)
}
