use bace_content::{Property, SparseProperties, WeenieV1};
use bace_loot::{MutationScript, MutationScripts, TreasureError, TreasureRandom};
mod table_support;
#[derive(Clone)]
struct Tape {
    roll: f64,
    draws: usize,
    fail_after: usize,
}
impl TreasureRandom for Tape {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        if self.draws == self.fail_after {
            return Err(TreasureError::Bounds);
        }
        self.draws += 1;
        Ok(self.roll)
    }
    fn inclusive(&mut self, _: i32, _: i32) -> Result<i32, TreasureError> {
        Err(TreasureError::Bounds)
    }
}
fn item() -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "fixture".into(),
        weenie_type: 6,
        last_modified: None,
        properties: SparseProperties {
            ints: vec![
                Property { id: 44, value: 100 },
                Property { id: 48, value: 45 },
            ],
            floats: vec![
                Property { id: 22, value: 0.2 },
                Property { id: 29, value: 1. },
                Property { id: 62, value: 1. },
            ],
            ..Default::default()
        },
    }
}
#[test]
fn all_embedded_scripts_match_independently_compiled_ace_parser_and_evaluator() {
    table_support::install_tables();
    let scripts = MutationScripts::pinned().unwrap();
    assert_eq!(scripts.len(), 51);
    let mut rows = 0;
    for line in include_str!("fixtures/ace_mutations.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let mut item = item();
        let mut rng = Tape {
            roll: f[2].parse().unwrap(),
            draws: 0,
            fail_after: usize::MAX,
        };
        let changed = scripts
            .apply(f[0], &mut item, f[1].parse().unwrap(), &mut rng)
            .unwrap();
        assert_eq!(
            changed,
            f[3] == "True",
            "{} tier{} roll{}",
            f[0],
            f[1],
            f[2]
        );
        assert_eq!(
            rng.draws,
            f[4].parse::<usize>().unwrap(),
            "{} tier{}",
            f[0],
            f[1]
        );
        let ints = f[5]
            .split(',')
            .map(|v| {
                let (id, value) = v.split_once('=').unwrap();
                Property {
                    id: id.parse().unwrap(),
                    value: value.parse::<i32>().unwrap(),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            item.properties.ints, ints,
            "{} tier{} roll{}",
            f[0], f[1], f[2]
        );
        let floats = f[6]
            .split(',')
            .map(|v| {
                let (id, value) = v.split_once('=').unwrap();
                Property {
                    id: id.parse::<u32>().unwrap(),
                    value: value.parse::<f64>().unwrap(),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(item.properties.floats.len(), floats.len(), "{}", f[0]);
        for (actual, expected) in item.properties.floats.iter().zip(&floats) {
            assert_eq!(actual.id, expected.id, "{}", f[0]);
            assert!(
                (actual.value - expected.value).abs() < 1e-12,
                "{} tier{} roll{} property{} actual{} expected{}",
                f[0],
                f[1],
                f[2],
                actual.id,
                actual.value,
                expected.value
            );
        }
        rows += 1;
    }
    assert_eq!(rows, 1836);
}
#[test]
fn rejected_randomness_preserves_item_and_caller_cursor() {
    table_support::install_tables();
    let scripts = MutationScripts::pinned().unwrap();
    let mut item = item();
    let before = item.clone();
    let mut rng = Tape {
        roll: 0.5,
        draws: 0,
        fail_after: 1,
    };
    assert!(
        scripts
            .apply("Casters/caster.txt", &mut item, 1, &mut rng)
            .is_err()
    );
    assert_eq!(item, before);
    assert_eq!(rng.draws, 0);
}
#[test]
fn missing_property_argument_skips_effect_and_missing_result_adds_from_zero() {
    let script = MutationScript::parse(
        "Tier chances: 1\n- Chance: 100%:\nWieldSkillType = WeaponSkill\nDamage += 7\n",
    )
    .unwrap();
    let mut item = item();
    item.properties.ints.clear();
    let mut rng = Tape {
        roll: 0.,
        draws: 0,
        fail_after: 99,
    };
    assert!(script.apply(&mut item, 1, &mut rng).unwrap());
    assert_eq!(item.properties.ints, vec![Property { id: 44, value: 7 }]);
    assert_eq!(rng.draws, 2);
}
#[test]
fn source_assign_divide_multiplies_and_divide_zero_retains_value() {
    let script = MutationScript::parse(
        "Tier chances: 1\n- Chance: 100%:\nDamage = 9 / 3\nWeaponOffense /= 0\n",
    )
    .unwrap();
    let mut item = item();
    let mut rng = Tape {
        roll: 0.,
        draws: 0,
        fail_after: 99,
    };
    script.apply(&mut item, 1, &mut rng).unwrap();
    assert_eq!(item.properties.ints[0].value, 27);
    assert_eq!(item.properties.floats[2].value, 1.);
}
#[test]
fn malformed_or_unsupported_script_is_rejected_before_execution() {
    for text in [
        "",
        "Tier chances: NaN\n- Chance: 100%:\nDamage = 1",
        "Tier chances: 1\n- Chance: 100%:\nInventedQuality = 1",
        "Tier chances: 1\n- Chance: 100%:\nDamage = Variable[0]",
    ] {
        assert!(MutationScript::parse(text).is_err());
    }
    assert!(MutationScript::parse(&"x".repeat(65537)).is_err());
}
#[test]
fn invalid_tier_still_consumes_filter_roll_without_mutation() {
    let script = MutationScript::parse("Tier chances: 1\n- Chance: 100%:\nDamage = 9").unwrap();
    let mut item = item();
    let before = item.clone();
    let mut rng = Tape {
        roll: 0.,
        draws: 0,
        fail_after: 99,
    };
    assert!(!script.apply(&mut item, 9, &mut rng).unwrap());
    assert_eq!(item, before);
    assert_eq!(rng.draws, 1);
}
