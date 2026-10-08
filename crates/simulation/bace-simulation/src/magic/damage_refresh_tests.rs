//! Wand identity is cached by a spell; source GDLE looks up current object
//! qualities and current wielder at collision, independently of later equips.
use super::*;
fn wand(entity: u32, revision: u64, elemental: f64) -> bace_magic::MagicWand {
    bace_magic::MagicWand {
        entity,
        revision,
        damage_type: 16,
        elemental_modifier: elemental,
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
    }
}
fn damage(magic: &Magic, world: &World) -> u32 {
    let flying = &magic.flying[&EntityId(90)];
    let mut random = flying.random.clone();
    let spec = ProjectileSpec {
        template: 1,
        shape: bace_magic::ProjectileShape::Bolt,
        count: 1,
        radius: 0.1,
        speed: 20.,
        gravity: 0.,
        tracking: false,
        perturbation: Vec3::ZERO,
        lifetime: 30.,
        spread_degrees: 0.,
        padding: Vec3::ZERO,
        offset: Vec3::ZERO,
        dimensions: [1, 1, 1],
        minimum_damage: 4,
        maximum_damage: 4,
        damage_type: 16,
        enchantment: None,
    };
    magic
        .resolved_projectile_damage(flying, EntityId(1), EntityId(90), &spec, &mut random, world)
        .unwrap()
}
#[test]
fn current_wand_revision_follows_same_identity_without_switching_to_new_weapon() {
    let (mut magic, _) = super::projectile_tests::fixture(true);
    let world = super::periodic_tests::world(true, 100);
    for id in [1, 2, 3, 10] {
        magic
            .register_registry(
                EntityId(id),
                EnchantmentRegistry::new(16).unwrap(),
                false,
                0.,
            )
            .unwrap();
    }
    magic
        .register_damage_profile(EntityId(1), bace_magic::MagicDamageProfile::neutral(true))
        .unwrap();
    let mut source = bace_magic::MagicDamageProfile::neutral(false);
    source.wand = Some(wand(10, 1, 1.));
    magic
        .register_damage_profile(EntityId(2), source.clone())
        .unwrap();
    magic.flying.get_mut(&EntityId(90)).unwrap().launch_wand = source.wand.clone();
    magic.damage_spell_levels.insert(123, 4);
    assert_eq!(damage(&magic, &world), 3);
    source.wand = Some(wand(10, 2, 3.));
    magic
        .refresh_damage_profile(EntityId(2), source.clone())
        .unwrap();
    assert_eq!(damage(&magic, &world), 9);
    source.elemental_modifier = 5.;
    source.wand = Some(wand(20, 1, 9.));
    magic.refresh_damage_profile(EntityId(2), source).unwrap();
    assert_eq!(
        damage(&magic, &world),
        9,
        "old unequipped wand no longer inherits former owner aura"
    );
    let mut new_owner = bace_magic::MagicDamageProfile::neutral(true);
    new_owner.elemental_modifier = 2.;
    new_owner.wand = Some(wand(10, 2, 3.));
    magic
        .register_damage_profile(EntityId(3), new_owner)
        .unwrap();
    assert_eq!(
        damage(&magic, &world),
        15,
        "actual current wielder supplies the aura"
    );
    magic.refresh_damage_wand(wand(10, 3, 4.)).unwrap();
    assert_eq!(damage(&magic, &world), 18);
    assert!(magic.refresh_damage_wand(wand(10, 2, 99.)).is_err());
    assert_eq!(damage(&magic, &world), 18);
    magic.reserve_registry(EntityId(10), true, 0.).unwrap();
    magic.retire_item_registry(EntityId(10), 0.).unwrap();
    assert!(
        damage(&magic, &world) <= 3,
        "deleted wand cannot buff remaining projectile"
    );
}
