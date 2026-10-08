use super::*;
use bace_combat::specialization::CombatSkill;
use bace_gameplay_api::SkillAdvancement;
fn skill(advancement: SkillAdvancement, base: u32) -> CombatSkill {
    CombatSkill {
        advancement,
        base,
        current: base,
    }
}
pub(super) fn world() -> World {
    use bace_geometry::Aabb;
    use bace_motion::Capabilities;
    use bace_physics::{Body, SyntheticScene};
    use bace_types::CellId;
    let mut world = World::default();
    world
        .register_scene(
            CellId(1),
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    let body = Body::spawn_oriented(
        world.scene(CellId(1)).unwrap(),
        Vec3::new(0.0, 0.0, 0.5),
        0.5,
        Capabilities {
            speed: 5.0,
            jump_impulse: 5.0,
        },
        0.0,
        3.0,
    )
    .unwrap();
    world
        .insert(bace_entity::Actor {
            id: EntityId(1),
            cell: CellId(1),
            body,
        })
        .unwrap();
    let mut target = bace_entity::Combatant::new(bace_entity::CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 1.0,
        attack_duration: 1.0,
        strike_offsets: vec![0.5],
        player: true,
    })
    .unwrap();
    target.set_mode(2);
    world.register_combatant(EntityId(1), target).unwrap();
    for (id, y) in [(90, 1.0), (91, -1.0)] {
        world
            .insert_projectile(
                EntityId(id),
                OwnedProjectile {
                    cell: CellId(1),
                    source: EntityId(2),
                    target: Some(EntityId(1)),
                    body: ProjectileBody::new(
                        Vec3::new(0.0, y, 0.5),
                        Vec3::new(0.0, -1.0, 0.0),
                        0.1,
                        0.0,
                        5.0,
                    )
                    .unwrap(),
                },
            )
            .ok()
            .unwrap();
    }
    world
}
fn magic(capacity: usize) -> Magic {
    let mut magic = Magic::new(capacity);
    magic
        .register_registry(EntityId(1), EnchantmentRegistry::new(8).unwrap(), true, 0.0)
        .unwrap();
    magic
}
fn spell(id: u32) -> PreparedMagicSpell {
    PreparedMagicSpell {
        spell: PreparedSpell {
            id,
            school: bace_magic::MagicSchool::Creature,
            power: 100,
            base_mana: 0,
            range_constant: 10.0,
            range_per_skill: 0.0,
            harmful: true,
            resistable: false,
            effect: SpellEffect::Enchantment(bace_magic::EnchantmentSpec {
                category: id as u16,
                power: 100,
                duration: 20.0,
                layer: 0,
                stat_type: 0x1000,
                stat_key: 7,
                value: -10.0,
                beneficial: false,
                set_id: None,
            }),
        },
        gestures: vec![],
        components: vec![],
        component_modifiers: vec![],
        component_loss: 0.0,
        fast_resistable_pk_spell: false,
    }
}
#[test]
fn projectile_uses_accepted_front_geometry_and_specialized_base_defense() {
    let world = world();
    let mut magic = magic(8);
    let profile = MagicDefenseProfile {
        player: true,
        magic_defense: skill(SkillAdvancement::Specialized, 400),
        shield: Some((skill(SkillAdvancement::Specialized, 300), 0.5)),
    };
    magic.refresh_magic_defenses(EntityId(1), profile).unwrap();
    // Pinned helpers' C# oracle covers base400 -> rating8 and shield300/cap0.5
    // -> absorption0.3. Integration must combine and round once after both.
    assert_eq!(
        magic.projectile_damage(EntityId(2), EntityId(1), EntityId(90), 100, &world),
        Ok(65)
    );
    assert_eq!(
        magic.projectile_damage(EntityId(2), EntityId(1), EntityId(91), 100, &world),
        Ok(93)
    );
    magic.casters.insert(
        EntityId(2),
        MagicCaster {
            player: true,
            known_spells: Default::default(),
            school_skills: [0; 5],
            magic_defense: 0,
            mana_conversion: 0,
            components_required: false,
            safe_components: true,
        },
    );
    assert_eq!(
        magic.projectile_damage(EntityId(2), EntityId(1), EntityId(90), 100, &world),
        Ok(73)
    );
    magic
        .refresh_magic_defenses(
            EntityId(1),
            MagicDefenseProfile {
                player: false,
                ..profile
            },
        )
        .unwrap();
    assert_eq!(
        magic.projectile_damage(EntityId(2), EntityId(1), EntityId(90), 100, &world),
        Ok(70)
    );
}
#[test]
fn high_proc_requires_both_prepared_spells_and_preserves_state_on_refusal() {
    let mut magic = magic(2);
    magic.register_spell(spell(5938)).unwrap();
    magic
        .register_enchantment_metadata(5938, EnchantmentMetadata::default())
        .unwrap();
    let apply = |magic: &mut Magic| {
        magic.apply_prepared_enchantments(EntityId(2), EntityId(1), &[5938, 5941], 0.0)
    };
    assert_eq!(apply(&mut magic), Err(CastRejection::MissingAssets));
    assert_eq!(magic.registry(EntityId(1)).unwrap().revision(), 0);
    assert!(magic.events.is_empty());
    magic.register_spell(spell(5941)).unwrap();
    magic
        .register_enchantment_metadata(5941, EnchantmentMetadata::default())
        .unwrap();
    magic.reserve_registry(EntityId(1), true, 0.0).unwrap();
    assert_eq!(apply(&mut magic), Err(CastRejection::Busy));
    magic.reserve_registry(EntityId(1), false, 0.0).unwrap();
    magic.events.push_back(MagicEvent::ProjectileRemoved {
        tick: 0,
        actor: EntityId(90),
    });
    assert_eq!(apply(&mut magic), Err(CastRejection::Capacity));
    magic.take_event();
    apply(&mut magic).unwrap();
    let registry = magic.registry(EntityId(1)).unwrap();
    assert_eq!(registry.entries().len(), 2);
    assert_eq!(registry.revision(), 2);
    assert_eq!(magic.events.len(), 2);
    assert!(magic.attempts.is_empty());
    assert!(magic.random.is_none());
}
#[test]
fn registry_capacity_failure_cannot_partially_apply_two_effects() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), EnchantmentRegistry::new(1).unwrap(), true, 0.0)
        .unwrap();
    for id in [5938, 5941] {
        magic.register_spell(spell(id)).unwrap();
        magic
            .register_enchantment_metadata(id, EnchantmentMetadata::default())
            .unwrap();
    }
    assert!(
        magic
            .apply_prepared_enchantments(EntityId(2), EntityId(1), &[5938, 5941], 0.0)
            .is_err()
    );
    assert!(magic.registry(EntityId(1)).unwrap().entries().is_empty());
    assert!(magic.events.is_empty());
}
