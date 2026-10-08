use bace_combat::specialization::*;
use bace_gameplay_api::SkillAdvancement;
fn skill(sac: u32, value: u32) -> CombatSkill {
    CombatSkill {
        advancement: SkillAdvancement::try_from(sac).unwrap(),
        base: value,
        current: value,
    }
}
#[test]
fn exact_scalar_bits_match_verbatim_official_methods() {
    let mut count = 0;
    for line in include_str!("fixtures/specialization.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = line.split(',').collect();
        let sac: u32 = f[1].parse().unwrap();
        let value: u32 = f[2].parse().unwrap();
        let s = skill(sac, value);
        match f[0] {
            "armor" => assert_eq!(
                shield_physical_modifier(s, 0.0, true, f[3].parse().unwrap())
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "rating" => assert_eq!(
                defense_rating_modifier(f[3].parse().unwrap())
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "dirty" => {
                for (class, start) in [(3, 3), (2, 7)] {
                    let s = skill(class, 300);
                    assert_eq!(
                        dirty_fighting_effects(s, 300, DirtyAttackHeight::Low, 0.0)
                            .unwrap()
                            .spells[0],
                        f[start].parse::<u32>().unwrap()
                    );
                    assert_eq!(
                        dirty_fighting_effects(s, 300, DirtyAttackHeight::Medium, 0.0)
                            .unwrap()
                            .spells[0],
                        f[start + 1].parse::<u32>().unwrap()
                    );
                    assert_eq!(
                        dirty_fighting_effects(s, 300, DirtyAttackHeight::High, 0.0)
                            .unwrap()
                            .spells,
                        [
                            f[start + 2].parse::<u32>().unwrap(),
                            f[start + 3].parse::<u32>().unwrap()
                        ]
                    );
                }
            }
            "shield" => assert_eq!(
                shield_magic_modifier(s, f[3].parse().unwrap(), 0.6)
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "defense" => assert_eq!(
                specialized_defense_rating(
                    true,
                    match f[3] {
                        "0" => DefenseKind::Melee,
                        "1" => DefenseKind::Missile,
                        "2" => DefenseKind::Magic,
                        _ => panic!(),
                    },
                    s
                ),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "reck" => assert_eq!(
                recklessness_modifier(s, 300, f[3].parse().unwrap(), true, false)
                    .unwrap()
                    .to_bits(),
                f[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "sneak" => assert_eq!(
                sneak_attack_modifier(SneakAttackInput {
                    sneak: s,
                    deception: s,
                    attack_skill: 300,
                    target_assess_person: 153,
                    target_is_creature: true,
                    angle_degrees: f[3].parse().unwrap(),
                    roll: f[4].parse().unwrap()
                })
                .unwrap()
                .to_bits(),
                f[5].parse::<u32>().unwrap(),
                "{line}"
            ),
            "heal" => {
                let result = healing_check(s, f[3].parse().unwrap(), 17, f[4] == "1").unwrap();
                assert_eq!(
                    result.effective_skill,
                    f[5].parse::<i32>().unwrap(),
                    "{line}"
                );
                assert_eq!(result.difficulty, f[6].parse::<i32>().unwrap(), "{line}");
            }
            _ => panic!("unknown {line}"),
        }
        count += 1;
    }
    assert_eq!(count, 302);
}
#[test]
fn rejected_nonfinite_values_never_become_damage_modifiers() {
    let s = skill(3, 300);
    assert_eq!(
        shield_magic_modifier(s, f32::NAN, 0.6),
        Err(SpecializationError::Nonfinite)
    );
    assert_eq!(
        shield_magic_modifier(s, 0.0, 2.0),
        Err(SpecializationError::InvalidRange)
    );
    assert_eq!(
        recklessness_modifier(s, 300, f32::INFINITY, true, false),
        Err(SpecializationError::Nonfinite)
    );
    assert_eq!(recklessness_modifier(s, 300, 0.5, true, true), Ok(1.0));
    assert_eq!(
        dirty_fighting_effects(s, 300, DirtyAttackHeight::Low, 1.0),
        Err(SpecializationError::InvalidRange)
    );
    assert_eq!(
        healing_check(s, i32::MAX, u32::MAX, true),
        Err(SpecializationError::Overflow)
    );
    assert_eq!(specialized_defense_rating(false, DefenseKind::Magic, s), 0);
}
#[test]
fn shield_caps_and_dirty_fighting_select_exact_specialized_effects() {
    assert_eq!(shield_armor_cap(skill(2, 101)), 50);
    assert_eq!(shield_armor_cap(skill(2, 103)), 52);
    assert_eq!(shield_armor_cap(skill(3, 101)), 101);
    for (class, low, medium, high) in [(2, 5944, 5943, [5942, 5945]), (3, 5940, 5939, [5938, 5941])]
    {
        let s = skill(class, 300);
        assert_eq!(
            dirty_fighting_effects(s, 300, DirtyAttackHeight::Low, 0.0)
                .unwrap()
                .spells,
            [low, 0]
        );
        assert_eq!(
            dirty_fighting_effects(s, 300, DirtyAttackHeight::Medium, 0.0)
                .unwrap()
                .spells,
            [medium, 0]
        );
        let result = dirty_fighting_effects(s, 300, DirtyAttackHeight::High, 0.0).unwrap();
        assert_eq!(result.spells, high);
        assert_eq!(result.count, 2);
        assert_eq!(
            dirty_fighting_effects(s, 300, DirtyAttackHeight::High, 0.25)
                .unwrap()
                .count,
            0
        );
    }
    assert_eq!(
        dirty_fighting_effects(skill(3, 150), 300, DirtyAttackHeight::High, 0.125)
            .unwrap()
            .count,
        0
    );
}
