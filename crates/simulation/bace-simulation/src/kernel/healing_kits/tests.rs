use super::*;
use bace_character::*;
use bace_gameplay_api::{CharacterBinding, ProgressionTarget, SessionId, TraitDetails};
use bace_inventory::{InventoryContainer, InventoryItem};
use bace_motion::{
    ExecutionClip, MotionPhysics, RootFrame, SourceMotionState, SourceMotionTransition,
};
use bace_types::AccountId;
use std::sync::Arc;

#[test]
fn source_heal_spends_stamina_before_rating_and_targets_selected_vital() {
    let mut kernel = Kernel::new(bace_world::World::default(), 64).unwrap();
    kernel
        .configure_combat_random_shared(
            Arc::new(bace_random::RandomRoot::new([0x31; 32], 1).unwrap()),
            7,
        )
        .unwrap();
    let check = bace_combat::specialization::HealingCheck {
        effective_skill: 10_000,
        difficulty: 0,
    };
    let stamina = bace_entity::VitalPool {
        current: 10,
        maximum: 100,
    };
    let (success, _, amount, cost, vitals) = kernel
        .healing_vitals(
            1,
            EntityId(1),
            EntityVital::Mana,
            1.0,
            1.5,
            100,
            bace_entity::VitalPool {
                current: 20,
                maximum: 100,
            },
            stamina,
            check,
        )
        .unwrap();
    assert!(success);
    assert!(cost <= stamina.current);
    assert_eq!(vitals.len(), 2);
    assert_eq!(vitals[0].vital, EntityVital::Stamina);
    assert_eq!(vitals[0].after, stamina.current - cost);
    assert_eq!(vitals[1].vital, EntityVital::Mana);
    assert_eq!(vitals[1].after, 20 + amount.min(80));

    let (success, _, amount, cost, vitals) = kernel
        .healing_vitals(
            2,
            EntityId(1),
            EntityVital::Health,
            1.0,
            1.0,
            100,
            bace_entity::VitalPool {
                current: 20,
                maximum: 100,
            },
            stamina,
            bace_combat::specialization::HealingCheck {
                effective_skill: -10_000,
                difficulty: 0,
            },
        )
        .unwrap();
    assert!(!success);
    assert_eq!((amount, cost), (0, 0));
    assert!(vitals.is_empty());
}

fn fixture() -> Kernel {
    let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
    let combatant = bace_entity::Combatant::new(bace_entity::CombatantProfile {
        maximum_health: 100,
        melee_damage: 0,
        melee_range: 0.0,
        attack_duration: 0.0,
        strike_offsets: vec![],
        player: true,
    })
    .unwrap()
    .with_resources(
        Some(bace_entity::VitalPool {
            current: 100,
            maximum: 100,
        }),
        Some(bace_entity::VitalPool {
            current: 100,
            maximum: 100,
        }),
    )
    .unwrap();
    kernel
        .world
        .register_combatant(EntityId(1), combatant)
        .unwrap();
    kernel
        .world
        .apply_vital_batch(
            &[VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before: 100,
                after: 40,
            }],
            None,
        )
        .unwrap();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let character = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(21),
                advancement: SkillAdvancement::Trained,
                experience_spent: 0,
            },
            details: TraitDetails::Skill {
                initial_level: 100,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        0,
        0,
    )
    .unwrap();
    kernel
        .register_character(
            CharacterBinding {
                actor: EntityId(1),
                account: AccountId(1),
                session: SessionId(1),
            },
            character,
        )
        .unwrap();
    kernel
        .register_magic_registry(
            EntityId(1),
            bace_magic::EnchantmentRegistry::new(128).unwrap(),
            true,
        )
        .unwrap();
    kernel
        .register_magic_damage_profile(EntityId(1), bace_magic::MagicDamageProfile::neutral(true))
        .unwrap();
    let input = SkillValueInputs {
        formula: VitalFormula {
            enabled: true,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [100; 6],
        current_attributes: [100; 6],
        bonuses: SkillBonuses::default(),
        multiplier: 1.0,
        vitae: 1.0,
        additive: 0,
    };
    kernel
        .register_character_skill_inputs(
            EntityId(1),
            crate::PreparedCharacterSkillInputs {
                attack_skill: 21,
                inputs: vec![(21, input)],
                shield: None,
                attribute_modifiers: [crate::PreparedAttributeModifier::default(); 6],
            },
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
            id: EntityId(10),
            revision: 1,
            template: 28,
            stack_key: 28,
            place: ItemPlace::Contained {
                container: EntityId(1),
                slot: 0,
                equipped: 0,
            },
            stack: 1,
            maximum_stack: 1,
            unit_burden: 1,
            unit_value: 100,
            structure: Some(3),
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
        })
        .unwrap();
    kernel
        .configure_combat_random_shared(
            Arc::new(bace_random::RandomRoot::new([0x31; 32], 1).unwrap()),
            7,
        )
        .unwrap();
    kernel
}

