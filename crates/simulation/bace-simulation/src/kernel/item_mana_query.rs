//! Small read projection for an owned item's current mana. The simulation
//! thread owns this state; callers must not infer it from a saved item row.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemManaValues {
    pub current: Option<i32>,
    pub maximum: Option<i32>,
}

/// Bounded read request queued to the simulation owner after authenticated C8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemManaQueryRequest {
    pub correlation: u64,
    pub binding: CharacterBinding,
    pub target: EntityId,
}

/// The correlation is an application-owned token, never a client action ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemManaQueryOutcome {
    pub correlation: u64,
    pub result: Result<Option<ItemManaValues>, CharacterRegistrationError>,
}

impl Kernel {
    pub fn peek_item_mana_query_outcome(&self) -> Option<&ItemManaQueryOutcome> {
        self.item_mana_query_outcomes.front()
    }

    pub fn take_item_mana_query_outcome(&mut self) -> Option<ItemManaQueryOutcome> {
        self.item_mana_query_outcomes.pop_front()
    }

    pub fn has_item_mana_query_work(&self) -> bool {
        !self.item_mana_query_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|command| matches!(command, Command::ItemManaQuery(_)))
    }

    pub(super) fn handle_item_mana_query(&mut self, request: ItemManaQueryRequest) {
        self.item_mana_query_outcomes
            .push_back(self.answer_item_mana_query(request));
    }

    pub fn answer_item_mana_query(&self, request: ItemManaQueryRequest) -> ItemManaQueryOutcome {
        let result = if request.correlation == 0 {
            Err(CharacterRegistrationError::OwnershipMismatch)
        } else {
            self.current_item_mana(request.binding, request.target)
        };
        ItemManaQueryOutcome {
            correlation: request.correlation,
            result,
        }
    }

    /// Return a live value only for an exact bound owner and owned item.
    /// `None` means the item has no equipment-mana state; its immutable source
    /// properties remain the appraisal source in that case.
    pub fn current_item_mana(
        &self,
        binding: CharacterBinding,
        target: EntityId,
    ) -> Result<Option<ItemManaValues>, CharacterRegistrationError> {
        if target.0 == 0
            || !self.characters.binding_matches(binding)
            || !self.inventory.owned(binding.actor, target)
        {
            return Err(CharacterRegistrationError::OwnershipMismatch);
        }
        Ok(self
            .equipment_mana
            .players
            .get(&binding.actor)
            .and_then(|player| player.items.get(&target))
            .map(|item| ItemManaValues {
                current: item.current,
                maximum: item.maximum,
            }))
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

    #[test]
    fn current_item_mana_is_exact_owned_read_without_full_snapshot() {
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
        kernel
            .register_inventory_item(InventoryItem {
                structure: None,
                id: EntityId(2),
                revision: 1,
                template: 100,
                stack_key: 1,
                place: ItemPlace::Contained {
                    container: EntityId(1),
                    slot: 0,
                    equipped: 1,
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
            })
            .unwrap();
        assert_eq!(kernel.current_item_mana(binding(), EntityId(2)), Ok(None));
        kernel
            .register_equipment_mana(
                binding(),
                crate::EquipmentManaRecovery {
                    fresh: false,
                    heartbeat: 5.0,
                    rating: 0,
                    heartbeat_remaining: 5.0,
                    items: vec![crate::EquipmentManaItem {
                        item: EntityId(2),
                        name: "Training Wand".into(),
                        current: Some(40),
                        maximum: Some(100),
                        rate: Some(-0.5),
                        affecting: Some(true),
                        removals: Vec::new(),
                        accumulator: 0.0,
                        warned: false,
                        removal_remaining: None,
                    }],
                },
            )
            .unwrap();
        kernel
            .equipment_mana
            .players
            .get_mut(&EntityId(1))
            .unwrap()
            .items
            .get_mut(&EntityId(2))
            .unwrap()
            .current = Some(39);
        assert_eq!(
            kernel.current_item_mana(binding(), EntityId(2)),
            Ok(Some(ItemManaValues {
                current: Some(39),
                maximum: Some(100),
            }))
        );
        assert_eq!(
            kernel.answer_item_mana_query(ItemManaQueryRequest {
                correlation: 77,
                binding: binding(),
                target: EntityId(2),
            }),
            ItemManaQueryOutcome {
                correlation: 77,
                result: Ok(Some(ItemManaValues {
                    current: Some(39),
                    maximum: Some(100),
                })),
            }
        );
        assert_eq!(
            kernel
                .answer_item_mana_query(ItemManaQueryRequest {
                    correlation: 0,
                    binding: binding(),
                    target: EntityId(2),
                })
                .result,
            Err(CharacterRegistrationError::OwnershipMismatch)
        );
        let mut wrong = binding();
        wrong.session = SessionId(8);
        assert_eq!(
            kernel.current_item_mana(wrong, EntityId(2)),
            Err(CharacterRegistrationError::OwnershipMismatch)
        );
        assert_eq!(
            kernel.current_item_mana(binding(), EntityId(3)),
            Err(CharacterRegistrationError::OwnershipMismatch)
        );
    }

    #[test]
    fn batch_outcomes_fill_capacity_and_retain_next_request() {
        let mut kernel =
            Kernel::with_gameplay_limits(bace_world::World::default(), 2048, 1, 1024).unwrap();
        for correlation in 1..=1025 {
            assert!(
                kernel
                    .try_enqueue(Command::ItemManaQuery(ItemManaQueryRequest {
                        correlation,
                        binding: binding(),
                        target: EntityId(2),
                    }))
                    .is_ok()
            );
        }
        kernel.step().unwrap();
        assert_eq!(kernel.item_mana_query_outcomes.len(), 1024);
        assert_eq!(kernel.commands.len(), 1);
        assert_eq!(
            kernel.peek_item_mana_query_outcome().unwrap().correlation,
            1
        );
        assert_eq!(
            kernel.take_item_mana_query_outcome().unwrap().correlation,
            1
        );
        kernel.step().unwrap();
        assert_eq!(kernel.item_mana_query_outcomes.len(), 1024);
        assert!(kernel.commands.is_empty());
        for correlation in 2..=1025 {
            assert_eq!(
                kernel.take_item_mana_query_outcome().unwrap().correlation,
                correlation
            );
        }
        assert!(!kernel.has_item_mana_query_work());
    }
}
