use super::*;
fn program(component: u32, motion: u32) -> PreparedMagicDefinition {
    let chain = bace_motion::PreparedMotionChain::prepare(
        vec![bace_motion::ExecutionClip {
            animation: 0x03000001,
            frame_count: 1,
            low: 0,
            high: 0,
            framerate: 30.,
            frames: Arc::from([bace_motion::RootFrame {
                translation: Vec3::ZERO,
                heading: 0.,
            }]),
            hooks: Arc::from([]),
        }],
        0,
        0,
        bace_motion::MotionPhysics {
            velocity: Vec3::ZERO,
            omega: Vec3::ZERO,
        },
        motion,
        2.,
    )
    .unwrap();
    PreparedMagicDefinition {
        spell: PreparedMagicSpell {
            spell: PreparedSpell {
                id: 100,
                school: bace_magic::MagicSchool::Life,
                power: 10,
                base_mana: 5,
                range_constant: 10.,
                range_per_skill: 0.,
                harmful: false,
                resistable: false,
                effect: SpellEffect::Boost {
                    vital: bace_magic::Vital::Health,
                    minimum: 1,
                    maximum: 2,
                },
            },
            gestures: vec![PreparedCastGesture {
                gesture: CastGesture {
                    motion,
                    minimum_seconds: 0.1,
                },
                duration_seconds: 0.,
                motion_chain: Some(Arc::new(chain)),
            }],
            components: vec![(component, 1)],
            component_modifiers: vec![(component, 0.5)],
            component_loss: 0.25,
            fast_resistable_pk_spell: false,
        },
        metadata: None,
        category: 0,
        formula_level: 1,
        flags: 4,
        target_mask: 1,
    }
}
fn caster() -> MagicCaster {
    MagicCaster {
        player: true,
        known_spells: BTreeSet::from([100]),
        school_skills: [100; 5],
        magic_defense: 1,
        mana_conversion: 1,
        components_required: true,
        safe_components: false,
    }
}
fn admitted_world() -> World {
    let mut world = World::default();
    let cell = bace_types::CellId(1);
    world
        .register_scene(
            cell,
            bace_physics::SyntheticScene::new(
                0.0,
                bace_geometry::Aabb::new(
                    Vec3::new(-10.0, -10.0, -1.0),
                    Vec3::new(10.0, 10.0, 10.0),
                )
                .unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=3 {
        let body = bace_physics::Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32 * 2.0, 0.0, 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(bace_entity::Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        let mut combatant = bace_entity::Combatant::new(bace_entity::CombatantProfile {
            maximum_health: 100,
            melee_damage: 1,
            melee_range: 2.0,
            attack_duration: 1.0,
            strike_offsets: vec![0.5],
            player: true,
        })
        .unwrap()
        .with_resources(
            Some(bace_entity::VitalPool {
                current: 100,
                maximum: 100,
            }),
            Some(bace_entity::VitalPool {
                current: 100,
                maximum: 100,
            }),
        )
        .unwrap();
        combatant.set_mode(8);
        world.register_combatant(EntityId(id), combatant).unwrap();
    }
    world
}
#[test]
fn two_account_programs_keep_components_and_gestures_separate_and_missing_never_falls_back() {
    let mut magic = Magic::new(16);
    magic.random = Some(Arc::new(RandomRoot::new([7; 32], 1).unwrap()));
    let a = EntityId(1);
    let b = EntityId(2);
    let missing = EntityId(3);
    for actor in [a, b, missing] {
        magic.casters.insert(actor, caster());
    }
    magic
        .register_actor_program(a, program(1001, 0x13000132), vec![])
        .unwrap();
    assert_eq!(
        magic.prepared_for_actor(missing, 100).unwrap_err(),
        CastRejection::MissingAssets
    );
    assert!(magic.required_components(missing, 100).is_none());
    let mut world = admitted_world();
    let policy = Combat::new(8);
    let fellowships = crate::fellowships::Fellowships::new(8);
    for origin in [
        CastOrigin::Player(ActionContext {
            actor: missing,
            account: bace_types::AccountId(3),
            session: bace_gameplay_api::SessionId(3),
            sequence: 1,
        }),
        CastOrigin::Monster {
            actor: missing,
            event: 1,
        },
        CastOrigin::Emote {
            actor: missing,
            event: 1,
            instant: false,
        },
    ] {
        assert_eq!(
            magic.apply_origin(
                origin,
                CastRequest::Untargeted { spell: 100 },
                &mut world,
                0.0,
                &policy,
                &fellowships,
                None,
            ),
            Err(CastRejection::MissingAssets)
        );
    }
    assert!(magic.attempts.is_empty());
    assert!(magic.server_sequences.is_empty());

    magic
        .register_actor_program(b, program(1002, 0x13000133), vec![])
        .unwrap();
    assert_eq!(
        magic.prepared_for_actor(a, 100).unwrap().components,
        vec![(1001, 1)]
    );
    assert_eq!(
        magic.prepared_for_actor(b, 100).unwrap().components,
        vec![(1002, 1)]
    );
    assert_eq!(
        magic.prepared_for_actor(a, 100).unwrap().gestures[0]
            .gesture
            .motion,
        0x13000132
    );
    assert_eq!(
        magic.prepared_for_actor(b, 100).unwrap().gestures[0]
            .gesture
            .motion,
        0x13000133
    );
    let before = magic.prepared_for_actor(a, 100).unwrap();
    let mut wrong = program(9000, 0x13000133);
    wrong.flags = 8;
    assert_eq!(
        magic.register_actor_program(a, wrong, vec![]),
        Err(CastRejection::InvalidState)
    );
    assert!(Arc::ptr_eq(
        &before,
        &magic.prepared_for_actor(a, 100).unwrap()
    ));
    let mut invalid = program(9000, 0x13000133);
    invalid.spell.spell.id = 101;
    invalid.spell.gestures[0].motion_chain = None;
    assert_eq!(
        magic.register_actor_program(a, invalid, vec![]),
        Err(CastRejection::MissingAssets)
    );
    assert!(!magic.spells.contains_key(&101));
    assert!(!magic.actor_components.contains_key(&(a, 101)));
}

#[test]
fn instant_server_origins_use_effect_metadata_without_another_actors_formula() {
    // GDLE SpellcastingManager::CastSpellInstant resolves and launches the effect
    // without BeginNextMotion or mana expenditure. Actor formula isolation is a
    // noninstant-casting requirement; an absent RNG must not mask this distinction.
    let mut magic = Magic::new(16);
    magic.random = Some(Arc::new(RandomRoot::new([7; 32], 1).unwrap()));
    for actor in [EntityId(1), EntityId(3)] {
        magic.casters.insert(actor, caster());
    }
    magic
        .register_actor_program(EntityId(1), program(1001, 0x13000132), vec![])
        .unwrap();
    let mut world = admitted_world();
    let policy = Combat::new(8);
    let fellowships = crate::fellowships::Fellowships::new(8);
    for origin in [
        CastOrigin::Staff {
            actor: EntityId(3),
            event: 1,
        },
        CastOrigin::Emote {
            actor: EntityId(3),
            event: 1,
            instant: true,
        },
    ] {
        assert!(matches!(
            magic.apply_origin(
                origin,
                CastRequest::Targeted {
                    target: EntityId(3),
                    spell: 100
                },
                &mut world,
                0.0,
                &policy,
                &fellowships,
                None,
            ),
            Ok(CastChange::Started { .. })
        ));
    }
    assert!(magic.attempts.is_empty());
    assert!(magic.required_components(EntityId(3), 100).is_none());
    assert_eq!(
        world.vital(EntityId(3), EntityVital::Mana).unwrap().current,
        100
    );
}