fn motion() -> Arc<PreparedMotionChain> {
    let ready = SourceMotionState {
        style: 0x8000_003d,
        substate: 0x4100_0003,
        speed: 1.0,
    };
    let clip = ExecutionClip {
        animation: 0x0300_0001,
        frame_count: 6,
        low: 0,
        high: 5,
        framerate: 30.0,
        frames: vec![RootFrame::default(); 6].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: bace_geometry::Vec3::ZERO,
        omega: bace_geometry::Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone()], 0, 0, physics, ready.substate, 1.0)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone(), clip], 1, 1, physics, 0x1000_010e, 1.0)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap(),
    )
}

fn use_kit_with_boost(boost_value: i32) -> Command {
    Command::HealingKit(C::Use {
        correlation: 9,
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
            sequence: 1,
        },
        item: EntityId(10),
        item_revision: 1,
        target: EntityId(1),
        profile: PreparedHealingKit {
            vital: EntityVital::Health,
            boost_value,
            healkit_mod: 1.0,
            unlimited_use: false,
            max_structure: Some(3),
            template_value: Some(100),
            activation: None,
        },
        motion: motion(),
    })
}

fn use_kit() -> Command {
    use_kit_with_boost(1000)
}

#[test]
fn motion_completion_precedes_charge_and_exact_receipt_adopts_both() {
    let mut kernel = fixture();
    kernel.enqueue(use_kit()).unwrap();
    kernel.step().unwrap();
    assert!(matches!(
        kernel.take_healing_kit_outcome().unwrap().result,
        Ok(D::MotionStarted {
            command: 0x1000_010e,
            ..
        })
    ));
    assert_eq!(
        kernel.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    let mut proposed = None;
    for _ in 0..30 {
        kernel.step().unwrap();
        if let Some(outcome) = kernel.take_healing_kit_outcome() {
            proposed = Some(outcome.result.unwrap());
            break;
        }
    }
    let D::Proposed(ticket) = proposed.expect("motion must finish") else {
        panic!("healing proposal");
    };
    assert_eq!(
        ticket.operation,
        ticket.inventory.as_ref().unwrap().operation
    );
    assert_eq!(
        kernel.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    assert_eq!(
        kernel.characters.get(EntityId(1)).unwrap().revision(),
        ticket.actor_revision
    );
    let wrong = kernel.apply_healing_kit_command(C::Commit {
        correlation: 10,
        operation: ticket.operation,
        receipt: Some(crate::InventoryReceipt {
            operation: ticket.operation,
            revisions: vec![],
        }),
    });
    assert!(matches!(wrong.result, Err(E::Receipt)));
    assert_eq!(
        kernel.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    let receipt = crate::InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .inventory
            .as_ref()
            .unwrap()
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    let committed = kernel.apply_healing_kit_command(C::Commit {
        correlation: 11,
        operation: ticket.operation,
        receipt: Some(receipt),
    });
    assert!(matches!(committed.result, Ok(D::Committed(_))));
    assert_eq!(
        kernel.inventory.item(EntityId(10)).unwrap().structure,
        Some(2)
    );
    assert_eq!(
        kernel.characters.get(EntityId(1)).unwrap().revision(),
        ticket.actor_revision + 1
    );
    let detach_request = crate::PlayerDetachRequest {
        correlation: 12,
        binding: CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
        },
        expected_revision: ticket.actor_revision + 1,
        expected_items: vec![(EntityId(10), 2)],
        capture_final: true,
    };
    assert!(matches!(
        kernel.detach_player(&detach_request),
        Err(crate::CharacterRegistrationError::DurabilityPending)
    ));
    for _ in 0..30 {
        if kernel.healing_kits.returns.is_empty() {
            break;
        }
        kernel.step().unwrap();
    }
    assert!(
        kernel.healing_kits.returns.is_empty(),
        "Ready motion completion must be consumed"
    );
    assert!(
        kernel.world.can_retire_actor_motion(EntityId(1)),
        "Ready hooks must not retain a disconnected player"
    );
    let detached = kernel.detach_player(&detach_request);
    if let Err(error) = detached {
        panic!("committed healer must release every detach hold: {error:?}");
    }
}

#[test]
fn cancelled_motion_never_consumes_healing_kit() {
    let mut kernel = fixture();
    kernel.enqueue(use_kit()).unwrap();
    kernel.step().unwrap();
    assert!(matches!(
        kernel.take_healing_kit_outcome().unwrap().result,
        Ok(D::MotionStarted { .. })
    ));
    let token = kernel
        .healing_kits
        .animations
        .get(&EntityId(1))
        .unwrap()
        .token;
    kernel.world.cancel_motion(EntityId(1), token).unwrap();
    kernel.step().unwrap();
    assert!(matches!(
        kernel.take_healing_kit_outcome().unwrap().result,
        Err(E::MotionDisrupted)
    ));
    assert_eq!(
        kernel.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    assert!(kernel.healing_kits.pending.is_empty());
}

#[test]
fn full_vital_refuses_before_motion_and_failed_skill_still_spends_charge() {
    let mut full = fixture();
    full.world
        .apply_vital_batch(
            &[VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before: 40,
                after: 100,
            }],
            None,
        )
        .unwrap();
    full.enqueue(use_kit()).unwrap();
    full.step().unwrap();
    assert!(matches!(
        full.take_healing_kit_outcome().unwrap().result,
        Err(E::FullVital)
    ));
    assert_eq!(
        full.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    assert!(full.healing_kits.animations.is_empty());

    let mut failed = fixture();
    failed.enqueue(use_kit_with_boost(-10_000)).unwrap();
    failed.step().unwrap();
    assert!(matches!(
        failed.take_healing_kit_outcome().unwrap().result,
        Ok(D::MotionStarted { .. })
    ));
    let mut ticket = None;
    for _ in 0..30 {
        failed.step().unwrap();
        if let Some(outcome) = failed.take_healing_kit_outcome() {
            let D::Proposed(found) = outcome.result.unwrap() else {
                panic!("expected attempt proposal")
            };
            ticket = Some(found);
            break;
        }
    }
    let ticket = ticket.expect("motion completion");
    assert!(!ticket.success);
    assert!(ticket.vitals.is_empty());
    assert_eq!(
        failed.inventory.item(EntityId(10)).unwrap().structure,
        Some(3)
    );
    let receipt = crate::InventoryReceipt {
        operation: ticket.operation,
        revisions: ticket
            .inventory
            .as_ref()
            .unwrap()
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    assert!(matches!(
        failed
            .apply_healing_kit_command(C::Commit {
                correlation: 10,
                operation: ticket.operation,
                receipt: Some(receipt),
            })
            .result,
        Ok(D::Committed(_))
    ));
    assert_eq!(
        failed.inventory.item(EntityId(10)).unwrap().structure,
        Some(2)
    );
    assert_eq!(
        failed
            .world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        40
    );
}
