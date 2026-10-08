use super::*;
pub(super) fn fixture() -> (Magic, World, bace_content::WeenieV1) {
    let mut world = super::periodic_tests::world(false, 40);
    let cell = CellId(1);
    let body = bace_physics::Body::spawn(
        world.scene(cell).unwrap(),
        Vec3::new(3., 0., 0.5),
        0.5,
        bace_motion::Capabilities {
            speed: 0.,
            jump_impulse: 0.,
        },
    )
    .unwrap();
    world
        .insert(bace_entity::Actor {
            id: EntityId(2),
            cell,
            body,
        })
        .unwrap();
    let source = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 500,
        class_name: "instant-object-fixture".into(),
        weenie_type: 19,
        last_modified: None,
        properties: bace_content::SparseProperties {
            ints: vec![bace_content::Property {
                id: 106,
                value: 123,
            }],
            ..Default::default()
        },
    };
    let mut magic = Magic::new(32);
    magic.random = Some(Arc::new(RandomRoot::new([7; 32], 1).unwrap()));
    magic
        .register_object_caster(EntityId(2), &source, &world)
        .unwrap();
    magic
        .register_registry(EntityId(1), EnchantmentRegistry::new(16).unwrap(), true, 0.)
        .unwrap();
    magic
        .register_damage_profile(EntityId(1), bace_magic::MagicDamageProfile::neutral(true))
        .unwrap();
    (magic, world, source)
}
fn definition(effect: SpellEffect) -> PreparedMagicDefinition {
    PreparedMagicDefinition {
        spell: PreparedMagicSpell {
            spell: PreparedSpell {
                id: 100,
                school: bace_magic::MagicSchool::Life,
                power: 1,
                base_mana: 999,
                range_constant: 30.,
                range_per_skill: 0.,
                harmful: false,
                resistable: false,
                effect,
            },
            gestures: vec![],
            components: vec![],
            component_modifiers: vec![],
            component_loss: 0.,
            fast_resistable_pk_spell: false,
        },
        metadata: None,
        formula_level: 1,
        category: 0,
        flags: 4,
        target_mask: 16,
    }
}
#[test]
fn instant_world_object_uses_authored_spellcraft_without_creature_or_vital_owner() {
    let (mut magic, mut world, _) = fixture();
    let spell = definition(SpellEffect::Boost {
        vital: Vital::Health,
        minimum: 10,
        maximum: 10,
    });
    assert_eq!(
        magic
            .effective_cast_skill(
                CastOrigin::Emote {
                    actor: EntityId(2),
                    event: 1,
                    instant: true
                },
                &spell.spell.spell,
                &world
            )
            .unwrap(),
        123
    );
    magic.register_instant_definition(spell, vec![]).unwrap();
    let origin = CastOrigin::Emote {
        actor: EntityId(2),
        event: 1,
        instant: true,
    };
    magic
        .apply_origin(
            origin,
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
            &mut world,
            0.,
            &Combat::new(32),
            &crate::fellowships::Fellowships::new(32),
            None,
        )
        .unwrap();
    assert_eq!(
        world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        50
    );
    assert!(world.combatant(EntityId(2)).is_none());
    assert!(world.vital(EntityId(2), EntityVital::Mana).is_err());
    assert!(matches!(
        magic.take_server_outcome().unwrap().result,
        Ok(CastChange::Completed { .. })
    ));
}
#[test]
fn object_vital_transfer_and_animated_cast_reject_without_mutation() {
    let (mut magic, mut world, _) = fixture();
    magic
        .register_instant_definition(
            definition(SpellEffect::Transfer {
                source: Vital::Health,
                destination: Vital::Health,
                source_is_caster: true,
                destination_is_caster: false,
                proportion: 0.5,
                loss: 0.,
                cap: 10,
            }),
            vec![],
        )
        .unwrap();
    for instant in [true, false] {
        let result = magic.apply_origin(
            CastOrigin::Emote {
                actor: EntityId(2),
                event: 1,
                instant,
            },
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
            &mut world,
            0.,
            &Combat::new(32),
            &crate::fellowships::Fellowships::new(32),
            None,
        );
        assert_eq!(result, Err(CastRejection::MissingAssets));
    }
    assert_eq!(
        world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        40
    );
    assert!(magic.events.is_empty());
}
#[test]
fn resistance_names_freeze_before_source_rename_and_reject_oversized_names() {
    use bace_entity::{EntityProperties, PropertyFamily as F, PropertyValue as V};
    let (_, mut world, _) = fixture();
    for (actor, name) in [(EntityId(1), "Target"), (EntityId(2), "Trap")] {
        let properties =
            EntityProperties::restore_snapshot(0, vec![(F::String, 1, V::String(name.into()))])
                .unwrap();
        world.register_properties(actor, properties).unwrap();
    }
    let notice = super::resist_notices::capture(&world, EntityId(2), EntityId(1), None).unwrap();
    let source = world.properties_mut(EntityId(2)).unwrap();
    let change = source
        .propose(F::String, 1, Some(V::String("Renamed".into())))
        .unwrap();
    source.adopt(change).unwrap();
    assert_eq!(&*notice.source_name, "Trap");
    assert_eq!(&*notice.target_name, "Target");
    assert!(
        notice.source.is_none() && notice.target.is_none(),
        "no fabricated private recipient"
    );
    let change = source
        .propose(F::String, 1, Some(V::String("x".repeat(1025))))
        .unwrap();
    source.adopt(change).unwrap();
    assert!(super::resist_notices::capture(&world, EntityId(2), EntityId(1), None).is_err());
}
#[test]
fn instant_object_portal_retains_exact_owner_completion() {
    let (mut magic, mut world, _) = fixture();
    magic
        .register_instant_definition(
            definition(SpellEffect::Portal(bace_magic::PortalEffect::Sending {
                cell: 1,
                position: Vec3::new(1., 2., 0.5),
                heading: 0.,
            })),
            vec![],
        )
        .unwrap();
    let origin = CastOrigin::Emote {
        actor: EntityId(2),
        event: 9,
        instant: true,
    };
    let change = magic
        .apply_origin(
            origin,
            CastRequest::Targeted {
                target: EntityId(1),
                spell: 100,
            },
            &mut world,
            0.,
            &Combat::new(32),
            &crate::fellowships::Fellowships::new(32),
            None,
        )
        .unwrap();
    let CastChange::Started { cast } = change else {
        panic!("retained source cast");
    };
    assert!(magic.take_server_outcome().is_none());
    assert_eq!(magic.pending_portal_casts()[0].mana_cost, 0);
    assert_eq!(magic.portal_origin(EntityId(2), cast), Some(origin));
    magic.bind_portal(EntityId(2), cast, 11).unwrap();
    assert!(magic.pending_portal_casts().is_empty());
    assert!(
        magic
            .finish_portal_service(EntityId(2), cast + 1, Ok(()), &mut world, 1.)
            .is_err()
    );
    magic
        .finish_portal_service(EntityId(2), cast, Ok(()), &mut world, 1.)
        .unwrap();
    assert!(magic.instant_continuations.is_empty());
    assert_eq!(magic.take_server_outcome().unwrap().origin, origin);
    assert!(world.combatant(EntityId(2)).is_none());
}
#[test]
fn owned_instant_queue_allows_nested_order_but_rejects_exact_request_replay() {
    let (mut magic, mut world, _) = fixture();
    magic
        .register_instant_definition(
            definition(SpellEffect::Boost {
                vital: Vital::Health,
                minimum: 10,
                maximum: 10,
            }),
            vec![],
        )
        .unwrap();
    // Queue metadata is the proof here; no physical hit or invented inventory
    // item is needed to exercise the shared instant continuation dispatcher.
    for event in [1, 2] {
        magic.item_procs.push_back(MagicItemProcRequest {
            origin: CastOrigin::PhysicalProc {
                actor: EntityId(2),
                event,
            },
            target: EntityId(1),
            spell: 100,
            parent: None,
            depth: 0,
            skill_override: None,
            admitted: false,
        });
    }
    for event in [2, 1] {
        let origin = CastOrigin::PhysicalProc {
            actor: EntityId(2),
            event,
        };
        assert!(
            magic
                .apply_origin(
                    origin,
                    CastRequest::Targeted {
                        target: EntityId(1),
                        spell: 100
                    },
                    &mut world,
                    0.,
                    &Combat::new(32),
                    &crate::fellowships::Fellowships::new(32),
                    None
                )
                .is_ok()
        );
        assert!(magic.item_proc_request(origin).unwrap().admitted);
        assert_eq!(
            magic.apply_origin(
                origin,
                CastRequest::Targeted {
                    target: EntityId(1),
                    spell: 100
                },
                &mut world,
                0.,
                &Combat::new(32),
                &crate::fellowships::Fellowships::new(32),
                None
            ),
            Err(CastRejection::StaleSequence)
        );
    }
    assert_eq!(
        world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        60
    );
    assert_eq!(magic.server_sequences[&(EntityId(2), 5)], 2);
}
