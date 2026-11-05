//! Bounded, immutable decisions from pinned ACE `ManaStone.HandleActionUseOnTarget`.
//! The caller must reserve and durably commit every returned item change before
//! adopting it or projecting the corresponding client output.
use crate::{InventoryProposal, InventoryView, ItemChange, ItemPlace};
use bace_gameplay_api::InventoryRejection;
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaCharge {
    pub current: Option<i32>,
    pub maximum: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaStoneItem {
    pub id: EntityId,
    pub mana: ManaCharge,
    pub retained: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManaStoneProfile {
    pub efficiency: Option<f64>,
    pub destroy_chance: Option<f64>,
    pub unlimited_use: bool,
    /// ACE `Player.LumAugItemManaGain`, each rank contributing five rating.
    pub mana_gain_rank: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManaStoneFailure {
    ActionCancelled,
    ItemDoesntHaveEnoughMana,
    InvalidState,
    Capacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaChange {
    pub item: EntityId,
    pub before: ManaCharge,
    pub after: ManaCharge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManaStoneDecision {
    /// ACE sends a successful UseDone without a mutation for a full target.
    AlreadyFull,
    /// A stone with ItemCurMana=0 also completes without changing anything.
    Empty,
    Extract {
        target: EntityId,
        drained: i32,
        stone: ManaChange,
    },
    Pour {
        given: i32,
        remaining_need: i32,
        targets: Vec<ManaChange>,
        stone: ManaChange,
        destroy_stone: bool,
    },
    NoEquippedItemsNeedMana,
}

/// `destroy_roll` is one pre-sampled uniform draw in [0,1). ACE samples only
/// after a successful pour; it samples even for an unlimited-use stone.
pub fn propose_mana_stone_use(
    stone: ManaStoneItem,
    target: Option<ManaStoneItem>,
    equipped: &[ManaStoneItem],
    profile: ManaStoneProfile,
    destroy_roll: Option<f64>,
) -> Result<ManaStoneDecision, ManaStoneFailure> {
    if stone.id.0 == 0
        || stone.mana.current.is_some_and(|value| value < 0)
        || stone.mana.maximum.is_some_and(|value| value < 0)
        || profile
            .efficiency
            .is_some_and(|value| !value.is_finite() || value < 0.)
        || profile
            .destroy_chance
            .is_some_and(|value| !value.is_finite() || !(0. ..=1.).contains(&value))
        || profile.mana_gain_rank < 0
        || profile.mana_gain_rank > 100
        || destroy_roll.is_some_and(|value| !value.is_finite() || !(0. ..1.).contains(&value))
        || equipped.len() > 1024
    {
        return Err(ManaStoneFailure::InvalidState);
    }
    if let Some(target) = target {
        if target.id.0 == 0 || target.id == stone.id || !equipped.is_empty() {
            return Err(ManaStoneFailure::InvalidState);
        }
        validate_target(target)?;
    } else {
        let mut ids = std::collections::BTreeSet::new();
        for item in equipped {
            validate_target(*item)?;
            if item.id == stone.id || !ids.insert(item.id) {
                return Err(ManaStoneFailure::InvalidState);
            }
        }
    }
    match stone.mana.current {
        None => {
            let target = target.ok_or(ManaStoneFailure::ActionCancelled)?;
            if target.retained {
                return Err(ManaStoneFailure::ActionCancelled);
            }
            let (Some(current), Some(maximum)) = (target.mana.current, target.mana.maximum) else {
                return Err(ManaStoneFailure::ItemDoesntHaveEnoughMana);
            };
            if current <= 0 || maximum <= 0 {
                return Err(ManaStoneFailure::ItemDoesntHaveEnoughMana);
            }
            let efficiency = profile.efficiency.ok_or(ManaStoneFailure::InvalidState)?;
            let drained = checked_round(f64::from(current) * efficiency)?;
            Ok(ManaStoneDecision::Extract {
                target: target.id,
                drained,
                stone: ManaChange {
                    item: stone.id,
                    before: stone.mana,
                    after: ManaCharge {
                        current: Some(drained),
                        maximum: stone.mana.maximum,
                    },
                },
            })
        }
        Some(0) => Ok(ManaStoneDecision::Empty),
        Some(current) => {
            let mut source = current;
            let mut changes = Vec::new();
            let mut given = 0i32;
            let mut needed = 0i32;
            if let Some(target) = target {
                let Some(maximum) = target.mana.maximum.filter(|value| *value > 0) else {
                    return Err(ManaStoneFailure::ActionCancelled);
                };
                let before = target.mana.current.unwrap_or(0);
                if before >= maximum {
                    return Ok(ManaStoneDecision::AlreadyFull);
                }
                let need = maximum - before;
                let ration = source.min(need);
                // ACE's single-item branch scales the poured amount but does
                // not subtract the pre-scaled ration from the stone.
                given = scale_gain(ration, profile.mana_gain_rank)?.min(need);
                changes.push(ManaChange {
                    item: target.id,
                    before: target.mana,
                    after: ManaCharge {
                        current: Some(before + given),
                        maximum: target.mana.maximum,
                    },
                });
                needed = need - given;
            } else {
                let mut values: Vec<_> = equipped
                    .iter()
                    .filter(|item| {
                        matches!((item.mana.current,item.mana.maximum), (Some(c),Some(m)) if c < m)
                    })
                    .map(|item| (item.id, item.mana, item.mana.current.unwrap()))
                    .collect();
                if values.is_empty() {
                    return Ok(ManaStoneDecision::NoEquippedItemsNeedMana);
                }
                while source > 0 {
                    let count = values
                        .iter()
                        .filter(|(_, mana, value)| *value < mana.maximum.unwrap())
                        .count();
                    if count == 0 {
                        break;
                    }
                    let ration = (source
                        / i32::try_from(count).map_err(|_| ManaStoneFailure::Capacity)?)
                    .max(1);
                    for (_, mana, value) in &mut values {
                        let need = mana.maximum.unwrap() - *value;
                        if need == 0 {
                            continue;
                        }
                        let adjusted = ration.min(need);
                        source -= adjusted;
                        let augmented = scale_gain(adjusted, profile.mana_gain_rank)?;
                        let gained = augmented.min(need);
                        // ACE returns the gain clamped by a full target to the
                        // stone. The unclamped augmentation bonus is not spent.
                        if gained < augmented {
                            source = source
                                .checked_add(augmented - gained)
                                .ok_or(ManaStoneFailure::Capacity)?;
                        }
                        *value += gained;
                        given = given
                            .checked_add(gained)
                            .ok_or(ManaStoneFailure::Capacity)?;
                        if source <= 0 {
                            break;
                        }
                    }
                }
                for (id, before, after) in values {
                    needed = needed
                        .checked_add(before.maximum.unwrap() - after)
                        .ok_or(ManaStoneFailure::Capacity)?;
                    if before.current != Some(after) {
                        changes.push(ManaChange {
                            item: id,
                            before,
                            after: ManaCharge {
                                current: Some(after),
                                maximum: before.maximum,
                            },
                        });
                    }
                }
            }
            let chance = profile.destroy_chance.unwrap_or(0.);
            // A null ACE DestroyChance still takes a random draw (nullable
            // equality with zero is false), though it cannot destroy.
            let destroyed = if profile.destroy_chance == Some(0.) {
                false
            } else {
                destroy_roll.ok_or(ManaStoneFailure::InvalidState)? < chance
            };
            let after = if destroyed {
                stone.mana
            } else if profile.unlimited_use {
                ManaCharge {
                    current: stone.mana.maximum,
                    maximum: stone.mana.maximum,
                }
            } else {
                ManaCharge {
                    current: None,
                    maximum: stone.mana.maximum,
                }
            };
            Ok(ManaStoneDecision::Pour {
                given,
                remaining_need: needed,
                targets: changes,
                stone: ManaChange {
                    item: stone.id,
                    before: stone.mana,
                    after,
                },
                destroy_stone: destroyed,
            })
        }
    }
}

/// Reserve the exact item revisions touched by a source decision. The caller
/// must freeze PropertyInt 107 and any removed placement in the same durable
/// operation, then adopt the mana owner only after its receipt.
pub fn propose_mana_stone_inventory(
    actor: EntityId,
    stone: EntityId,
    decision: &ManaStoneDecision,
    view: InventoryView<'_>,
) -> Result<Option<InventoryProposal>, InventoryRejection> {
    view.validate()?;
    let (mana, consume, destroy_stone): (&[ManaChange], Option<EntityId>, bool) = match decision {
        ManaStoneDecision::AlreadyFull
        | ManaStoneDecision::Empty
        | ManaStoneDecision::NoEquippedItemsNeedMana => return Ok(None),
        ManaStoneDecision::Extract {
            target, stone: s, ..
        } => {
            if s.item != stone || *target == stone || s.before.current.is_some() {
                return Err(InventoryRejection::InvalidState);
            }
            (std::slice::from_ref(s), Some(*target), false)
        }
        ManaStoneDecision::Pour {
            targets,
            stone: s,
            destroy_stone,
            ..
        } => {
            if s.item != stone || s.before.current.is_none() {
                return Err(InventoryRejection::InvalidState);
            }
            // Include the stone first, then every target in source order.
            let mut changes = Vec::with_capacity(targets.len() + 1);
            changes.push(mana_change(actor, stone, *destroy_stone, &view)?);
            for target in targets {
                if target.item == stone {
                    return Err(InventoryRejection::InvalidState);
                }
                changes.push(mana_change(actor, target.item, false, &view)?);
            }
            return crate::propose_item_changes(actor, changes, view).map(Some);
        }
    };
    let mut changes = vec![mana_change(actor, mana[0].item, destroy_stone, &view)?];
    if let Some(target) = consume {
        changes.push(mana_change(actor, target, true, &view)?);
    }
    crate::propose_item_changes(actor, changes, view).map(Some)
}

fn mana_change(
    actor: EntityId,
    item: EntityId,
    consume: bool,
    view: &InventoryView<'_>,
) -> Result<ItemChange, InventoryRejection> {
    let before = view.item(item)?;
    if view.owner(before)? != Some(actor)
        || before.trade_reserved
        || before.active_pet
        || before.is_container
        || before.stack == 0
    {
        return Err(InventoryRejection::OwnershipMismatch);
    }
    let mut after = before.clone();
    if consume {
        after.stack -= 1;
        if after.stack == 0 {
            after.place = ItemPlace::Removed;
        }
    }
    after.revision = after
        .revision
        .checked_add(1)
        .ok_or(InventoryRejection::Overflow)?;
    Ok(ItemChange {
        before: Some(before.clone()),
        after,
    })
}

fn validate_target(item: ManaStoneItem) -> Result<(), ManaStoneFailure> {
    if item.id.0 == 0
        || item.mana.current.is_some_and(|value| value < 0)
        || item.mana.maximum.is_some_and(|value| value < 0)
    {
        return Err(ManaStoneFailure::InvalidState);
    }
    Ok(())
}

fn checked_round(value: f64) -> Result<i32, ManaStoneFailure> {
    if !value.is_finite() || value < 0. || value > f64::from(i32::MAX) {
        return Err(ManaStoneFailure::Capacity);
    }
    // .NET Math.Round defaults to ties-to-even.
    Ok(value.round_ties_even() as i32)
}

fn scale_gain(amount: i32, rank: i32) -> Result<i32, ManaStoneFailure> {
    let rating = rank.checked_mul(5).ok_or(ManaStoneFailure::Capacity)?;
    let multiplier = (100 + rating) as f32 / 100.0;
    // ACE's int * float expression rounds to f32 before Math.Round receives
    // the resulting value as a double.
    checked_round(f64::from(amount as f32 * multiplier))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InventoryContainer, InventoryItem};

    fn item(id: u32, current: Option<i32>, maximum: Option<i32>) -> ManaStoneItem {
        ManaStoneItem {
            id: EntityId(id),
            mana: ManaCharge { current, maximum },
            retained: false,
        }
    }
    fn profile() -> ManaStoneProfile {
        ManaStoneProfile {
            efficiency: Some(0.5),
            destroy_chance: Some(0.),
            unlimited_use: false,
            mana_gain_rank: 0,
        }
    }

    #[test]
    fn empty_stone_extracts_with_ace_bankers_rounding() {
        // ManaStone.cs uses Math.Round(Efficiency * ItemCurMana), which is
        // ties-to-even, then consumes one unit of the charged target.
        let decision = propose_mana_stone_use(
            item(1, None, Some(100)),
            Some(item(2, Some(9), Some(10))),
            &[],
            profile(),
            None,
        )
        .unwrap();
        assert_eq!(
            decision,
            ManaStoneDecision::Extract {
                target: EntityId(2),
                drained: 4,
                stone: ManaChange {
                    item: EntityId(1),
                    before: ManaCharge {
                        current: None,
                        maximum: Some(100),
                    },
                    after: ManaCharge {
                        current: Some(4),
                        maximum: Some(100),
                    },
                },
            }
        );
        let mut retained = item(2, Some(9), Some(10));
        retained.retained = true;
        assert_eq!(
            propose_mana_stone_use(item(1, None, None), Some(retained), &[], profile(), None),
            Err(ManaStoneFailure::ActionCancelled)
        );
    }

    #[test]
    fn charged_stone_pours_with_ace_augmentation_and_resets_to_empty() {
        let mut augmented = profile();
        augmented.mana_gain_rank = 2; // ACE rating = 10; mod = 1.1f.
        let decision = propose_mana_stone_use(
            item(1, Some(20), Some(50)),
            Some(item(2, Some(10), Some(100))),
            &[],
            augmented,
            None,
        )
        .unwrap();
        assert_eq!(
            decision,
            ManaStoneDecision::Pour {
                given: 22,
                remaining_need: 68,
                targets: vec![ManaChange {
                    item: EntityId(2),
                    before: ManaCharge {
                        current: Some(10),
                        maximum: Some(100),
                    },
                    after: ManaCharge {
                        current: Some(32),
                        maximum: Some(100),
                    },
                }],
                stone: ManaChange {
                    item: EntityId(1),
                    before: ManaCharge {
                        current: Some(20),
                        maximum: Some(50),
                    },
                    after: ManaCharge {
                        current: None,
                        maximum: Some(50),
                    },
                },
                destroy_stone: false,
            }
        );
    }

    #[test]
    fn source_destruction_roll_precedes_unlimited_refill() {
        let mut p = profile();
        p.unlimited_use = true;
        p.destroy_chance = Some(0.3);
        let destroyed = propose_mana_stone_use(
            item(1, Some(10), Some(50)),
            Some(item(2, Some(0), Some(30))),
            &[],
            p,
            Some(0.2),
        )
        .unwrap();
        assert!(matches!(
            destroyed,
            ManaStoneDecision::Pour {
                destroy_stone: true,
                ..
            }
        ));
        let retained = propose_mana_stone_use(
            item(1, Some(10), Some(50)),
            Some(item(2, Some(0), Some(30))),
            &[],
            p,
            Some(0.7),
        )
        .unwrap();
        assert!(matches!(
            retained,
            ManaStoneDecision::Pour {
                stone: ManaChange {
                    after: ManaCharge {
                        current: Some(50),
                        ..
                    },
                    ..
                },
                destroy_stone: false,
                ..
            }
        ));
    }

    #[test]
    fn self_target_distributes_rations_in_equipment_order() {
        let decision = propose_mana_stone_use(
            item(1, Some(5), Some(20)),
            None,
            &[item(2, Some(0), Some(10)), item(3, Some(8), Some(10))],
            profile(),
            None,
        )
        .unwrap();
        assert!(
            matches!(decision, ManaStoneDecision::Pour { given: 5, remaining_need: 7, targets, .. } if targets.iter().map(|c| c.after.current.unwrap()).collect::<Vec<_>>() == vec![3,10])
        );
    }

    #[test]
    fn extraction_reserves_stone_and_consumed_target_in_one_proposal() {
        let inventory = [inventory_item(1, 1), inventory_item(2, 1)];
        let containers = [InventoryContainer {
            id: EntityId(99),
            revision: 1,
            root_owner: Some(EntityId(99)),
            slots: 10,
            pack_slots: 2,
            burden_limit: 1000,
            accessible: true,
            open: false,
            generation: 1,
        }];
        let decision = propose_mana_stone_use(
            item(1, None, Some(100)),
            Some(item(2, Some(20), Some(50))),
            &[],
            profile(),
            None,
        )
        .unwrap();
        let proposal = propose_mana_stone_inventory(
            EntityId(99),
            EntityId(1),
            &decision,
            InventoryView {
                items: &inventory,
                containers: &containers,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(proposal.changes.len(), 2);
        assert_eq!(proposal.changes[0].after.revision, 2);
        assert_eq!(proposal.changes[1].after.stack, 0);
        assert_eq!(proposal.changes[1].after.place, ItemPlace::Removed);
    }

    fn inventory_item(id: u32, stack: u32) -> InventoryItem {
        InventoryItem {
            structure: None,
            id: EntityId(id),
            revision: 1,
            template: id,
            stack_key: u64::from(id),
            place: ItemPlace::Contained {
                container: EntityId(99),
                slot: id - 1,
                equipped: 0,
            },
            stack,
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
        }
    }
}
