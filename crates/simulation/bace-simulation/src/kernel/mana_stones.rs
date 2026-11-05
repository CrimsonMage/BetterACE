//! One simulation owner reserves both inventory revisions and live mana before
//! a valuable placement. Rejected saves leave all accepted mana unchanged.
use super::*;
use crate::mana_stones::{
    ManaStoneCommand as C, ManaStoneError as E, ManaStoneOutcome, ManaStoneResult as R,
    ManaStoneTicket,
};
use bace_gameplay_api::CharacterBinding;
use bace_inventory::{
    InventoryView, ManaCharge, ManaStoneDecision, ManaStoneFailure, ManaStoneItem,
    ManaStoneProfile, propose_mana_stone_inventory, propose_mana_stone_use,
};

impl Kernel {
    pub fn take_mana_stone_outcome(&mut self) -> Option<ManaStoneOutcome> {
        self.mana_stones.outcomes.pop_front()
    }
    pub fn has_mana_stone_work(&self) -> bool {
        !self.mana_stones.pending.is_empty()
            || !self.mana_stones.outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::ManaStone(_)))
    }
    pub(super) fn apply_mana_stone_command(&mut self, command: C) -> ManaStoneOutcome {
        let correlation = command.correlation();
        let context = match &command {
            C::Use { context, .. } => Some(*context),
            _ => None,
        };
        let result = match command {
            C::Use {
                context,
                stone,
                stone_revision,
                target,
                target_revision,
                profile,
                ..
            } => self.propose_mana_stone(
                correlation,
                context,
                stone,
                stone_revision,
                target,
                target_revision,
                profile,
            ),
            C::Commit { receipt, .. } => self.commit_mana_stone(&receipt).map(R::Committed),
            C::Reject { operation, .. } => self.reject_mana_stone(operation).map(R::Rejected),
        };
        ManaStoneOutcome {
            correlation,
            context,
            result,
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "one exact source UseWithTarget capture"
    )]
    fn propose_mana_stone(
        &mut self,
        correlation: u64,
        context: ActionContext,
        mut stone: ManaStoneItem,
        stone_revision: u64,
        mut target: ManaStoneItem,
        target_revision: u64,
        profile: ManaStoneProfile,
    ) -> Result<R, E> {
        if !self.characters.binding_matches(CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        }) {
            return Err(E::NotBound);
        }
        if stone.id == target.id || target.id == context.actor {
            return Err(E::InvalidTarget);
        }
        if self.mana_stones.pending.len() >= self.mana_stones.capacity
            || self.mana_stones.outcomes.len() >= self.mana_stones.capacity
            || self.characters.reserved(context.actor)
            || self.inventory.reserved(context.actor)
            || self.equipment_mana_pending(context.actor)
            || self.magic.busy(context.actor)
        {
            return Err(E::Busy);
        }
        let accepted_stone = self.inventory.item(stone.id).ok_or(E::Ownership)?;
        let accepted_target = self.inventory.item(target.id).ok_or(E::Ownership)?;
        if !self.inventory.owned(context.actor, stone.id)
            || !self.inventory.owned(context.actor, target.id)
        {
            return Err(E::Ownership);
        }
        if accepted_stone.revision != stone_revision
            || accepted_target.revision != target_revision
            || self.inventory.reserved(stone.id)
            || self.inventory.reserved(target.id)
        {
            return Err(E::Stale);
        }
        if let Some(live) = self.equipment_mana.players.get(&context.actor) {
            if let Some(value) = live.items.get(&stone.id) {
                stone.mana = ManaCharge {
                    current: value.current,
                    maximum: value.maximum,
                };
            }
            if let Some(value) = live.items.get(&target.id) {
                target.mana = ManaCharge {
                    current: value.current,
                    maximum: value.maximum,
                };
            }
        }
        // Consuming an equipped target also needs the equipment-effect and
        // physical registry composite. Hold that branch until it can be
        // included in the same durable operation.
        if stone.mana.current.is_none()
            && matches!(accepted_target.place, bace_inventory::ItemPlace::Contained { equipped, .. } if equipped != 0)
        {
            return Err(E::InvalidTarget);
        }
        let destroy_roll = if profile.destroy_chance == Some(0.) || stone.mana.current.is_none() {
            None
        } else {
            let root = self.social.random.as_ref().ok_or(E::Capacity)?;
            let mut identity = [0; 16];
            identity[..8].copy_from_slice(&correlation.to_le_bytes());
            identity[8..].copy_from_slice(&self.tick.to_le_bytes());
            let mut stream = root
                .event_stream(identity, bace_random::Domain::Magic)
                .and_then(|stream| stream.fork(b"mana-stone", u64::from(stone.id.0)))
                .map_err(|_| E::Capacity)?;
            Some((stream.next_u64().map_err(|_| E::Capacity)? >> 11) as f64 / ((1u64 << 53) as f64))
        };
        let decision = propose_mana_stone_use(stone, Some(target), &[], profile, destroy_roll)
            .map_err(map_failure)?;
        if matches!(
            decision,
            ManaStoneDecision::AlreadyFull
                | ManaStoneDecision::Empty
                | ManaStoneDecision::NoEquippedItemsNeedMana
        ) {
            return Ok(R::Immediate(decision));
        }
        let items: Vec<_> = self
            .inventory
            .items()
            .filter(|item| self.inventory.owned(context.actor, item.id))
            .cloned()
            .collect();
        let containers: Vec<_> = self
            .inventory
            .containers()
            .filter(|container| container.root_owner == Some(context.actor))
            .copied()
            .collect();
        let proposal = propose_mana_stone_inventory(
            context.actor,
            stone.id,
            &decision,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .map_err(|_| E::InvalidSource)?
        .ok_or(E::InvalidSource)?;
        let operation = self
            .stage_inventory(context.actor, |inventory| {
                inventory.reserve(context.actor, proposal)
            })
            .map_err(|_| E::Busy)?;
        if self.inventory.claim(operation).is_err() {
            self.release_inventory_registries(operation)
                .map_err(|_| E::Receipt)?;
            self.inventory.reject(operation).map_err(|_| E::Receipt)?;
            return Err(E::Receipt);
        }
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::Receipt)?
            .clone();
        let actor_revision = self
            .characters
            .get(context.actor)
            .ok_or(E::NotBound)?
            .revision();
        let ticket = ManaStoneTicket {
            context,
            actor_revision,
            inventory,
            decision,
        };
        self.mana_stones.pending.insert(operation, ticket.clone());
        Ok(R::Proposed(ticket))
    }
    fn commit_mana_stone(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<ManaStoneTicket, E> {
        let ticket = self
            .mana_stones
            .pending
            .get(&receipt.operation)
            .ok_or(E::Receipt)?
            .clone();
        self.inventory
            .validate_receipt(receipt)
            .map_err(|_| E::Receipt)?;
        let actor = ticket.context.actor;
        if let Some(live) = self.equipment_mana.players.get(&actor) {
            for change in mana_changes(&ticket.decision) {
                if let Some(item) = live.items.get(&change.item)
                    && (item.current != change.before.current
                        || item.maximum != change.before.maximum)
                {
                    return Err(E::Stale);
                }
            }
        }
        self.confirm_inventory_committed_inner(receipt)
            .map_err(|_| E::Receipt)?;
        if let Some(live) = self.equipment_mana.players.get_mut(&actor) {
            for change in mana_changes(&ticket.decision) {
                if let Some(item) = live.items.get_mut(&change.item) {
                    item.current = change.after.current;
                }
            }
            if let ManaStoneDecision::Extract { target, .. } = ticket.decision
                && !self.inventory.owned(actor, target)
            {
                live.items.remove(&target);
            }
            if matches!(
                ticket.decision,
                ManaStoneDecision::Pour {
                    destroy_stone: true,
                    ..
                }
            ) {
                let stone = ticket.inventory.proposal.changes[0].after.id;
                if !self.inventory.owned(actor, stone) {
                    live.items.remove(&stone);
                }
            }
        }
        self.mana_stones.pending.remove(&receipt.operation);
        Ok(ticket)
    }
    fn reject_mana_stone(&mut self, operation: u64) -> Result<u64, E> {
        if !self.mana_stones.owns(operation) {
            return Err(E::Receipt);
        }
        self.release_inventory_registries(operation)
            .map_err(|_| E::Receipt)?;
        self.inventory.reject(operation).map_err(|_| E::Receipt)?;
        self.mana_stones.pending.remove(&operation);
        Ok(operation)
    }
    pub(super) fn validate_mana_stone_snapshot(
        &self,
        binding: CharacterBinding,
        operation: u64,
        revision: u64,
    ) -> bool {
        self.mana_stones
            .pending
            .get(&operation)
            .is_some_and(|ticket| {
                ticket.context.actor == binding.actor
                    && ticket.context.account == binding.account
                    && ticket.context.session == binding.session
                    && ticket.actor_revision == revision
                    && self
                        .characters
                        .get(binding.actor)
                        .is_some_and(|c| c.revision() == revision)
                    && self.inventory.pending_ticket(operation) == Some(&ticket.inventory)
                    && self.inventory.reserved(binding.actor)
            })
    }
}

