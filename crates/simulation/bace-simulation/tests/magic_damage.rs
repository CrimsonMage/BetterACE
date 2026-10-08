mod magic_common;
use bace_gameplay_api::{CastOrigin, CastRejection};
use bace_magic::{EnchantmentSpec, MagicDamageProfile, MagicWand};
use magic_common::*;
fn prepared() -> Kernel {
    prepared_on(kernel(64))
}
fn prepared_with_heading(heading: f32) -> Kernel {
    prepared_on(kernel_fixture_heading(64, false, false, true, heading))
}
fn prepared_on(mut k: Kernel) -> Kernel {
    let mut source = MagicDamageProfile::neutral(true);
    source.wand = Some(MagicWand {
        entity: 900,
        revision: 1,
        damage_type: 16,
        elemental_modifier: 2.,
        elemental_present: true,
        inherit_wielder: true,
        imbues: 0,
        biting: 1.,
        double_enchant_biting: true,
        crushing: 0.,
        double_enchant_crushing: true,
        slayer_type: 0,
        slayer_bonus: 0.,
        resistance_cleaving: None,
        ignore_magic_resistance: false,
    });
    k.register_magic_damage_profile(EntityId(1), source)
        .unwrap();
    k.register_magic_damage_profile(EntityId(2), MagicDamageProfile::neutral(false))
        .unwrap();
    let mut spec = shot(ProjectileShape::Bolt, 1);
    spec.minimum_damage = 4;
    spec.maximum_damage = 4;
    let mut prepared = spell(100, SpellEffect::Projectile(spec));
    prepared.spell.school = MagicSchool::War;
    k.register_magic_spell(prepared).unwrap();
    k.supply_projectile_id(EntityId(1000)).unwrap();
    k
}
#[test]
fn ring_and_strike_apply_live_protection_through_the_combat_damage_owner_once() {
    for (shape, shots, self_target, heading) in [
        (ProjectileShape::Ring, 8, true, std::f32::consts::PI / 8.0),
        (ProjectileShape::Strike, 1, false, 0.0),
    ] {
        let mut k = prepared_with_heading(heading);
        let mut projectile = shot(shape, shots);
        projectile.minimum_damage = 4;
        projectile.maximum_damage = 4;
        let mut attack = spell(102, SpellEffect::Projectile(projectile));
        attack.spell.school = MagicSchool::War;
        k.register_magic_spell(attack).unwrap();
        k.register_magic_damage_spell(102, 4).unwrap();
        k.register_magic_spell(spell(
            101,
            SpellEffect::Enchantment(EnchantmentSpec {
                category: 900,
                power: 100,
                duration: 30.,
                layer: 1,
                stat_type: 0x5008,
                stat_key: 68,
                value: 0.5,
                beneficial: true,
                set_id: None,
            }),
        ))
        .unwrap();
        k.register_enchantment_metadata(101, bace_magic::EnchantmentMetadata::default())
            .unwrap();
        for id in 1001..1000 + u32::from(shots) {
            k.supply_projectile_id(EntityId(id)).unwrap();
        }
        k.enqueue(Command::Cast {
            context: context(1),
            request: CastRequest::Targeted {
                target: EntityId(if self_target { 1 } else { 2 }),
                spell: 102,
            },
        })
        .unwrap();
        let mut created = 0;
        for _ in 0..12 {
            created += step(&mut k)
                .iter()
                .filter(|event| matches!(event, MagicEvent::ProjectileCreated { .. }))
                .count();
            if created == shots as usize {
                break;
            }
        }
        assert_eq!(created, shots as usize);
        k.cast_from_server(
            CastOrigin::Emote {
                actor: EntityId(1),
                event: 900,
                instant: true,
            },
            CastRequest::Targeted {
                target: EntityId(2),
                spell: 101,
            },
        )
        .unwrap();
        let mut target_hits = 0;
        for _ in 0..60 {
            target_hits += step(&mut k)
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        MagicEvent::Vital {
                            actor: EntityId(2),
                            ..
                        }
                    )
                })
                .count();
        }
        assert_eq!(target_hits, 1);
        assert_eq!(
            k.world()
                .vital(EntityId(2), EntityVital::Health)
                .unwrap()
                .current,
            42
        );
    }
}
#[test]
fn missing_formula_preparation_rejects_before_motion_or_mana() {
    let mut k = prepared();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    assert!(step(&mut k).is_empty());
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::MissingAssets)
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        100
    );
}
#[test]
fn full_impact_uses_wand_crit_skill_bonus_and_live_target_protection_once() {
    let mut k = prepared();
    k.register_magic_damage_spell(100, 4).unwrap();
    k.register_magic_spell(spell(
        101,
        SpellEffect::Enchantment(EnchantmentSpec {
            category: 900,
            power: 100,
            duration: 30.,
            layer: 1,
            stat_type: 0x5008,
            stat_key: 68,
            value: 0.5,
            beneficial: true,
            set_id: None,
        }),
    ))
    .unwrap();
    k.register_enchantment_metadata(101, bace_magic::EnchantmentMetadata::default())
        .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    let mut launched = false;
    for _ in 0..12 {
        if step(&mut k)
            .iter()
            .any(|e| matches!(e, MagicEvent::ProjectileCreated { .. }))
        {
            launched = true;
            break;
        }
    }
    assert!(launched);
    k.cast_from_server(
        CastOrigin::Emote {
            actor: EntityId(1),
            event: 900,
            instant: true,
        },
        CastRequest::Targeted {
            target: EntityId(2),
            spell: 101,
        },
    )
    .unwrap();
    for _ in 0..30 {
        step(&mut k);
    }
    // Original source pipeline: (4*2 elemental + 4 critical + 4 skill) * .5 prot.
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        42
    );
    assert_eq!(
        k.world()
            .vital(EntityId(1), EntityVital::Mana)
            .unwrap()
            .current,
        90
    );
    for _ in 0..30 {
        step(&mut k);
    }
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        42
    );
}

