use bace_magic::*;
fn row() -> bace_content::SpellRowV1 {
    bace_content::SpellRowV1 {
        id: 42,
        name: "native ring".into(),
        stat_mod_type: None,
        stat_mod_key: None,
        stat_mod_val: None,
        e_type: None,
        base_intensity: None,
        variance: None,
        wcid: None,
        num_projectiles: None,
        num_projectiles_variance: None,
        spread_angle: None,
        vertical_angle: None,
        default_launch_angle: None,
        non_tracking: None,
        create_offset_origin_x: None,
        create_offset_origin_y: None,
        create_offset_origin_z: None,
        padding_origin_x: None,
        padding_origin_y: None,
        padding_origin_z: None,
        dims_origin_x: None,
        dims_origin_y: None,
        dims_origin_z: None,
        peturbation_origin_x: None,
        peturbation_origin_y: None,
        peturbation_origin_z: None,
        imbued_effect: None,
        slayer_creature_type: None,
        slayer_damage_bonus: None,
        crit_freq: None,
        crit_multiplier: None,
        ignore_magic_resist: None,
        elemental_modifier: None,
        drain_percentage: None,
        damage_ratio: None,
        damage_type: None,
        boost: None,
        boost_variance: None,
        source: None,
        destination: None,
        proportion: None,
        loss_percent: None,
        source_loss: None,
        transfer_cap: None,
        max_boost_allowed: None,
        transfer_bitfield: None,
        index: None,
        link: None,
        position_obj_cell_id: None,
        position_origin_x: None,
        position_origin_y: None,
        position_origin_z: None,
        position_angles_w: None,
        position_angles_x: None,
        position_angles_y: None,
        position_angles_z: None,
        min_power: None,
        max_power: None,
        power_variance: None,
        dispel_school: None,
        align: None,
        number: None,
        number_variance: None,
        dot_duration: None,
        last_modified: "fixture".into(),
    }
}

fn header(meta: u32) -> NativeSpellHeader {
    NativeSpellHeader {
        id: 42,
        school: 1,
        category: 222,
        flags: 0x4003,
        base_mana: 17,
        power: 250,
        range_constant: 30.,
        range_modifier: 0.1,
        meta_type: meta,
        enchantment: None,
        portal_lifetime: None,
    }
}
#[test]
fn native_ring_requires_exact_geometry_fields_and_preserves_damage_and_flags() {
    let mut row = row();
    row.wcid = Some(900);
    row.num_projectiles = Some(8);
    row.dims_origin_x = Some(8.);
    row.dims_origin_y = Some(1.);
    row.dims_origin_z = Some(1.);
    row.spread_angle = Some(360.);
    row.base_intensity = Some(13);
    row.variance = Some(7);
    row.e_type = Some(16);
    let motion = NativeProjectileMotion {
        template: 900,
        radius: 0.15,
        speed: 22.,
        gravity: 0.,
    };
    assert!(matches!(
        prepare_native_spell(header(2), &row, None),
        Err(NativeSpellError::Missing(_))
    ));
    let definition = prepare_native_spell(header(2), &row, Some(motion)).unwrap();
    let SpellEffect::Projectile(p) = definition.spell.effect else {
        panic!("wrong family")
    };
    assert_eq!(
        (p.count, p.dimensions, p.minimum_damage, p.maximum_damage),
        (8, [8, 1, 1], 13, 20)
    );
    assert_eq!(p.shape, ProjectileShape::Ring);
    assert_eq!((p.radius, p.speed), (0.15, 22.));
    assert!(definition.fast_resistable_pk_spell);
    row.dims_origin_x = Some(7.);
    assert!(prepare_native_spell(header(2), &row, Some(motion)).is_err());
    row.dims_origin_x = Some(f32::NAN);
    assert!(prepare_native_spell(header(2), &row, Some(motion)).is_err());
}
#[test]
fn native_source_roles_and_metadata_are_not_inferred_from_target_packets() {
    let mut row = row();
    row.source = Some(4);
    row.destination = Some(6);
    row.proportion = Some(0.33);
    row.loss_percent = Some(0.1);
    row.transfer_cap = Some(99);
    row.transfer_bitfield = Some(1 | 4);
    let definition = prepare_native_spell(header(4), &row, None).unwrap();
    assert!(matches!(
        definition.spell.effect,
        SpellEffect::Transfer {
            source: Vital::Stamina,
            destination: Vital::Mana,
            source_is_caster: true,
            destination_is_caster: true,
            cap: 99,
            ..
        }
    ));
    row.transfer_bitfield = Some(1 | 2 | 4);
    assert!(prepare_native_spell(header(4), &row, None).is_err());
    row.stat_mod_type = Some(0x9004);
    row.stat_mod_key = Some(350);
    row.stat_mod_val = Some(27.);
    let mut h = header(1);
    h.school = 3;
    h.flags = 4;
    h.enchantment = Some((10., 0.25, 0.5));
    let definition = prepare_native_spell(h, &row, None).unwrap();
    assert_eq!(definition.metadata.unwrap().degrade_modifier, 0.25);
    assert!(matches!(
        definition.spell.effect,
        SpellEffect::Enchantment(EnchantmentSpec {
            duration: 10.,
            stat_key: 350,
            value: 27.,
            beneficial: true,
            ..
        })
    ));
}
#[test]
fn native_skill_quality_join_distinguishes_sneak_deception_and_unrelated_skills() {
    let weenie = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "actor".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let skills = [
        (15, 123),
        (19, 100),
        (20, 200),
        (23, 999),
        (50, 888),
        (51, 300),
    ]
    .map(|(id, current)| {
        (
            id,
            bace_gameplay_api::weapon_combat::PhysicalSkill {
                advancement: 3,
                current,
            },
        )
    });
    let result = prepare_magic_damage_profile(MagicDamagePreparation {
        player: true,
        weenie: &weenie,
        equipment: &[],
        skills: &skills,
        base_attributes: [40, 50, 60, 70, 80, 90],
        base_shield_skill: 17,
    })
    .unwrap();
    assert_eq!(
        (
            result.magic_defense.current,
            result.assess_person.current,
            result.deception.current,
            result.sneak.current
        ),
        (123, 100, 200, 300)
    );
    assert_eq!((result.base_strength, result.base_endurance), (40, 50));
}
#[test]
fn raw_native_school_ids_do_not_reuse_internal_skill_array_discriminants() {
    let mut row = row();
    row.stat_mod_type = Some(0x9004);
    row.stat_mod_key = Some(350);
    row.stat_mod_val = Some(27.);
    for (native, expected) in [(3, MagicSchool::Item), (4, MagicSchool::Creature)] {
        let mut h = header(1);
        h.school = native;
        h.enchantment = Some((10., 0., 0.));
        assert_eq!(
            prepare_native_spell(h, &row, None).unwrap().spell.school,
            expected
        );
    }
}