fn mana_changes(decision: &ManaStoneDecision) -> Vec<bace_inventory::ManaChange> {
    match decision {
        ManaStoneDecision::Extract { stone, .. } => vec![*stone],
        ManaStoneDecision::Pour { stone, targets, .. } => {
            let mut changes = Vec::with_capacity(targets.len() + 1);
            changes.push(*stone);
            changes.extend(targets);
            changes
        }
        _ => Vec::new(),
    }
}

fn map_failure(error: ManaStoneFailure) -> E {
    match error {
        ManaStoneFailure::ActionCancelled => E::InvalidTarget,
        ManaStoneFailure::ItemDoesntHaveEnoughMana => E::NeedsMana,
        ManaStoneFailure::InvalidState => E::InvalidSource,
        ManaStoneFailure::Capacity => E::Capacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_character::{CharacterProgression, ProgressionTables, RankTable};
    use bace_gameplay_api::SessionId;
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    use bace_types::AccountId;
    use std::sync::Arc;

    fn binding() -> CharacterBinding {
        CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(7),
        }
    }
    fn context() -> ActionContext {
        ActionContext {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(7),
            sequence: 1,
        }
    }
    fn item(id: u32, equipped: u32) -> InventoryItem {
        InventoryItem {
            structure: None,
            id: EntityId(id),
            revision: 1,
            template: id,
            stack_key: u64::from(id),
            place: ItemPlace::Contained {
                container: EntityId(1),
                slot: id - 2,
                equipped,
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
            valid_wield: 1,
            incompatible_wield: 0,
            wield_requirements_met: true,
        }
    }
    fn setup() -> Kernel {
        let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
        let ranks = RankTable::new(&[0, 10]).unwrap();
        kernel
            .register_character(
                binding(),
                CharacterProgression::new(
                    &[],
                    Arc::new(ProgressionTables {
                        attributes: ranks.clone(),
                        vitals: ranks.clone(),
                        trained_skills: ranks.clone(),
                        specialized_skills: ranks,
                    }),
                    0,
                    0,
                )
                .unwrap(),
            )
            .unwrap();
        kernel
            .register_inventory_container(InventoryContainer {
                id: EntityId(1),
                revision: 1,
                root_owner: Some(EntityId(1)),
                slots: 10,
                pack_slots: 2,
                burden_limit: 1000,
                accessible: true,
                open: false,
                generation: 1,
            })
            .unwrap();
        kernel.register_inventory_item(item(2, 0)).unwrap();
        kernel.register_inventory_item(item(3, 1)).unwrap();
        kernel
            .register_equipment_mana(
                binding(),
                crate::EquipmentManaRecovery {
                    fresh: false,
                    heartbeat: 5.0,
                    rating: 0,
                    heartbeat_remaining: 5.0,
                    items: vec![
                        crate::EquipmentManaItem {
                            item: EntityId(2),
                            name: "Stone".into(),
                            current: Some(20),
                            maximum: Some(50),
                            rate: None,
                            affecting: None,
                            removals: Vec::new(),
                            accumulator: 0.,
                            warned: false,
                            removal_remaining: None,
                        },
                        crate::EquipmentManaItem {
                            item: EntityId(3),
                            name: "Wand".into(),
                            current: Some(10),
                            maximum: Some(100),
                            rate: None,
                            affecting: None,
                            removals: Vec::new(),
                            accumulator: 0.,
                            warned: false,
                            removal_remaining: None,
                        },
                    ],
                },
            )
            .unwrap();
        kernel
    }
    fn use_command(correlation: u64) -> C {
        C::Use {
            correlation,
            context: context(),
            stone: ManaStoneItem {
                id: EntityId(2),
                mana: ManaCharge {
                    current: Some(20),
                    maximum: Some(50),
                },
                retained: false,
            },
            stone_revision: 1,
            target: ManaStoneItem {
                id: EntityId(3),
                mana: ManaCharge {
                    current: Some(10),
                    maximum: Some(100),
                },
                retained: false,
            },
            target_revision: 1,
            profile: ManaStoneProfile {
                efficiency: Some(1.),
                destroy_chance: Some(0.),
                unlimited_use: false,
                mana_gain_rank: 0,
            },
        }
    }

    #[test]
    fn mana_stone_rejected_save_preserves_live_mana_and_exact_receipt_adopts() {
        let mut kernel = setup();
        let first = kernel.apply_mana_stone_command(use_command(11));
        let Ok(R::Proposed(first)) = first.result else {
            panic!("first proposal");
        };
        assert_eq!(
            kernel
                .current_item_mana(binding(), EntityId(3))
                .unwrap()
                .unwrap()
                .current,
            Some(10)
        );
        assert!(
            kernel
                .apply_mana_stone_command(C::Commit {
                    correlation: 12,
                    receipt: crate::InventoryReceipt {
                        operation: first.inventory.operation,
                        revisions: vec![(EntityId(2), 99)]
                    }
                })
                .result
                .is_err()
        );
        assert_eq!(
            kernel
                .current_item_mana(binding(), EntityId(3))
                .unwrap()
                .unwrap()
                .current,
            Some(10)
        );
        assert!(matches!(
            kernel
                .apply_mana_stone_command(C::Reject {
                    correlation: 13,
                    operation: first.inventory.operation
                })
                .result,
            Ok(R::Rejected(_))
        ));
        assert_eq!(kernel.inventory.item(EntityId(3)).unwrap().revision, 1);
        let second = kernel.apply_mana_stone_command(use_command(14));
        let Ok(R::Proposed(second)) = second.result else {
            panic!("second proposal");
        };
        let receipt = crate::InventoryReceipt {
            operation: second.inventory.operation,
            revisions: second
                .inventory
                .proposal
                .changes
                .iter()
                .map(|change| (change.after.id, change.after.revision))
                .collect(),
        };
        assert!(matches!(
            kernel
                .apply_mana_stone_command(C::Commit {
                    correlation: 15,
                    receipt
                })
                .result,
            Ok(R::Committed(_))
        ));
        assert_eq!(
            kernel
                .current_item_mana(binding(), EntityId(3))
                .unwrap()
                .unwrap()
                .current,
            Some(30)
        );
        assert_eq!(kernel.inventory.item(EntityId(3)).unwrap().revision, 2);
    }
}
