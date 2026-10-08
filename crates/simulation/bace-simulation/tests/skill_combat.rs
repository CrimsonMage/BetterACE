//! Accepted strike and progression-owner integration. Exact source numerical
//! vectors are checked separately by bace-combat's official .NET oracle.
use bace_character::*;
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_gameplay_api::*;
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::*;
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;
use std::sync::Arc;
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn setup(skill: u32, sac: SkillAdvancement) -> Kernel {
    setup_roles(skill, sac, true)
}
fn setup_roles(skill: u32, sac: SkillAdvancement, attacker_player: bool) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(0.0, if id == 1 { -1.1 } else { 0.0 }, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 1000,
                    melee_damage: 100,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: if id == 1 {
                        attacker_player
                    } else {
                        !attacker_player
                    },
                })
                .unwrap(),
            )
            .unwrap();
    }
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let c = CharacterProgression::with_state(
        &[
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Skill(skill),
                    advancement: sac,
                    experience_spent: 0,
                },
                details: TraitDetails::Skill {
                    initial_level: if sac == SkillAdvancement::Specialized {
                        10
                    } else {
                        0
                    },
                    resistance_at_last_check: 0,
                    last_used_time: 0.0,
                },
            },
            TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Skill(44),
                    advancement: SkillAdvancement::Trained,
                    experience_spent: 0,
                },
                details: TraitDetails::Skill {
                    initial_level: 0,
                    resistance_at_last_check: 0,
                    last_used_time: 0.0,
                },
            },
        ],
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        1000,
        0,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(&[
                SkillCosts {
                    skill,
                    trained_cost: 4,
                    specialized_cost: 6,
                },
                SkillCosts {
                    skill: 44,
                    trained_cost: 6,
                    specialized_cost: 6,
                },
            ])
            .unwrap(),
        ),
        20,
        &[],
    )
    .unwrap();
    let mut k = Kernel::with_gameplay_limits(world, 16, 4, 16).unwrap();
    k.register_character(
        CharacterBinding {
            session: SessionId(1),
            account: AccountId(1),
            actor: EntityId(1),
        },
        c,
    )
    .unwrap();
    k.configure_combat_random_shared(
        Arc::new(bace_random::RandomRoot::new([13; 32], 1).unwrap()),
        7,
    )
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
    k.register_character_skill_inputs(
        EntityId(1),
        PreparedCharacterSkillInputs {
            attack_skill: 44,
            inputs: vec![(44, input), (skill, input)],
            shield: None,
            attribute_modifiers: [PreparedAttributeModifier::default(); 6],
        },
    )
    .unwrap();
    k
}
fn strike(k: &mut Kernel, sequence: u32, power: f32) -> u32 {
    k.enqueue(Command::Combat {
        context: context(sequence),
        request: CombatRequest::ChangeMode(2),
    })
    .unwrap();
    k.enqueue(Command::Combat {
        context: context(sequence + 1),
        request: CombatRequest::TargetedMelee {
            target: EntityId(2),
            height: 2,
            power,
        },
    })
    .unwrap();
    let before = k.world().combatant(EntityId(2)).unwrap().health();
    for _ in 0..5 {
        k.step().unwrap();
        while k.take_combat_outcome().is_some() {}
        while k.take_combat_event().is_some() {}
    }
    before - k.world().combatant(EntityId(2)).unwrap().health()
}
#[test]
fn specialization_refresh_changes_next_accepted_recklessness_and_sneak_strike() {
    for skill in [50, 51] {
        let mut k = setup(skill, SkillAdvancement::Trained);
        assert_eq!(strike(&mut k, 1, 0.5), 110);
        let ticket = k
            .propose_skill(context(3), SkillIntent::Specialize(skill))
            .unwrap();
        assert_eq!(
            k.character_skill_projection(EntityId(1), skill)
                .unwrap()
                .1
                .advancement,
            SkillAdvancement::Trained
        );
        k.confirm_skill_committed(ticket).unwrap();
        let (revision, value) = k.character_skill_projection(EntityId(1), skill).unwrap();
        assert_eq!(revision, 1);
        assert_eq!(value.advancement, SkillAdvancement::Specialized);
        assert_eq!(value.base, 110);
        assert_eq!(strike(&mut k, 4, 0.5), 120);
    }
}
#[test]
fn ordinary_skill_xp_spend_refreshes_live_projection() {
    let mut k = setup(51, SkillAdvancement::Trained);
    k.enqueue(Command::RaiseProgression {
        context: context(1),
        request: RaiseProgression {
            target: ProgressionTarget::Skill(51),
            amount: 10,
        },
    })
    .unwrap();
    k.step().unwrap();
    assert!(k.take_progression_outcome().unwrap().result.is_ok());
    let (revision, value) = k.character_skill_projection(EntityId(1), 51).unwrap();
    assert_eq!(revision, 1);
    assert_eq!(value.base, 101);
}
#[test]
fn endpoint_power_has_no_recklessness_and_rejected_refresh_keeps_previous_projection() {
    let mut k = setup(50, SkillAdvancement::Specialized);
    assert_eq!(strike(&mut k, 1, 0.0), 50);
    let before = k.character_skill_projection(EntityId(1), 50);
    assert!(
        k.register_character_skill_inputs(
            EntityId(1),
            PreparedCharacterSkillInputs {
                attack_skill: 44,
                inputs: vec![],
                shield: None,
                attribute_modifiers: [PreparedAttributeModifier::default(); 6]
            }
        )
        .is_err()
    );
    assert_eq!(k.character_skill_projection(EntityId(1), 50), before);
}

