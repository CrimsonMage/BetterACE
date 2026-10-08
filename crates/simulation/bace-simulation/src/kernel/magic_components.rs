//! Bounded cast/inventory handoff. No-burn reservations have no durable mutation
//! and complete locally; burns require the exact persisted inventory/mana receipt.
use super::*;
impl Kernel {
    pub(super) fn service_magic_components(&mut self) {
        for _ in 0..32 {
            let Some(pending) = self.magic.pending_components() else {
                break;
            };
            let mut resources = match self.magic.prepare_component_resources(
                pending.actor,
                pending.cast,
                &self.world,
                self.tick as f64 / 30.0,
            ) {
                Ok(resources) => resources,
                Err(bace_gameplay_api::CastRejection::Capacity) => break,
                Err(error) => {
                    self.magic
                        .fail_component_request(pending.actor, pending.cast, error);
                    continue;
                }
            };
            match self.propose_component_use(pending.actor, &pending.required, &pending.consumed) {
                Ok(operation) => {
                    // Registry preparation can advance the character revision at a
                    // heartbeat. Capture the aggregate fence only after those
                    // owner mutations and the inventory reservation have succeeded.
                    let Some(before) = self.characters.get(pending.actor).map(|c| c.revision())
                    else {
                        self.inventory
                            .reject(operation)
                            .expect("unsubmitted component proposal");
                        self.release_inventory_registries(operation)
                            .expect("owned component registries");
                        self.magic.fail_component_request(
                            pending.actor,
                            pending.cast,
                            bace_gameplay_api::CastRejection::MissingActor,
                        );
                        continue;
                    };
                    let Some(after) = before.checked_add(1) else {
                        self.inventory
                            .reject(operation)
                            .expect("unsubmitted component proposal");
                        self.release_inventory_registries(operation)
                            .expect("owned component registries");
                        self.magic.fail_component_request(
                            pending.actor,
                            pending.cast,
                            bace_gameplay_api::CastRejection::InvalidState,
                        );
                        continue;
                    };
                    resources.before_revision = before;
                    resources.after_revision = after;
                    let token = bace_world::VitalReservationToken {
                        domain: bace_world::VitalReservationDomain::SpellComponents,
                        operation,
                    };
                    if self
                        .world
                        .reserve_vitals(&[(pending.actor, bace_entity::EntityVital::Mana)], token)
                        .is_err()
                    {
                        // Nothing has been published or submitted. Undo only this
                        // operation's inventory and registry ownership.
                        self.inventory
                            .reject(operation)
                            .expect("unsubmitted component proposal");
                        self.release_inventory_registries(operation)
                            .expect("owned component registries");
                        break;
                    }
                    self.magic.bind_component_operation(operation, resources);
                    if self
                        .inventory
                        .pending_ticket(operation)
                        .is_some_and(|t| t.proposal.changes.is_empty())
                    {
                        // No item/save row changes: retain all requirement reservations,
                        // validate them on this same owner, then release locally.
                        self.inventory
                            .claim(operation)
                            .expect("new unsubmitted reservation");
                        let receipt = crate::InventoryReceipt {
                            operation,
                            revisions: Vec::new(),
                        };
                        self.confirm_inventory_committed_inner(&receipt)
                            .expect("preflighted no-change component reservation");
                    } else {
                        // The named magic durability lane owns this proposal;
                        // generic inventory drains must never steal its receipt.
                        self.inventory
                            .claim(operation)
                            .expect("new magic burn proposal");
                    }
                }
                Err(bace_gameplay_api::InventoryRejection::Requirements) => {
                    self.magic.fail_component_request(
                        pending.actor,
                        pending.cast,
                        bace_gameplay_api::CastRejection::MissingComponents,
                    )
                }
                Err(_) => break,
            }
        }
    }
    pub fn pending_magic_resources(
        &self,
        operation: u64,
    ) -> Option<&crate::magic::MagicResourceCommit> {
        self.magic.component_resources(operation)
    }
}

#[cfg(test)]
#[path = "magic_components_tests.rs"]
mod tests;