#[test]
fn distinct_school_projection_refreshes_defense_but_retains_active_source_skill() {
    let mut k = kernel_with_school_values(64, [350, 250, 150, 50, 450]);
    let mut source = MagicDamageProfile::neutral(true);
    source.wand = Some(MagicWand {
        entity: 900,
        revision: 1,
        damage_type: 16,
        elemental_modifier: 1.,
        elemental_present: true,
        inherit_wielder: false,
        imbues: 0,
        biting: 1.,
        double_enchant_biting: true,
        crushing: 0.,
        double_enchant_crushing: true,
        slayer_type: 0,
        slayer_bonus: 0.,
        resistance_cleaving: None,
        ignore_magic_resistance: false,
    });
    k.register_magic_damage_profile(EntityId(1), source)
        .unwrap();
    k.register_magic_damage_profile(EntityId(2), MagicDamageProfile::neutral(false))
        .unwrap();
    let mut projectile = shot(ProjectileShape::Bolt, 1);
    projectile.minimum_damage = 4;
    projectile.maximum_damage = 4;
    let mut attack = spell(100, SpellEffect::Projectile(projectile));
    attack.spell.school = MagicSchool::War;
    attack.spell.range_constant = 0.;
    attack.spell.range_per_skill = 0.02;
    k.register_magic_spell(attack).unwrap();
    k.register_magic_damage_spell(100, 4).unwrap();
    k.register_magic_spell(spell(
        101,
        SpellEffect::Enchantment(EnchantmentSpec {
            category: 900,
            power: 100,
            duration: 30.,
            layer: 1,
            stat_type: 0x9010,
            stat_key: 34,
            value: -200.,
            beneficial: false,
            set_id: None,
        }),
    ))
    .unwrap();
    k.register_enchantment_metadata(101, bace_magic::EnchantmentMetadata::default())
        .unwrap();
    k.supply_projectile_id(EntityId(1000)).unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    assert!(k.take_cast_outcome().unwrap().result.is_ok());
    k.cast_from_server(
        CastOrigin::Emote {
            actor: EntityId(1),
            event: 800,
            instant: true,
        },
        CastRequest::Targeted {
            target: EntityId(1),
            spell: 101,
        },
    )
    .unwrap();
    for _ in 0..30 {
        step(&mut k);
    }
    assert_eq!(
        k.character_skill_projection(EntityId(1), 34)
            .unwrap()
            .1
            .current,
        150
    );
    assert_eq!(
        k.world()
            .vital(EntityId(2), EntityVital::Health)
            .unwrap()
            .current,
        40
    );
    while k.take_cast_outcome().is_some() {}
    for _ in 0..60 {
        step(&mut k);
    }
    k.enqueue(Command::Cast {
        context: context(2),
        request: CastRequest::Targeted {
            target: EntityId(2),
            spell: 100,
        },
    })
    .unwrap();
    step(&mut k);
    assert_eq!(
        k.take_cast_outcome().unwrap().result,
        Err(CastRejection::OutOfRange)
    );
}