fn defender(
    k: &mut Kernel,
    skill: u32,
    sac: SkillAdvancement,
    base: u32,
    shield: Option<PreparedShield>,
) {
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let states = [skill, 44].map(|id| TraitState {
        progress: TraitProgress {
            target: ProgressionTarget::Skill(id),
            advancement: if id == skill {
                sac
            } else {
                SkillAdvancement::Trained
            },
            experience_spent: 0,
        },
        details: TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time: 0.0,
        },
    });
    let c = CharacterProgression::with_state(
        &states,
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
    k.register_character(
        CharacterBinding {
            session: SessionId(2),
            account: AccountId(2),
            actor: EntityId(2),
        },
        c,
    )
    .unwrap();
    let input = SkillValueInputs {
        formula: VitalFormula {
            enabled: true,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [base; 6],
        current_attributes: [base; 6],
        bonuses: SkillBonuses::default(),
        multiplier: 1.0,
        vitae: 1.0,
        additive: 0,
    };
    k.register_character_skill_inputs(
        EntityId(2),
        PreparedCharacterSkillInputs {
            attack_skill: 44,
            inputs: vec![(44, input), (skill, input)],
            shield,
            attribute_modifiers: [PreparedAttributeModifier::default(); 6],
        },
    )
    .unwrap();
    k.enqueue(Command::Combat {
        context: ActionContext {
            actor: EntityId(2),
            account: AccountId(2),
            session: SessionId(2),
            sequence: 1,
        },
        request: CombatRequest::ChangeMode(2),
    })
    .unwrap();
}
#[test]
fn specialized_defense_applies_to_actual_player_damage() {
    let mut trained = setup_roles(50, SkillAdvancement::Untrained, false);
    defender(&mut trained, 6, SkillAdvancement::Trained, 600, None);
    assert_eq!(strike(&mut trained, 1, 0.5), 100);
    let mut spec = setup_roles(50, SkillAdvancement::Untrained, false);
    defender(&mut spec, 6, SkillAdvancement::Specialized, 600, None);
    assert_eq!(strike(&mut spec, 1, 0.5), 91);
}
#[test]
fn shield_caps_only_protect_accepted_front_geometry() {
    // Actor starts behind: neither trained nor specialized shield protects.
    for (sac, expected) in [
        (SkillAdvancement::Trained, 57),
        (SkillAdvancement::Specialized, 40),
    ] {
        let mut k = setup_roles(50, SkillAdvancement::Untrained, false);
        defender(
            &mut k,
            48,
            sac,
            100,
            Some(PreparedShield {
                effective_armor: 1000.0,
                magic_absorption: 0.5,
            }),
        );
        assert_eq!(strike(&mut k, 1, 0.5), 100);
        k.enqueue(Command::ServerTeleport {
            cell: CellId(1),
            actor: EntityId(1),
            position: Vec3::new(0.0, 1.1, 0.5),
        })
        .unwrap();
        k.step().unwrap();
        assert_eq!(strike(&mut k, 3, 0.5), expected);
    }
}

#[test]
fn healer_and_missile_consumer_views_read_accepted_class_and_values() {
    let trained = setup(21, SkillAdvancement::Trained);
    let specialized = setup(21, SkillAdvancement::Specialized);
    assert_eq!(
        trained
            .project_healing_check(EntityId(1), 20, bace_entity::EntityVital::Health)
            .unwrap()
            .effective_skill,
        132
    );
    assert_eq!(
        specialized
            .project_healing_check(EntityId(1), 20, bace_entity::EntityVital::Health)
            .unwrap()
            .effective_skill,
        195
    );
    let trained = setup(7, SkillAdvancement::Trained);
    let specialized = setup(7, SkillAdvancement::Specialized);
    assert_eq!(
        trained
            .project_specialized_defense_rating(
                EntityId(1),
                bace_combat::specialization::DefenseKind::Missile
            )
            .unwrap(),
        0
    );
    assert_eq!(
        specialized
            .project_specialized_defense_rating(
                EntityId(1),
                bace_combat::specialization::DefenseKind::Missile
            )
            .unwrap(),
        2
    );
}
