use bace_crafting::{ChanceInput, CraftError, roll_tinker, tinker_chance};
use bace_random::RandomRoot;

fn input() -> ChanceInput {
    ChanceInput {
        skill: 1000,
        trained: true,
        lum_craft: 0,
        tool_workmanship: 10.0,
        target_workmanship: 10.0,
        material: 0x3d,
        times_tinkered: 0,
        imbue: true,
        imbue_augmentation: false,
        foolproof: false,
    }
}

#[test]
fn compiled_official_ace_formula_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/chance.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<f64> = line.split(',').map(|x| x.parse().unwrap()).collect();
        let actual = tinker_chance(ChanceInput {
            material: v[0] as u32,
            skill: v[1] as u32,
            tool_workmanship: v[2] as f32,
            target_workmanship: v[3] as f32,
            times_tinkered: v[4] as u32,
            lum_craft: v[5] as u32,
            imbue: false,
            ..input()
        })
        .unwrap();
        assert_eq!(actual.difficulty, v[6] as i32, "{line}");
        assert!(
            (actual.probability - v[7]).abs() < 1e-14,
            "{line} actual {}",
            actual.probability
        );
        count += 1;
    }
    assert_eq!(count, 770);
}

#[test]
fn exact_caps_retain_skill_dependency_and_foolproof_override() {
    assert_eq!(tinker_chance(input()).unwrap().probability, 0.33);
    assert_eq!(
        tinker_chance(ChanceInput {
            imbue_augmentation: true,
            ..input()
        })
        .unwrap()
        .probability,
        0.38
    );
    let difficulty = tinker_chance(input()).unwrap().difficulty as u32;
    assert_eq!(
        tinker_chance(ChanceInput {
            skill: difficulty,
            ..input()
        })
        .unwrap()
        .probability,
        0.5 / 3.0
    );
    assert_eq!(
        tinker_chance(ChanceInput {
            skill: difficulty,
            imbue_augmentation: true,
            ..input()
        })
        .unwrap()
        .probability,
        0.5 / 3.0 + 0.05
    );
    for skill in [0, 1, 1000] {
        assert_eq!(
            tinker_chance(ChanceInput {
                skill,
                foolproof: true,
                ..input()
            })
            .unwrap()
            .probability,
            1.0
        );
    }
    assert_eq!(
        tinker_chance(ChanceInput {
            trained: false,
            ..input()
        }),
        Err(CraftError::Untrained)
    );
    assert_eq!(
        tinker_chance(ChanceInput {
            times_tinkered: 10,
            ..input()
        }),
        Err(CraftError::TinkerLimit)
    );
    assert_eq!(
        tinker_chance(ChanceInput {
            tool_workmanship: f32::NAN,
            ..input()
        }),
        Err(CraftError::InvalidState)
    );
    assert_eq!(
        tinker_chance(ChanceInput {
            skill: u32::MAX,
            ..input()
        }),
        Err(CraftError::Overflow)
    );
}

#[test]
fn thirty_two_characters_each_pass_four_simultaneous_statistical_scenarios() {
    // Predeclared, two-sided Hoeffding bound with union bound across 128 cases.
    // Family-wise alpha <= 1e-6; no seed selection, aggregate-only pass or rerun.
    const N: u32 = 10_000;
    let epsilon = ((2.0_f64 * 128.0 / 1e-6).ln() / (2.0 * f64::from(N))).sqrt();
    let root = RandomRoot::new([0x6b; 32], 7).unwrap();
    let low = tinker_chance(input()).unwrap().difficulty as u32;
    for character in 1_u128..=32 {
        for scenario in 0_u32..4 {
            let setup = ChanceInput {
                skill: if scenario < 2 { low } else { 1000 },
                imbue_augmentation: scenario % 2 == 1,
                ..input()
            };
            let expected = tinker_chance(setup).unwrap().probability;
            let mut successes = 0;
            for ordinal in 1..=N {
                let op = ((u128::from(scenario) << 64) | u128::from(ordinal)).to_le_bytes();
                successes += u32::from(
                    roll_tinker(setup, &root, character.to_le_bytes(), op)
                        .unwrap()
                        .1,
                );
            }
            let frequency = f64::from(successes) / f64::from(N);
            assert!(
                (frequency - expected).abs() <= epsilon,
                "character {character} scenario {scenario}: {frequency} vs {expected}, bound {epsilon}"
            );
        }
    }
}
