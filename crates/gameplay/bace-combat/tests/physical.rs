use bace_combat::physical::*;
use bace_gameplay_api::weapon_combat::*;
fn prepared(player: bool) -> PhysicalCombatProfile {
    let weapon = PhysicalWeapon {
        entity: if player { 100 } else { 200 },
        revision: 1,
        style: 2,
        attack_type: 6,
        damage_type: 3,
        damage: 20.0,
        variance: 0.0,
        skill: 44,
        attack_time: 0,
        encumbrance: 0,
        offense: 1.0,
        imbues: 0,
        biting: 0.0,
        crushing: 0.0,
        slayer_type: 0,
        slayer_bonus: 0.0,
        cleave_targets: 1,
        ignore_magic_armor: false,
        ignore_magic_resistance: false,
        armor_cleaving: false,
        resistance_cleaving: None,
        proc_spell: None,
        proc_chance: 0.0,
    };
    let armor = PhysicalBodyDefense {
        part: 0,
        hit_weights: [1.0; 12],
        armor: [quality(0.0); 8],
    };
    PhysicalCombatProfile {
        equipment: vec![PhysicalEquipmentStamp {
            entity: if player { 100 } else { 200 },
            revision: 1,
            location: 0x100000,
        }],
        revision: 1,
        content_hash: [7; 32],
        player,
        creature_type: 1,
        pk: PkStatus::Npk,
        attackable: true,
        immune: false,
        lifestone_protected: false,
        style: 0x80000040,
        main: Some(weapon),
        offhand: None,
        launcher: None,
        ammunition: None,
        gloves: None,
        boots: None,
        body_attacks: vec![],
        maneuvers: vec![PhysicalManeuver {
            style: 0x80000040,
            attack_type: 4,
            height: 2,
            minimum_skill: 0,
            motion: 0x10000063,
            duration: 0.2,
            hooks: vec![
                PhysicalAttackHook {
                    seconds: 0.05,
                    part: 0,
                },
                PhysicalAttackHook {
                    seconds: 0.1,
                    part: 0,
                },
            ],
        }],
        skills: vec![
            (
                6,
                PhysicalSkill {
                    advancement: 2,
                    current: 100,
                },
            ),
            (
                44,
                PhysicalSkill {
                    advancement: 3,
                    current: 300,
                },
            ),
        ],
        strength: 100,
        coordination: 100,
        quickness: 100,
        base_strength: 0,
        base_endurance: 0,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
        armor: vec![armor],
        resistances: [PhysicalResistance {
            quality: quality(1.0),
            augmentation: 0,
        }; 8],
        shield_encumbrance: 0,
        shield_placement: true,
        armor_layers: vec![],
        ignore_shield: 0.0,
        shield: None,
        shield_skill: PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        ratings: PhysicalRatings::default(),
        critical_defense: false,
        range: 3.0,
        height: 1.0,
        missile: None,
    }
}
#[test]
fn unchanged_gdle_setup_and_scalar_methods_match_independent_vectors() {
    for line in include_str!("fixtures/physical.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let c: Vec<_> = line.split(',').collect();
        match c[0] {
            "maneuver" => {
                let style = c[1].parse().unwrap();
                let attack = c[2].parse().unwrap();
                let height = c[3].parse().unwrap();
                let power = c[4].parse().unwrap();
                let available = c[5] == "1";
                let mut p = prepared(true);
                p.style = style;
                p.main.as_mut().unwrap().attack_type = attack;
                p.main.as_mut().unwrap().style = if attack == 1 { 1 } else { 2 };
                p.offhand = p.main.clone();
                p.skills.push((
                    49,
                    PhysicalSkill {
                        advancement: 3,
                        current: 300,
                    },
                ));
                p.maneuvers.clear();
                let types = [
                    1, 2, 4, 6, 8, 0x10, 0x19, 0x20, 0x40, 0x80, 0x100, 0xa0, 0x140, 0x200, 0x400,
                    0x800, 0x1000, 0x2000, 0x4000,
                ];
                for t in types {
                    p.maneuvers.push(PhysicalManeuver {
                        style: if available { style } else { 0 },
                        attack_type: t,
                        height,
                        minimum_skill: 0,
                        motion: 0x10001000 + t,
                        duration: 1.0,
                        hooks: vec![PhysicalAttackHook {
                            seconds: 0.1,
                            part: 0,
                        }],
                    });
                }
                for motion in [
                    0x10000062, 0x10000063, 0x10000064, 0x10000065, 0x10000066, 0x10000067,
                    0x10000068, 0x10000069, 0x1000006a, 0x10000186, 0x10000187, 0x10000188,
                    0x10000189, 0x1000018a, 0x1000018b, 0x1000018c, 0x1000018d, 0x1000018e,
                    0x10000ffe,
                ] {
                    p.maneuvers.push(PhysicalManeuver {
                        style: 0,
                        attack_type: 0,
                        height: 0,
                        minimum_skill: 0,
                        motion,
                        duration: 1.0,
                        hooks: vec![PhysicalAttackHook {
                            seconds: 0.1,
                            part: 0,
                        }],
                    });
                }
                let selected = select_melee(&p, height, power, false).unwrap();
                assert_eq!(
                    p.maneuvers[selected.maneuver].motion,
                    c[6].parse::<u32>().unwrap(),
                    "{line}"
                );
                assert_eq!(
                    selected.speed.to_bits(),
                    c[8].parse::<u32>().unwrap(),
                    "{line}"
                );
                if style == 0x80000046 {
                    let selected = select_melee(&p, height, power, true).unwrap();
                    assert_eq!(
                        p.maneuvers[selected.maneuver].motion,
                        c[7].parse::<u32>().unwrap(),
                        "{line}"
                    );
                }
            }
            "imbue" => {
                let value = imbue(
                    c[1].parse().unwrap(),
                    150.0,
                    400.0,
                    c[2].parse().unwrap(),
                    c[3].parse().unwrap(),
                );
                assert_eq!(value, c[4].parse::<f64>().unwrap(), "{line}");
            }
            "rating" => {
                let rating: i32 = c[1].parse().unwrap();
                let fixed = physical_rating_modifier(rating).unwrap();
                if rating >= 0 {
                    assert_eq!(fixed, c[2].parse::<f32>().unwrap());
                } else {
                    assert!(fixed > 0.0 && fixed < 1.0);
                    if rating == -10 {
                        assert!(c[2].parse::<f32>().unwrap() > 1.0);
                    }
                }
            }
            "skill" => assert_eq!(
                bace_combat::skill_chance(c[1].parse().unwrap(), c[2].parse().unwrap()).unwrap(),
                c[3].parse::<f64>().unwrap()
            ),
            _ => panic!("unknown oracle"),
        }
    }
}
#[test]
fn physical_pk_matrix_and_invalid_preparation_fail_closed() {
    for a in [PkStatus::Npk, PkStatus::Pk, PkStatus::PkLite] {
        for b in [PkStatus::Npk, PkStatus::Pk, PkStatus::PkLite] {
            let mut source = prepared(true);
            source.pk = a;
            let mut target = prepared(true);
            target.pk = b;
            assert_eq!(
                physical_permission(&source, &target),
                a == b && a != PkStatus::Npk
            );
        }
    }
    let mut p = prepared(true);
    p.maneuvers[0].hooks[0].seconds = f64::NAN;
    assert!(validate_physical_profile(&p).is_err());
}
fn rolls() -> PhysicalRolls {
    PhysicalRolls {
        evade: 0.0,
        critical: 0.99,
        variance: 0.5,
        body_part: 0.0,
        attacker_stamina: 0.5,
        defender_stamina: 0.5,
        critical_defense: 0.99,
        sneak: 0.99,
        dirty: 0.0,
        weapon_proc: 0.99,
    }
}
fn hit(
    a: &PhysicalCombatProfile,
    d: &PhysicalCombatProfile,
    power: f32,
    height: u32,
    rolls: PhysicalRolls,
) -> PhysicalImpact {
    let mut selected = select_melee(a, 2, power, false).unwrap();
    selected.maneuver = 0;
    resolve_physical(PhysicalDamageInput {
        pk_override: None,
        rating_override: None,
        attacker: a,
        defender: d,
        selected,
        kind: PhysicalKind::Melee,
        hook_part: 0,
        height,
        original_power: power,
        power,
        target_angle_degrees: 0.0,
        defender_stamina: 100,
        defender_in_combat: true,
        rolls,
    })
    .unwrap()
}
#[test]
fn shield_multistrike_thrust_preserves_pierce_even_at_high_power() {
    let mut a = prepared(true);
    a.main.as_mut().unwrap().attack_type = 0xa0;
    a.maneuvers[0].attack_type = 0x80;
    a.maneuvers[0].motion = 0x10000125;
    let d = prepared(false);
    assert_eq!(hit(&a, &d, 1.0, 2, rolls()).damage_type, 2);
    a.maneuvers[0].motion = 0x10000063;
    assert_eq!(hit(&a, &d, 1.0, 2, rolls()).damage_type, 1);
}
#[test]
fn defense_never_amplifies_and_critical_removes_recklessness() {
    let mut a = prepared(true);
    a.skills.push((
        50,
        PhysicalSkill {
            advancement: 3,
            current: 300,
        },
    ));
    let mut d = prepared(false);
    let normal = hit(&a, &d, 0.5, 2, rolls());
    assert_eq!(normal.attacker_reckless_rating, 20);
    d.ratings.resistance = 50;
    assert!(hit(&a, &d, 0.5, 2, rolls()).damage < normal.damage);
    let mut r = rolls();
    r.critical = 0.0;
    let critical = hit(&a, &d, 0.5, 2, r);
    assert!(critical.critical);
    assert_eq!(critical.attacker_reckless_rating, 0);
    assert_eq!(critical.attack_conditions & 2, 0);
    for p in [0.1, 0.9] {
        assert_eq!(hit(&a, &d, p, 2, rolls()).attacker_reckless_rating, 0);
    }
}
#[test]
fn gdle_dirty_fighting_high_medium_low_and_evasion_are_actual_impacts() {
    let mut a = prepared(true);
    a.skills.push((
        52,
        PhysicalSkill {
            advancement: 3,
            current: 300,
        },
    ));
    let mut d = prepared(false);
    assert_eq!(hit(&a, &d, 0.5, 1, rolls()).dirty_spells, [5938, 5941]);
    assert_eq!(hit(&a, &d, 0.5, 2, rolls()).dirty_spells, [5939, 0]);
    assert_eq!(hit(&a, &d, 0.5, 3, rolls()).dirty_spells, [5940, 0]);
    d.skills[0].1.current = 1000;
    let mut r = rolls();
    r.evade = 0.999;
    let miss = hit(&a, &d, 0.5, 1, r);
    assert!(miss.evaded);
    assert_eq!(miss.damage, 0);
    assert_eq!(miss.dirty_count, 0);
}

fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}

#[test]
fn untrusted_power_and_original_power_are_bounded_at_selection_and_impact() {
    let a = prepared(true);
    let d = prepared(false);
    let selected = select_melee(&a, 2, 0.5, false).unwrap();
    for invalid in [
        f32::from_bits(0x3f800001),
        -f32::from_bits(1),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert_eq!(
            select_melee(&a, 2, invalid, false),
            Err(PhysicalError::InvalidInput)
        );
        for original in [false, true] {
            let result = resolve_physical(PhysicalDamageInput {
                attacker: &a,
                defender: &d,
                pk_override: None,
                rating_override: None,
                selected,
                kind: PhysicalKind::Melee,
                hook_part: 0,
                height: 2,
                original_power: if original { invalid } else { 0.5 },
                power: if original { 0.5 } else { invalid },
                target_angle_degrees: 0.0,
                defender_stamina: 100,
                defender_in_combat: true,
                rolls: rolls(),
            });
            assert_eq!(result, Err(PhysicalError::InvalidInput));
        }
    }
}

#[test]
fn proc_phase_cannot_reroll_contact_but_can_change_target_mitigation() {
    let a = prepared(true);
    let d = prepared(false);
    let selected = select_melee(&a, 2, 0.5, false).unwrap();
    let input = PhysicalDamageInput {
        pk_override: None,
        rating_override: None,
        attacker: &a,
        defender: &d,
        selected,
        kind: PhysicalKind::Melee,
        hook_part: 0,
        height: 2,
        original_power: 0.5,
        power: 0.5,
        target_angle_degrees: 0.0,
        defender_stamina: 100,
        defender_in_combat: true,
        rolls: rolls(),
    };
    let contact = prepare_physical_contact(input).unwrap();
    let before = finish_physical_contact(input, contact).unwrap();
    let mut buffed = a.clone();
    buffed.main.as_mut().unwrap().damage *= 100.0;
    buffed.main.as_mut().unwrap().biting = 1.0;
    let mut protected = d.clone();
    protected.skills[0].1.current = 100000;
    protected.armor[0].armor = [quality(190.0 / 3.0); 8];
    let after = finish_physical_contact(
        PhysicalDamageInput {
            attacker: &buffed,
            defender: &protected,
            ..input
        },
        contact,
    )
    .unwrap();
    assert!(!after.evaded);
    assert_eq!(after.critical, before.critical);
    assert!(after.damage < before.damage);
    assert!(after.damage >= before.damage / 2);
}
