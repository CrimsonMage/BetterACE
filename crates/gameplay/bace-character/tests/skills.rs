use bace_character::*;
use bace_gameplay_api::*;
use std::sync::Arc;
fn tables() -> Arc<ProgressionTables> {
    Arc::new(ProgressionTables {
        attributes: RankTable::new(&[0, 10, 100]).unwrap(),
        vitals: RankTable::new(&[0, 10, 100]).unwrap(),
        trained_skills: RankTable::new(&[0, 10, 50, 100]).unwrap(),
        specialized_skills: RankTable::new(&[0, 5, 20, 60]).unwrap(),
    })
}
fn character(skill: u32, sac: SkillAdvancement, pp: u32, aug: bool) -> CharacterProgression {
    CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(skill),
                advancement: sac,
                experience_spent: pp,
            },
            details: TraitDetails::Skill {
                initial_level: if sac == SkillAdvancement::Specialized {
                    10
                } else {
                    0
                },
                resistance_at_last_check: 27,
                last_used_time: 14.5,
            },
        }],
        tables(),
        100,
        4,
    )
    .unwrap()
    .with_training(
        Arc::new(
            SkillTrainingRules::new(&[SkillCosts {
                skill,
                trained_cost: 4,
                specialized_cost: 6,
            }])
            .unwrap(),
        ),
        20,
        if aug && is_augmentation_skill(skill) {
            std::slice::from_ref(&skill)
        } else {
            &[]
        },
    )
    .unwrap()
}
fn inputs() -> SkillValueInputs {
    SkillValueInputs {
        formula: VitalFormula {
            enabled: false,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [0; 6],
        current_attributes: [0; 6],
        bonuses: SkillBonuses {
            all_skills: 3,
            skilled_melee: 1,
            skilled_missile: 2,
            skilled_magic: 3,
            enlightenment: 2,
            jack_of_all_trades: 1,
            specialized_luminance: 4,
        },
        multiplier: 1.0,
        vitae: 1.0,
        additive: 0,
    }
}
#[test]
fn independent_official_lowering_bonus_and_specialization_overage_vectors() {
    let mut rows = 0;
    for line in include_str!("fixtures/skills.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (kind, tail) = line.split_once(',').unwrap();
        let n: Vec<u64> = tail.split(',').map(|x| x.parse().unwrap()).collect();
        match kind {
            "lower" => {
                let mut c = character(
                    n[0] as u32,
                    SkillAdvancement::try_from(n[1] as u32).unwrap(),
                    50,
                    n[2] != 0,
                );
                let result = if n[3] == 1 {
                    c.reset_skill(n[0] as u32)
                } else {
                    c.lower_skill(n[0] as u32, &[])
                };
                assert_eq!(result.is_ok(), n[4] != 0, "{line}");
                let change = result.unwrap();
                assert_eq!(change.after.advancement as u64, n[5], "{line}");
                assert_eq!(u64::from(change.after.experience_spent), n[6]);
                assert_eq!(u64::from(change.after.ranks), n[7]);
                assert_eq!(
                    change.after.details,
                    Some(TraitDetails::Skill {
                        initial_level: n[8] as u32,
                        resistance_at_last_check: 27,
                        last_used_time: 14.5
                    }),
                    "{line}"
                );
                assert_eq!(change.available_experience, n[9]);
                assert_eq!(u64::from(change.available_skill_credits), n[10]);
            }
            "bonus" => {
                let c = project_skill_values(
                    n[0] as u32,
                    SkillAdvancement::try_from(n[1] as u32).unwrap(),
                    0,
                    0,
                    inputs(),
                )
                .unwrap();
                assert_eq!(u64::from(c.base), n[2], "{line}");
                assert_eq!(u64::from(c.current), n[2] + n[3], "{line}");
            }
            "overage" => {
                let mut c = character(6, SkillAdvancement::Trained, n[0] as u32, false);
                let change = c.specialize_skill(6).unwrap();
                assert_eq!(u64::from(change.after.experience_spent), n[1]);
                assert_eq!(u64::from(change.after.ranks), n[2]);
                assert_eq!(
                    change.after.details,
                    Some(TraitDetails::Skill {
                        initial_level: n[3] as u32,
                        resistance_at_last_check: 27,
                        last_used_time: 14.5
                    })
                );
            }
            _ => panic!("unknown {kind}"),
        }
        rows += 1;
    }
    assert_eq!(rows, 117);
}
#[test]
fn all_five_augmentation_transitions_and_retraining_preserve_flags() {
    for skill in AUGMENTATION_SKILLS {
        let mut c = character(skill, SkillAdvancement::Trained, 100, false);
        let proposal = c.propose_augment_skill(skill, 20).unwrap();
        assert_eq!(c.available_experience(), 100);
        assert_eq!(proposal.change().after.experience_spent, 100);
        c.adopt_skill_proposal(proposal).unwrap();
        assert_eq!(c.available_experience(), 80);
        assert!(c.augmented_skills().any(|id| id == skill));
        assert_eq!(
            c.augment_skill(skill, 20),
            Err(SkillTransitionError::AlreadyAugmented)
        );
        c.reset_skill(skill).unwrap();
        if !is_locked_skill(skill) {
            let update = c
                .train_skill(TrainSkill {
                    skill,
                    quoted_credits: 4,
                })
                .unwrap();
            assert_eq!(update.after.advancement, SkillAdvancement::Specialized);
            assert_eq!(update.after.experience_spent, 0);
        } else {
            assert_eq!(
                c.projection(ProgressionTarget::Skill(skill))
                    .unwrap()
                    .advancement,
                SkillAdvancement::Specialized
            );
        }
    }
}
#[test]
fn proposals_are_atomic_fenced_and_reservable_until_durable_acknowledgment() {
    let mut c = character(6, SkillAdvancement::Trained, 50, false);
    let proposal = c.propose_lower_skill(6, &[]).unwrap();
    assert_eq!(c.available_experience(), 100);
    assert_eq!(proposal.proposed_state().available_experience(), 150);
    c.touch_revision().unwrap();
    assert!(c.adopt_skill_proposal(proposal).is_err());
    assert_eq!(c.available_experience(), 100);
    let p = c.propose_lower_skill(6, &[]).unwrap();
    let after = p.change();
    assert_eq!(c.adopt_skill_proposal(p).unwrap(), after);
    assert_eq!(c.available_experience(), 150);
    assert_eq!(c.available_skill_credits(), Some(24));
}
#[test]
fn lowering_and_refund_failures_retain_every_field() {
    let mut c = character(6, SkillAdvancement::Specialized, 60, false);
    let before = c.projection(ProgressionTarget::Skill(6));
    for requirement in [
        SkillWieldRequirement::RawSkill(6),
        SkillWieldRequirement::CurrentSkill(6),
        SkillWieldRequirement::Training {
            skill: 6,
            advancement: SkillAdvancement::Specialized,
        },
    ] {
        assert_eq!(
            c.lower_skill(6, &[requirement]),
            Err(SkillTransitionError::WieldRequirement)
        );
        assert_eq!(c.projection(ProgressionTarget::Skill(6)), before);
        assert_eq!(c.revision(), 4);
    }
    assert!(!lowering_blocked(
        6,
        SkillAdvancement::Specialized,
        &[SkillWieldRequirement::Training {
            skill: 6,
            advancement: SkillAdvancement::Trained
        }]
    ));
    let states: Vec<_> = c
        .trait_states()
        .map(|(progress, details)| TraitState {
            progress,
            details: details.unwrap(),
        })
        .collect();
    let mut c = CharacterProgression::with_state(&states, tables(), i64::MAX as u64, 4)
        .unwrap()
        .with_training(
            Arc::new(
                SkillTrainingRules::new(&[SkillCosts {
                    skill: 6,
                    trained_cost: 4,
                    specialized_cost: 6,
                }])
                .unwrap(),
            ),
            20,
            &[],
        )
        .unwrap();
    assert_eq!(
        c.lower_skill(6, &[]),
        Err(SkillTransitionError::ExperienceOverflow)
    );
    assert_eq!(c.projection(ProgressionTarget::Skill(6)), before);
    assert_eq!(c.revision(), 4);
}
#[test]
fn cap_uses_heritage_total_not_upgrade_price_and_excludes_augmentations() {
    for (cost, accepted) in [(69, true), (70, true), (71, false)] {
        let mut c = CharacterProgression::with_state(
            &[TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Skill(6),
                    experience_spent: 50,
                    advancement: SkillAdvancement::Trained,
                },
                details: TraitDetails::Skill {
                    initial_level: 0,
                    resistance_at_last_check: 0,
                    last_used_time: 0.0,
                },
            }],
            tables(),
            100,
            4,
        )
        .unwrap()
        .with_training(
            Arc::new(
                SkillTrainingRules::new(&[SkillCosts {
                    skill: 6,
                    trained_cost: 4,
                    specialized_cost: 6,
                }])
                .unwrap()
                .with_specialization_costs(&[(6, cost)])
                .unwrap(),
            ),
            20,
            &[],
        )
        .unwrap();
        assert_eq!(c.specialize_skill(6).is_ok(), accepted);
        assert_eq!(
            c.available_skill_credits(),
            Some(if accepted { 14 } else { 20 })
        );
    }
    assert_eq!(
        character(18, SkillAdvancement::Specialized, 0, true).specialized_credit_total(),
        Ok(0)
    );
}
#[test]
fn overage_survives_load_then_refunds_without_allowing_more_spending() {
    let mut c = character(6, SkillAdvancement::Specialized, 100, false);
    assert_eq!(c.projection(ProgressionTarget::Skill(6)).unwrap().ranks, 3);
    assert_eq!(
        c.raise(RaiseProgression {
            target: ProgressionTarget::Skill(6),
            amount: 0
        }),
        Err(ProgressionRejection::MaximumRank)
    );
    assert_eq!(c.lower_skill(6, &[]).unwrap().available_experience, 200);
}
#[test]
fn authoritative_value_order_keeps_post_vitae_bonuses_unscaled() {
    let mut input = inputs();
    input.formula = VitalFormula {
        enabled: true,
        divisor: 2,
        attribute1: 1,
        attribute2: 2,
    };
    input.base_attributes = [20; 6];
    input.current_attributes = [30; 6];
    input.multiplier = 1.5;
    input.vitae = 0.5;
    input.additive = -2;
    let result = project_skill_values(44, SkillAdvancement::Specialized, 3, 10, input).unwrap();
    assert_eq!(result.base, 48);
    assert_eq!(result.current, 55); // (30+13+3+10+2)*1.5*.5+5+8-2 = 54.5 -> 55
    input.additive = -1000;
    assert_eq!(
        project_skill_values(44, SkillAdvancement::Specialized, 3, 10, input)
            .unwrap()
            .current,
        0
    );
    input.vitae = f32::NAN;
    assert_eq!(
        project_skill_values(44, SkillAdvancement::Specialized, 3, 10, input),
        Err(SkillValueError::InvalidModifier)
    );
}
