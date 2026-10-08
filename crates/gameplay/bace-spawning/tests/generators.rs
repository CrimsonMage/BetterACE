mod common;
use bace_gameplay_api::*;
use bace_random::RandomRoot;
use bace_spawning::*;
use bace_types::EntityId;
use common::*;
use std::sync::Arc;
fn machine(def: GeneratorDefinition) -> GeneratorMachine {
    GeneratorMachine::new(Arc::new(def), clock(0), GeneratorLimits::default()).unwrap()
}
fn root() -> RandomRoot {
    RandomRoot::new([8; 32], 1).unwrap()
}
fn complete(m: &mut GeneratorMachine, entity: u32) {
    let key = m.next_spawn().unwrap().key;
    m.acknowledge(GeneratorSpawnReceipt {
        key,
        result: GeneratorSpawnResult::Completed {
            members: vec![GeneratorSpawnMember {
                entity: EntityId(entity),
                contribution: 1,
            }],
            materialized: true,
            failed_placements: 0,
        },
    })
    .unwrap();
}
#[test]
fn future_content_revision_preserves_old_queue_and_member_ownership() {
    let mut old = definition();
    old.maximum_count = 2;
    old.profiles[0].max_create = 2;
    let mut m = machine(old.clone());
    m.advance(clock(0), &root()).unwrap();
    let queued = m.next_spawn().unwrap().clone();
    let mut next = old.clone();
    next.identity.content_revision = 2;
    next.identity.random_identity = [2; 16];
    m.refresh_future_content(Arc::new(next.clone())).unwrap();
    assert_eq!(m.next_spawn(), Some(&queued));
    assert!(m.accepts_identity(old.identity));
    assert!(m.accepts_identity(next.identity));
    complete(&mut m, 40);
    assert!(m.owns_member(EntityId(40)));
    m.generate_now(clock(1), &root()).unwrap();
    assert_eq!(m.next_spawn().unwrap().key.generator, next.identity);
    complete(&mut m, 41);
    let retired = m.reset(clock(2)).unwrap();
    assert!(retired.effects.iter().any(|effect| matches!(effect,
        GeneratorLifecycleEffect::DestroyMember { generator, member, .. }
            if *generator == old.identity && member.entity == EntityId(40))));
    assert!(retired.effects.iter().any(|effect| matches!(effect,
        GeneratorLifecycleEffect::DestroyMember { generator, member, .. }
            if *generator == next.identity && member.entity == EntityId(41))));
    let mut changed_profiles = next;
    changed_profiles.identity.content_revision = 3;
    changed_profiles.identity.random_identity = [3; 16];
    changed_profiles.profiles[0].weenie_class_id = 11;
    assert_eq!(
        m.refresh_future_content(Arc::new(changed_profiles)),
        Err(GeneratorError::Definition)
    );
    assert!(!m.owns_member(EntityId(40)));
}
#[test]
fn staff_regenerate_retires_and_selects_in_one_transition() {
    let mut machine = machine(definition());
    machine.advance(clock(0), &root()).unwrap();
    complete(&mut machine, 2);
    machine.advance(clock(5), &root()).unwrap();
    assert!(machine.owns_member(EntityId(2)));
    let before = machine.clone();
    assert_eq!(
        machine.regenerate(clock(4), &root()),
        Err(GeneratorError::Time),
    );
    assert_eq!(machine.current_create(), before.current_create());
    assert!(machine.owns_member(EntityId(2)));
    let transition = machine.regenerate(clock(6), &root()).unwrap();
    assert!(!transition.effects.is_empty());
    assert_eq!(transition.enqueued, 1);
    assert_eq!(machine.current_create(), 1);
    assert_eq!(machine.pending(), 1);
    assert!(!machine.owns_member(EntityId(2)));
}
#[test]
fn cold_contained_member_occupies_profile_before_first_population_tick() {
    let mut definition = definition();
    definition.profiles[0].where_create = 8;
    let mut machine = machine(definition);
    machine
        .adopt_restored_contained_member(0, EntityId(0x8000_0001), 10)
        .unwrap();
    assert_eq!(machine.current_create(), 1);
    assert!(machine.owns_member(EntityId(0x8000_0001)));
    assert_eq!(
        machine.adopt_restored_contained_member(0, EntityId(0x8000_0001), 10),
        Err(GeneratorError::Receipt)
    );
    assert_eq!(
        machine.adopt_restored_contained_member(0, EntityId(0x8000_0002), 11),
        Err(GeneratorError::Receipt)
    );
    assert_eq!(machine.advance(clock(0), &root()).unwrap().enqueued, 0);
}
#[test]
fn immutable_retry_and_stale_generation_do_not_consume_work() {
    let mut m = machine(definition());
    m.advance(clock(0), &root()).unwrap();
    let intent = m.next_spawn().unwrap().clone();
    m.acknowledge(GeneratorSpawnReceipt {
        key: intent.key,
        result: GeneratorSpawnResult::Blocked(GeneratorBlockedReason::Geometry),
    })
    .unwrap();
    m.advance(clock(30), &root()).unwrap();
    assert_eq!(m.next_spawn(), Some(&intent));
    let mut stale = intent.key;
    stale.generator.incarnation += 1;
    assert_eq!(
        m.acknowledge(GeneratorSpawnReceipt {
            key: stale,
            result: GeneratorSpawnResult::Invalid(GeneratorFailure::Placement)
        }),
        Err(GeneratorError::StaleReceipt)
    );
    complete(&mut m, 2);
    assert_eq!(m.current_create(), 1);
    assert_eq!(m.pending(), 0);
}
#[test]
fn pickup_maps_destruction_and_cooldown_is_strict() {
    let mut d = definition();
    d.profiles[0].delay = Some(2.);
    let mut m = machine(d);
    m.advance(clock(0), &root()).unwrap();
    complete(&mut m, 2);
    m.notify(EntityId(2), GeneratorNotification::PickUp, clock(1))
        .unwrap();
    m.advance(clock(61), &root()).unwrap();
    assert_eq!(m.pending(), 0);
    m.advance(clock(91), &root()).unwrap();
    assert_eq!(m.pending(), 1);
}
#[test]
fn undefined_notifications_remap_but_composite_and_death_are_not_invented() {
    for when in [0, 1, 2, 3, 4] {
        let mut d = definition();
        d.profiles[0].when_create = when;
        let mut m = machine(d);
        m.advance(clock(0), &root()).unwrap();
        complete(&mut m, 2);
        m.notify(EntityId(2), GeneratorNotification::Destruction, clock(1))
            .unwrap();
        assert_eq!(m.current_create(), usize::from(when == 3 || when == 4));
    }
}
#[test]
fn initial_failed_placement_suppresses_slot_until_reset() {
    let mut m = machine(definition());
    m.advance(clock(0), &root()).unwrap();
    let key = m.next_spawn().unwrap().key;
    let out = m
        .acknowledge(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: vec![],
                materialized: true,
                failed_placements: 1,
            },
        })
        .unwrap();
    assert_eq!(
        out.effects,
        vec![GeneratorLifecycleEffect::SuppressedInitial { key, count: 1 }]
    );
    m.advance(clock(300), &root()).unwrap();
    assert_eq!(m.pending(), 0);
    m.reset(clock(301)).unwrap();
    m.advance(clock(330), &root()).unwrap();
    assert!(!m.next_spawn().unwrap().first_spawn);
}
#[test]
fn treasure_occupancy_collapses_members_but_cleanup_preserves_every_identity() {
    let mut d = definition();
    d.profiles[0].where_create = 64;
    let mut m = machine(d);
    m.advance(clock(0), &root()).unwrap();
    let key = m.next_spawn().unwrap().key;
    m.acknowledge(GeneratorSpawnReceipt {
        key,
        result: GeneratorSpawnResult::Completed {
            members: (2..12)
                .map(|id| GeneratorSpawnMember {
                    entity: EntityId(id),
                    contribution: 1,
                })
                .collect(),
            materialized: true,
            failed_placements: 0,
        },
    })
    .unwrap();
    assert_eq!(m.current_create(), 1);
    m.notify(EntityId(2), GeneratorNotification::PickUp, clock(1))
        .unwrap();
    assert_eq!(m.current_create(), 1);
    assert_eq!(m.reset(clock(2)).unwrap().effects.len(), 9);
    assert_eq!(m.current_create(), 0);
}
#[test]
fn staged_day_status_waits_second_heartbeat_but_initial_checks_twice() {
    let mut d = definition();
    d.time_type = GeneratorTimeType::Night;
    let mut m = machine(d);
    m.advance(clock(0), &root()).unwrap();
    assert!(m.disabled());
    assert_eq!(m.pending(), 0);
    let mut c = clock(150);
    c.is_day = false;
    m.advance(c, &root()).unwrap();
    assert!(m.disabled());
    c.tick = 300;
    m.advance(c, &root()).unwrap();
    assert!(!m.disabled());
    assert_eq!(m.pending(), 1);
}
#[test]
fn initial_timestamp_zero_skips_delay_and_nonzero_waits() {
    for (timestamp, expected) in [(0., 1), (1., 0)] {
        let mut d = definition();
        d.initial_delay = 10.;
        d.regeneration_timestamp = timestamp;
        let mut m = machine(d);
        m.advance(clock(0), &root()).unwrap();
        assert_eq!(m.pending(), expected);
        m.advance(clock(300), &root()).unwrap();
        assert_eq!(m.pending(), 1);
    }
}
#[test]
fn capacity_failure_does_not_consume_randomness_or_partially_enqueue() {
    let mut d = definition();
    d.maximum_count = 3;
    d.initial_count = 3;
    d.profiles[0].init_create = 3;
    d.profiles[0].max_create = 3;
    let mut m = GeneratorMachine::new(
        Arc::new(d),
        clock(0),
        GeneratorLimits {
            pending: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(m.advance(clock(0), &root()), Err(GeneratorError::Capacity));
    assert_eq!(m.pending(), 0);
    assert_eq!(m.current_create(), 0);
    assert_eq!(m.advance(clock(0), &root()), Err(GeneratorError::Capacity));
}
#[test]
fn zero_probability_loop_is_bounded_at_source_1001_attempts() {
    let mut d = definition();
    d.profiles[0].probability = 0.;
    let mut m = machine(d);
    let out = m.advance(clock(0), &root()).unwrap();
    assert_eq!(out.selection_attempts, 1001);
    assert!(out.exhausted_initial_loop);
    assert_eq!(m.pending(), 0);
}
#[test]
fn retained_shop_stock_contributions_are_summed() {
    let mut d = definition();
    d.profiles[0].max_create = -1;
    d.initial_count = 2;
    d.maximum_count = 2;
    let mut m = machine(d);
    m.advance(clock(0), &root()).unwrap();
    complete(&mut m, 2);
    complete(&mut m, 2);
    assert_eq!(m.current_create(), 1);
    assert!(matches!(
        &m.destroy(clock(1)).unwrap().effects[..],
        [GeneratorLifecycleEffect::DestroyMember {
            member: GeneratorSpawnMember {
                contribution: 2,
                ..
            },
            ..
        }]
    ));
}
#[test]
fn nothing_directive_retains_members_and_queue() {
    let mut d = definition();
    d.destruction = GeneratorDestruction::Nothing;
    let mut m = machine(d);
    m.advance(clock(0), &root()).unwrap();
    let before = m.next_spawn().cloned();
    assert!(m.destroy(clock(1)).unwrap().effects.is_empty());
    assert_eq!(m.next_spawn(), before.as_ref());
}
#[test]
fn vendor_shop_default_filters_even_mixed_destination_flags() {
    let mut d = definition();
    d.kind = GeneratorKind::Vendor;
    d.profiles[0].where_create = 36;
    let mut m = machine(d.clone());
    assert_eq!(m.advance(clock(0), &root()).unwrap().enqueued, 0);
    d.vendor_shop_uses_generator = true;
    assert_eq!(machine(d).advance(clock(0), &root()).unwrap().enqueued, 1);
}
#[test]
fn terminal_invalid_is_visible_and_not_retried() {
    let mut m = machine(definition());
    m.advance(clock(0), &root()).unwrap();
    let key = m.next_spawn().unwrap().key;
    assert_eq!(
        m.acknowledge(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Invalid(GeneratorFailure::MissingTemplate)
        })
        .unwrap()
        .effects,
        vec![GeneratorLifecycleEffect::Invalidated {
            key,
            failure: GeneratorFailure::MissingTemplate
        }]
    );
    assert_eq!(m.advance(clock(900), &root()).unwrap().enqueued, 0);
}
#[test]
fn malformed_receipt_does_not_mutate_registry() {
    let mut m = machine(definition());
    m.advance(clock(0), &root()).unwrap();
    let key = m.next_spawn().unwrap().key;
    assert_eq!(
        m.acknowledge(GeneratorSpawnReceipt {
            key,
            result: GeneratorSpawnResult::Completed {
                members: vec![GeneratorSpawnMember {
                    entity: EntityId(2),
                    contribution: 0
                }],
                materialized: true,
                failed_placements: 0
            }
        }),
        Err(GeneratorError::Receipt)
    );
    assert_eq!(m.pending(), 1);
    complete(&mut m, 2);
}
#[test]
fn compiled_source_status_staging_including_reversal_and_missing_event() {
    let mut active: Option<GeneratorMachine> = None;
    let mut previous = "";
    for line in include_str!("ace_status.tsv").lines() {
        let f: Vec<_> = line.split('|').collect();
        let mode = f[0];
        let step = f[1].parse::<usize>().unwrap();
        if mode != previous {
            let mut d = definition();
            d.time_type = match mode {
                "Day" => GeneratorTimeType::Day,
                "Night" => GeneratorTimeType::Night,
                "RealTime" => GeneratorTimeType::RealTime,
                "Event" => GeneratorTimeType::Event,
                "Defined" => GeneratorTimeType::Defined,
                _ => panic!(),
            };
            d.event = Some("event".into());
            d.start_time = 10;
            d.end_time = 20;
            active = Some(machine(d));
            previous = mode;
        }
        let mut clock = clock(step as u64 * 150);
        clock.unix_seconds = [0, 10, 20, 21, 19, 21, 21][step];
        clock.is_day = [true, false, true, false, false, true, true][step];
        clock.event = if step == 4 {
            GeneratorEventState::Missing
        } else {
            GeneratorEventState::Available {
                enabled: step != 1,
                started: step != 2 && step != 5,
            }
        };
        let m = active.as_mut().unwrap();
        m.advance(clock, &root()).unwrap();
        assert_eq!(m.disabled(), f[2] == "True", "{line}");
    }
}
#[test]
fn random_events_are_isolated_by_entity_incarnation_and_content_revision() {
    let d = definition();
    let mut original = machine(d.clone());
    original.advance(clock(0), &root()).unwrap();
    let first = original.next_spawn().unwrap().random_identity;
    for variant in 0..3 {
        let mut changed = d.clone();
        match variant {
            0 => changed.identity.entity.0 += 1,
            1 => changed.identity.incarnation += 1,
            _ => changed.identity.content_revision += 1,
        }
        let mut machine = machine(changed);
        machine.advance(clock(0), &root()).unwrap();
        assert_ne!(machine.next_spawn().unwrap().random_identity, first);
    }
}
