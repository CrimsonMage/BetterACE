use super::*;

fn registry(duration: f64) -> EnchantmentRegistry {
    EnchantmentRegistry::restore(
        8,
        20,
        vec![EnchantmentEntry {
            spell: 123,
            caster: 100,
            school: bace_magic::MagicSchool::Creature,
            spec: bace_magic::EnchantmentSpec {
                category: 23,
                power: 100,
                duration,
                layer: 3,
                stat_type: 0x1000,
                stat_key: 7,
                value: 12.5,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata::default(),
        }],
    )
    .unwrap()
}

#[test]
fn full_output_retains_due_heartbeat_and_serves_later_targets() {
    let mut magic = Magic::new(1);
    magic
        .register_registry(EntityId(1), registry(5.0), true, 0.0)
        .unwrap();
    magic
        .register_registry(EntityId(2), registry(5.0), true, 0.0)
        .unwrap();
    magic.events.push_back(MagicEvent::ProjectileRemoved {
        tick: 0,
        actor: EntityId(99),
    });
    magic.heartbeat(5.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        0.0
    );
    magic.take_event();
    magic.heartbeat(10.0);
    assert!(matches!(
        magic.take_event(),
        Some(MagicEvent::EnchantmentsRemoved {
            actor: EntityId(1),
            ..
        })
    ));
    magic.heartbeat(10.0);
    assert!(matches!(
        magic.take_event(),
        Some(MagicEvent::EnchantmentsRemoved {
            actor: EntityId(2),
            ..
        })
    ));
    assert!(magic.registries.values().all(|r| r.entries().is_empty()));
    assert!(magic.take_event().is_none());
}

#[test]
fn nonexpiring_clock_progress_does_not_need_output_capacity() {
    let mut magic = Magic::new(1);
    magic
        .register_registry(EntityId(1), registry(-1.0), true, 0.0)
        .unwrap();
    magic.events.push_back(MagicEvent::ProjectileRemoved {
        tick: 0,
        actor: EntityId(99),
    });
    magic.heartbeat(15.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -15.0
    );
    assert_eq!(magic.pending_events(), 1);
}

#[test]
fn inactive_items_do_not_age_and_reactivation_uses_server_time() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(60.0), false, 0.0)
        .unwrap();
    magic.heartbeat(1000.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        0.0
    );
    magic
        .set_registry_active(EntityId(1), true, 1000.0)
        .unwrap();
    magic.heartbeat(1005.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -5.0
    );
}

#[test]
fn reservation_retains_timer_debt_until_commit_or_rollback_release() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(60.0), true, 0.0)
        .unwrap();
    magic.reserve_registry(EntityId(1), true, 4.0).unwrap();
    magic.heartbeat(14.0);
    assert_eq!(magic.registry(EntityId(1)).unwrap().revision(), 20);
    magic.reserve_registry(EntityId(1), false, 14.0).unwrap();
    magic.heartbeat(14.0);
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -10.0
    );
    assert_eq!(magic.registry(EntityId(1)).unwrap().revision(), 21);
}

#[test]
fn owner_transfer_returns_registry_and_cannot_leave_offline_timer_owner() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(60.0), true, 0.0)
        .unwrap();
    let owned = magic.take_registry(EntityId(1), 10.0).unwrap();
    assert_eq!(owned.entries()[0].start_time, -10.0);
    assert!(!magic.has_state());
    magic.heartbeat(1000.0);
    magic
        .register_registry(EntityId(1), owned, true, 1000.0)
        .unwrap();
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        -10.0
    );
}

#[test]
fn failed_admission_returns_owned_registry_and_expiry_output_blocks_transfer() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(5.0), true, 0.0)
        .unwrap();
    let (error, recovered) = magic
        .register_registry(EntityId(1), registry(60.0), true, 0.0)
        .unwrap_err();
    assert_eq!(error, CastRejection::InvalidState);
    assert_eq!(recovered.entries().len(), 1);
    assert!(matches!(
        magic.take_registry(EntityId(1), 5.0),
        Err(CastRejection::Busy)
    ));
    assert!(matches!(
        magic.take_event(),
        Some(MagicEvent::EnchantmentsRemoved { .. })
    ));
    assert!(
        magic
            .take_registry(EntityId(1), 5.0)
            .unwrap()
            .entries()
            .is_empty()
    );
}

