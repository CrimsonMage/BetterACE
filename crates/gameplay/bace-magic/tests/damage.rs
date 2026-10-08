use bace_gameplay_api::weapon_combat::PhysicalSkill;
use bace_magic::{
    MagicDamageInput, MagicDamageProfile, MagicDamageRolls, MagicSchool, MagicWand,
    magic_damage_after_mitigation, magic_damage_before_mitigation,
};
#[test]
fn complete_original_gdle_projectile_damage_and_mitigation_vectors() {
    for row in include_str!("fixtures/damage.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        if let Some(rest) = row.strip_prefix("periodic,") {
            let p: Vec<f64> = rest.split(',').map(|v| v.parse().unwrap()).collect();
            assert_eq!(
                bace_magic::gdle_periodic_damage(
                    p[2] as u32,
                    p[0] != 0.,
                    p[3] as u32,
                    50,
                    p[1] != 0.,
                    p[4]
                )
                .unwrap(),
                p[5] as u32,
                "{row}"
            );
            continue;
        }
        if let Some(rest) = row.strip_prefix("formula,") {
            let p: Vec<u32> = rest.split(',').map(|s| s.parse().unwrap()).collect();
            assert_eq!(bace_magic::spell_formula_level(&[p[0]]), p[1]);
            continue;
        }
        let direct = row.starts_with("direct,");
        let values = row.strip_prefix("direct,").unwrap_or(row);
        let p: Vec<f64> = values.split(',').map(|s| s.parse().unwrap()).collect();
        let mut a = MagicDamageProfile::neutral(p[0] != 0.);
        let mut d = MagicDamageProfile::neutral(p[1] != 0.);
        a.ratings.damage = 20;
        a.ratings.critical_damage = 5;
        d.ratings.resistance = 10;
        a.sneak = PhysicalSkill {
            advancement: 3,
            current: 200,
        };
        a.deception = PhysicalSkill {
            advancement: 3,
            current: 300,
        };
        d.assess_person = PhysicalSkill {
            advancement: 2,
            current: 150,
        };
        d.creature_type = 9;
        match p[6] as u32 {
            1 => {
                d.base_strength = 300;
                d.base_endurance = 200;
                d.resistances[4].quality.decreasing = 0.8;
                d.resistances[4].quality.increasing = 1.5;
            }
            2 => {
                d.resistances[4].quality.raw = 0.;
                d.critical_defense = true;
            }
            3 => {
                d.shield = Some((
                    PhysicalSkill {
                        advancement: 3,
                        current: 300,
                    },
                    0.25,
                ));
                d.magic_defense = PhysicalSkill {
                    advancement: 3,
                    current: 600,
                };
                d.augmentation_family = true;
                d.resistances[4].augmentation = 2;
            }
            4 => {
                d.missile_absorption = true;
                d.magic_defense = PhysicalSkill {
                    advancement: 2,
                    current: 400,
                };
                d.resistances[4].quality.decreasing = 0.6;
            }
            _ => {}
        }
        if p[5] != 0. {
            a.wand = Some(MagicWand {
                entity: 10,
                revision: 0,
                damage_type: 16,
                elemental_modifier: 1.2,
                elemental_present: true,
                inherit_wielder: true,
                imbues: if p[5] == 2. { 3 | 128 } else { 0 },
                biting: 0.3,
                double_enchant_biting: true,
                crushing: 0.2,
                double_enchant_crushing: true,
                slayer_type: 9,
                slayer_bonus: 2.,
                resistance_cleaving: None,
                ignore_magic_resistance: false,
            });
        }
        if direct {
            d.resistances[8] = d.resistances[4];
        }
        let input = MagicDamageInput {
            source: &a,
            target: &d,
            school: if p[4] != 0. || direct {
                MagicSchool::Life
            } else {
                MagicSchool::War
            },
            skill: p[2] as u32,
            formula_level: 4,
            damage_type: if direct { 128 } else { 16 },
            minimum: 20,
            maximum: 60,
            life_damage: (p[4] != 0.).then_some(f64::from(30.0_f32 * 1.2)),
            projectile: !direct,
            target_in_combat: true,
            target_angle_degrees: 180.,
            rolls: MagicDamageRolls {
                variance: 0.25,
                critical: if p[3] != 0. { 0. } else { 0.9 },
                critical_defense: 0.01,
                sneak: 0.01,
            },
        };
        let result = magic_damage_before_mitigation(&input).unwrap();
        assert_eq!(result.rating, p[8] as i32, "rating {row}");
        assert_eq!(
            (result.critical, result.critical_defended, result.sneak),
            (p[10] != 0., p[11] != 0., p[12] != 0.),
            "conditions {row}"
        );
        assert!(
            (result.before_rating * f64::from(p[9] as f32) - p[7]).abs() < 0.0001,
            "before {row}: {}",
            result.before_rating
        );
        // Recorded original negative rating is intentionally passed here; the
        // simulation composes the independently qualified corrected helper.
        let damage = magic_damage_after_mitigation(&input, result, p[9] as f32).unwrap();
        assert_eq!(damage, p[13] as u32, "damage {row}");
    }
}
