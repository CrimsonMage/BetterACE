//! Contact and tick ordering regressions, not full collision-response parity.
use super::*;

pub(super) fn fixture(source_present: bool) -> (Magic, World) {
    let mut world = super::specialization_tests::world();
    if source_present {
        let body = bace_physics::Body::spawn_oriented(
            world.scene(CellId(1)).unwrap(),
            Vec3::new(0.0, -3.0, 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
            0.0,
            3.0,
        )
        .unwrap();
        world
            .insert(bace_entity::Actor {
                id: EntityId(2),
                cell: CellId(1),
                body,
            })
            .unwrap();
        let profile = world.combatant(EntityId(1)).unwrap().profile().clone();
        world
            .register_combatant(EntityId(2), bace_entity::Combatant::new(profile).unwrap())
            .unwrap();
    }
    let mut magic = Magic::new(16);
    magic.flying.insert(
        EntityId(90),
        Flying {
            proc_parent: None,
            pending_damage: None,
            launch_wand: None,
            source: EntityId(2),
            target: Some(EntityId(1)),
            spell: Arc::new(PreparedMagicSpell {
                spell: PreparedSpell {
                    id: 123,
                    school: bace_magic::MagicSchool::War,
                    power: 100,
                    base_mana: 1,
                    range_constant: 75.0,
                    range_per_skill: 0.0,
                    harmful: true,
                    resistable: false,
                    effect: SpellEffect::Boost {
                        vital: bace_magic::Vital::Health,
                        minimum: -20,
                        maximum: -20,
                    },
                },
                gestures: vec![],
                components: vec![],
                component_modifiers: vec![],
                component_loss: 0.0,
                fast_resistable_pk_spell: false,
            }),
            random: RandomRoot::new([1; 32], 1)
                .unwrap()
                .event_stream([2; 16], Domain::Magic)
                .unwrap(),
            lifetime: bace_magic::SpellProjectileLifetime::new(0.0).unwrap(),
            initial_cast: (CellId(1), Vec3::ZERO),
            maximum_range: 75.0,
            cast_skill: 100,
            resting: false,
            pending_impact: None,
            life_damage: None,
            credited_owner: EntityId(2),
        },
    );
    (magic, world)
}

#[test]
fn protected_target_or_missing_source_does_not_become_physics_pass_through() {
    for source_present in [false, true] {
        let (mut magic, mut world) = fixture(source_present);
        let policy = Combat::new(16);
        for tick in 1..=20 {
            magic.step_projectiles(&mut world, f64::from(tick) / 30.0, &policy);
        }
        let flying = &magic.flying[&EntityId(90)];
        assert!(flying.resting);
        assert!(!flying.lifetime.exploded());
        assert!(world.projectile(EntityId(90)).unwrap().body.finished());
        assert!(magic.events.is_empty());
        assert_eq!(world.combatant(EntityId(1)).unwrap().health(), 100);
        magic.step_projectiles(&mut world, 29.0, &policy);
        assert!(matches!(
            magic.take_event(),
            Some(MagicEvent::ProjectileExploded {
                actor: EntityId(90),
                ..
            })
        ));
        assert!(world.projectile(EntityId(90)).is_some());
        magic.step_projectiles(&mut world, 30.0, &policy);
        assert!(matches!(
            magic.take_event(),
            Some(MagicEvent::ProjectileRemoved {
                actor: EntityId(90),
                ..
            })
        ));
        assert!(world.projectile(EntityId(90)).is_none());
    }
}

#[test]
fn range_expiration_observes_position_after_current_physics_step() {
    let (mut magic, mut world) = fixture(false);
    let flying = magic.flying.get_mut(&EntityId(90)).unwrap();
    flying.initial_cast = (CellId(1), Vec3::new(0.0, 2.0, 0.5));
    flying.maximum_range = 1.01;
    assert_eq!(
        world
            .projectile_distance_from(EntityId(90), CellId(1), flying.initial_cast.1)
            .unwrap(),
        1.0
    );
    magic.step_projectiles(&mut world, 1.0 / 30.0, &Combat::new(16));
    assert!(matches!(
        magic.take_event(),
        Some(MagicEvent::ProjectileExploded {
            actor: EntityId(90),
            ..
        })
    ));
    assert!(magic.flying[&EntityId(90)].lifetime.exploded());
}
#[test]
fn beneficial_header_cannot_turn_projectile_damage_into_a_protection_bypass() {
    let (mut magic, _) = fixture(true);
    let mut world = super::periodic_tests::world(true, 100);
    world
        .combatant_mut(EntityId(1))
        .unwrap()
        .set_lifestone_protected(true);
    let flying = magic.flying.get_mut(&EntityId(90)).unwrap();
    let mut spell = flying.spell.as_ref().clone();
    spell.spell.harmful = false;
    spell.spell.resistable = false;
    flying.spell = Arc::new(spell);
    for tick in 1..=30 {
        magic.step_projectiles(&mut world, f64::from(tick) / 30., &Combat::new(64));
    }
    assert!(magic.flying[&EntityId(90)].resting);
    assert!(!magic.flying[&EntityId(90)].lifetime.exploded());
    assert_eq!(world.combatant(EntityId(1)).unwrap().health(), 100);
}