#[test]
fn registry_failure_is_observable_and_state_retained() {
    let mut magic = Magic::new(8);
    let rows = registry(60.0).into_entries();
    magic
        .register_registry(
            EntityId(1),
            EnchantmentRegistry::restore(8, u64::MAX, rows).unwrap(),
            true,
            0.0,
        )
        .unwrap();
    magic.heartbeat(5.0);
    assert_eq!(
        magic.registry_failure(EntityId(1)),
        Some(RegistryError::RevisionExhausted)
    );
    assert_eq!(
        magic.registry(EntityId(1)).unwrap().entries()[0].start_time,
        0.0
    );
    assert!(!magic.can_accept());
}

#[test]
fn projectile_impact_waits_for_reserved_registry_without_consuming_it() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(60.0), true, 0.0)
        .unwrap();
    magic.reserve_registry(EntityId(1), true, 0.0).unwrap();
    let spell = PreparedMagicSpell {
        spell: PreparedSpell {
            id: 123,
            school: bace_magic::MagicSchool::Creature,
            power: 100,
            base_mana: 1,
            range_constant: 10.0,
            range_per_skill: 0.0,
            harmful: true,
            resistable: true,
            effect: SpellEffect::Enchantment(registry(60.0).entries()[0].spec.clone()),
        },
        gestures: vec![],
        components: vec![],
        component_modifiers: vec![],
        component_loss: 0.0,
        fast_resistable_pk_spell: false,
    };
    magic.flying.insert(
        EntityId(90),
        Flying {
            proc_parent: None,
            pending_damage: None,
            launch_wand: None,
            source: EntityId(2),
            target: Some(EntityId(1)),
            spell: Arc::new(spell),
            random: RandomRoot::new([1; 32], 1)
                .unwrap()
                .event_stream([2; 16], Domain::Magic)
                .unwrap(),
            lifetime: bace_magic::SpellProjectileLifetime::new(0.0).unwrap(),
            initial_cast: (CellId(1), Vec3::ZERO),
            maximum_range: 75.0,
            cast_skill: 100,
            resting: false,
            pending_impact: Some(Some(1)),
            life_damage: None,
            credited_owner: EntityId(2),
        },
    );
    let mut world = super::specialization_tests::world();
    let source_body = bace_physics::Body::spawn_oriented(
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
            body: source_body,
        })
        .unwrap();
    let mut profile = world.combatant(EntityId(1)).unwrap().profile().clone();
    profile.player = false;
    world
        .register_combatant(EntityId(2), bace_entity::Combatant::new(profile).unwrap())
        .unwrap();
    magic.step_projectiles(&mut world, 1.0, &Combat::new(64));
    assert_eq!(magic.flying[&EntityId(90)].pending_impact, Some(Some(1)));
    assert_eq!(magic.registry(EntityId(1)).unwrap().revision(), 20);
    assert!(magic.events.is_empty());
    magic.reserve_registry(EntityId(1), false, 1.0).unwrap();
    magic.step_projectiles(&mut world, 1.0, &Combat::new(64));
    assert_eq!(magic.flying[&EntityId(90)].pending_impact, None);
    let mut lifetime = magic.flying[&EntityId(90)].lifetime;
    assert!(lifetime.exploded());
    assert_eq!(
        lifetime.tick(1.499, true, 1.0, 75.0).unwrap(),
        bace_magic::ProjectileLifeAction::None
    );
    assert_eq!(
        lifetime.tick(1.5, true, 1.0, 75.0).unwrap(),
        bace_magic::ProjectileLifeAction::Destroy
    );
}

#[test]
fn durable_item_retirement_requires_reservation_and_preserves_pending_output() {
    let mut magic = Magic::new(8);
    magic
        .register_registry(EntityId(1), registry(60.0), true, 0.0)
        .unwrap();
    assert_eq!(
        magic.retire_item_registry(EntityId(1), 0.0),
        Err(CastRejection::Busy)
    );
    magic.reserve_registry(EntityId(1), true, 0.0).unwrap();
    magic.events.push_back(MagicEvent::Enchantment {
        actor: EntityId(1),
        entry: magic.registry(EntityId(1)).unwrap().entries()[0].clone(),
    });
    magic.can_retire_item_registry(EntityId(1), 0.0).unwrap();
    magic.retire_item_registry(EntityId(1), 0.0).unwrap();
    assert!(magic.registry(EntityId(1)).is_none());
    assert!(!magic.registry_clocks.contains_key(&EntityId(1)));
    magic.heartbeat(10.0);
    assert_eq!(magic.events.len(), 1);
    assert!(matches!(
        magic.take_event(),
        Some(MagicEvent::Enchantment {
            actor: EntityId(1),
            ..
        })
    ));
}
