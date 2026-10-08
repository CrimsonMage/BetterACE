use super::*;
#[path = "motion/refresh.rs"]
mod refresh;
use bace_motion::{
    ExecutionClip, MotionHook, MotionHookPayload, MotionPhysics, PreparedMotionChain, RootFrame,
    SourceMotionState, SourceMotionTransition,
};

fn chain(motion: u32, speed: f32, frames: u32, hook: u32) -> Arc<PreparedMotionChain> {
    chain_in_style(motion, speed, frames, hook, 0x80000040)
}
fn chain_in_style(
    motion: u32,
    speed: f32,
    frames: u32,
    hook: u32,
    style: u32,
) -> Arc<PreparedMotionChain> {
    let ready = SourceMotionState {
        style,
        substate: 0x41000003,
        speed: 1.0,
    };
    let idle = ExecutionClip {
        animation: 0x03000002,
        frame_count: 2,
        low: 0,
        high: 1,
        framerate: 30.0,
        frames: vec![RootFrame::default(); 2].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![idle.clone()], 0, 0, physics, ready.substate, 1.0)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    Arc::new(
        PreparedMotionChain::prepare(
            vec![
                ExecutionClip {
                    animation: 0x03000001,
                    frame_count: frames,
                    low: 0,
                    high: frames as i32 - 1,
                    framerate: 30.0,
                    frames: vec![RootFrame::default(); frames as usize].into(),
                    hooks: vec![MotionHook {
                        frame: hook,
                        kind: 1,
                        direction: 1,
                        payload: MotionHookPayload::Attack {
                            part: 0,
                            left: [-1.0, 1.0],
                            right: [1.0, 1.0],
                            radius: 2.0,
                            height: 2.0,
                        },
                    }]
                    .into(),
                },
                idle,
            ],
            1,
            1,
            physics,
            motion,
            speed,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: ready,
            after: ready,
            continues_cycle: false,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    )
}
fn connected(distance: f32) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-20.0, -20.0, -1.0), Vec3::new(20.0, 20.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn_oriented(
            world.scene(cell).unwrap(),
            Vec3::new(if id == 1 { 0.0 } else { distance }, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
            -std::f32::consts::FRAC_PI_2,
            3.0,
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 1000,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap()
                .with_resources(
                    Some(bace_entity::VitalPool {
                        current: 1000,
                        maximum: 1000,
                    }),
                    Some(bace_entity::VitalPool {
                        current: 1000,
                        maximum: 1000,
                    }),
                )
                .unwrap(),
            )
            .unwrap();
    }
    let mut k = Kernel::with_gameplay_limits(world, 64, 4, 64).unwrap();
    k.register_character(binding(), character()).unwrap();
    k.configure_combat_random_shared(
        Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
        1,
    )
    .unwrap();
    for id in 1..=2 {
        k.register_inventory_container(bace_inventory::InventoryContainer {
            id: EntityId(id),
            revision: 1,
            root_owner: Some(EntityId(id)),
            slots: 16,
            pack_slots: 4,
            burden_limit: 10000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
        gear(&mut k, id, id * 100, 1, 0x100000, 1);
        k.register_physical_combat(EntityId(id), Arc::new(prepared(id == 1)))
            .unwrap();
    }
    let p = k.physical_combat_profile(EntityId(1)).unwrap();
    let selected = bace_combat::physical::select_melee(p, 2, 0.5, false).unwrap();
    let motion = p.maneuvers[selected.maneuver].motion;
    k.register_physical_motion(
        EntityId(1),
        motion,
        selected.speed,
        chain(motion, selected.speed, 16, 9),
    )
    .unwrap();
    k.set_physical_options(EntityId(1), 0, 0).unwrap();
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    k.step().unwrap();
    k.take_combat_outcome();
    k
}
fn drain(k: &mut Kernel) -> Vec<bace_simulation::PhysicalCombatEvent> {
    let mut result = vec![];
    while let Some(e) = k.take_physical_combat_event() {
        result.push(e);
    }
    while k.take_combat_event().is_some() {}
    while k.take_combat_outcome().is_some() {}
    result
}
#[test]
fn dat_hook_replaces_fixture_deadline_and_queued_requests_coalesce() {
    let mut k = connected(1.5);
    k.enqueue(attack(2)).unwrap();
    for _ in 0..5 {
        k.step().unwrap();
    }
    assert_eq!(
        k.world().combatant(EntityId(2)).unwrap().health(),
        1000,
        "fixture hooks must not apply damage"
    );
    assert!(
        drain(&mut k)
            .iter()
            .any(|e| matches!(e, bace_simulation::PhysicalCombatEvent::Motion { .. }))
    );
    for sequence in 3..20 {
        k.enqueue(attack(sequence)).unwrap();
    }
    let mut impacts = 0;
    let mut starts = 1;
    for _ in 0..50 {
        k.step().unwrap();
        for event in drain(&mut k) {
            impacts += usize::from(matches!(
                event,
                bace_simulation::PhysicalCombatEvent::Impact { .. }
            ));
            starts += usize::from(matches!(
                event,
                bace_simulation::PhysicalCombatEvent::Motion { .. }
            ));
            assert!(
                !matches!(event, bace_simulation::PhysicalCombatEvent::Fault { .. }),
                "{event:?}"
            );
        }
    }
    assert_eq!(
        (starts, impacts),
        (2, 2),
        "bounded replacement: token={:?} state={:?} busy={} pending={}",
        k.world().source_motion_token(EntityId(1)),
        k.world().source_motion_state(EntityId(1)),
        k.world().motion_busy(EntityId(1)),
        k.world().has_pending_action_motion(EntityId(1))
    );
}
#[test]
fn sticky_approach_uses_accepted_physics_and_cancel_clears_controller() {
    let mut k = connected(8.0);
    k.enqueue(attack(2)).unwrap();
    for _ in 0..5 {
        k.step().unwrap();
        drain(&mut k);
    }
    let position = k.world().actor_state(EntityId(1)).unwrap().1.position();
    assert!(position.x > 0.0 && position.x < 8.0);
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 1000);
    k.enqueue(combat(3, CombatRequest::CancelAttack)).unwrap();
    k.step().unwrap();
    assert!(k.world().body(EntityId(1)).unwrap().server_move().is_none());
    let stopped = k.world().actor_state(EntityId(1)).unwrap().1.position();
    for _ in 0..10 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert_eq!(
        k.world().actor_state(EntityId(1)).unwrap().1.position(),
        stopped
    );
}
#[test]
fn trusted_cancel_retains_charge_and_motion_admission_fence() {
    let mut k = connected(1.5);
    k.enqueue(attack(2)).unwrap();
    k.step().unwrap();
    k.enqueue(combat(3, CombatRequest::CancelAttack)).unwrap();
    k.enqueue(attack(4)).unwrap();
    for _ in 0..8 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 1000);
    assert!(k.capture_physical_recovery(EntityId(1)) > 0.0);
    for _ in 0..40 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(k.world().combatant(EntityId(2)).unwrap().health() < 1000);
}

#[test]
fn completed_physical_ready_releases_later_authoritative_locomotion() {
    let mut k = connected(1.5);
    k.enqueue(attack(2)).unwrap();
    for _ in 0..35 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(!k.world().has_pending_action_motion(EntityId(1)));
    let state = k.world().body(EntityId(1)).unwrap().accepted();
    let before = state.position();
    k.enqueue(Command::Movement {
        actor: EntityId(1),
        epoch: state.epoch(),
        sequence: 1,
        intent: bace_motion::MotionIntent::new(Vec3::new(0., 1., 0.), false).unwrap(),
    })
    .unwrap();
    for _ in 0..5 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(
        k.world().body(EntityId(1)).unwrap().accepted().position().y > before.y + 0.1,
        "retained zero Ready must not pin the actor forever"
    );
}

fn missile_kernel() -> Kernel {
    let mut k = connected(1.5);
    let mut p = prepared(true);
    let mut launcher = p.main.clone().unwrap();
    launcher.entity = 101;
    let mut ammo = launcher.clone();
    ammo.entity = 202;
    ammo.revision = 5;
    ammo.damage = 5.0;
    ammo.damage_type = 2;
    p.equipment.push(PhysicalEquipmentStamp {
        entity: 101,
        revision: 1,
        location: 0x400000,
    });
    p.equipment.push(PhysicalEquipmentStamp {
        entity: 202,
        revision: 5,
        location: 0x800000,
    });
    gear(&mut k, 1, 101, 1, 0x400000, 1);
    gear(&mut k, 1, 202, 5, 0x800000, 3);
    p.launcher = Some(launcher);
    p.ammunition = Some(ammo);
    p.missile = Some(PhysicalMissileSpec {
        ammunition_count: 3,
        speed: 20.0,
        radius: 0.05,
        gravity: false,
        tracking: false,
        attack_motion: 0x4000001e,
        launch_seconds: 0.01,
        duration_seconds: 0.05,
        damage_modifier: 1.0,
    });
    let speed = bace_combat::physical::missile_attack_speed(
        p.quickness,
        p.launcher.as_ref().unwrap().attack_time,
    )
    .unwrap();
    k.register_physical_combat(EntityId(1), Arc::new(p))
        .unwrap();
    for (motion, frames) in [(0x4000001e, 16), (0x40000016, 45)] {
        k.register_physical_motion(EntityId(1), motion, speed, chain(motion, speed, frames, 9))
            .unwrap();
    }
    k.set_physical_options(EntityId(1), 2, 0).unwrap();
    for id in 1000..1003 {
        k.supply_physical_projectile_id(EntityId(id)).unwrap();
    }
    k.enqueue(combat(2, CombatRequest::ChangeMode(4))).unwrap();
    k.step().unwrap();
    drain(&mut k);
    k
}

#[test]
fn connected_dual_repeat_is_faster_and_alternates_the_successful_hand() {
    let mut gaps = vec![];
    for dual in [false, true] {
        let mut k = connected(1.5);
        if dual {
            let mut p = prepared(true);
            p.style = 0x80000046;
            p.maneuvers[0].style = p.style;
            let mut off = p.main.clone().unwrap();
            off.entity = 102;
            p.offhand = Some(off);
            p.equipment.push(PhysicalEquipmentStamp {
                entity: 102,
                revision: 1,
                location: 0x200000,
            });
            p.skills.push((
                49,
                PhysicalSkill {
                    advancement: 2,
                    current: 200,
                },
            ));
            let mut maneuver = p.maneuvers[0].clone();
            maneuver.motion = 0x10000187;
            maneuver.attack_type = 0x400;
            p.maneuvers.push(maneuver);
            gear(&mut k, 1, 102, 1, 0x200000, 1);
            k.register_physical_combat(EntityId(1), Arc::new(p.clone()))
                .unwrap();
            for offhand in [false, true] {
                let selected = bace_combat::physical::select_melee(&p, 2, 0.5, offhand).unwrap();
                let motion = p.maneuvers[selected.maneuver].motion;
                k.register_physical_motion(
                    EntityId(1),
                    motion,
                    selected.speed,
                    chain_in_style(motion, selected.speed, 16, 9, p.style),
                )
                .unwrap();
            }
        }
        k.set_physical_options(EntityId(1), 2, 0).unwrap();
        k.enqueue(attack(2)).unwrap();
        let mut starts = vec![];
        for _ in 0..90 {
            k.step().unwrap();
            for e in drain(&mut k) {
                match e {
                    bace_simulation::PhysicalCombatEvent::Motion { motion, .. } => {
                        starts.push((k.ticks(), motion))
                    }
                    bace_simulation::PhysicalCombatEvent::Fault { .. } => panic!("{e:?}"),
                    _ => {}
                }
            }
            if starts.len() == 2 {
                break;
            }
        }
        assert_eq!(starts.len(), 2);
        if dual {
            assert_ne!(starts[0].1, starts[1].1);
        }
        gaps.push(starts[1].0 - starts[0].0);
    }
    assert!(
        (2..=4).contains(&(gaps[0] - gaps[1])),
        "dual .4s vs normal .5s charge at 30Hz: {gaps:?}"
    );
}
fn receipt(k: &mut Kernel, p: &PhysicalLaunchProposal) {
    let operation = k.propose_item_take(EntityId(1), EntityId(202), 1).unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    k.confirm_inventory_committed(&bace_simulation::InventoryReceipt {
        operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    })
    .unwrap();
    k.confirm_physical_launch(PhysicalLaunchReceipt {
        operation: p.operation,
        ammunition: p.ammunition,
        before_revision: p.ammunition_revision,
        after_revision: p.ammunition_revision + 1,
        remaining: p.expected_count - 1,
    })
    .unwrap();
}
#[test]
fn missile_release_waits_for_dat_completion_and_live_settings_are_sampled_once() {
    let mut k = missile_kernel();
    k.enqueue(combat(
        3,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 2,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    for _ in 0..12 {
        k.step().unwrap();
        drain(&mut k);
        assert!(k.take_physical_launch().is_none());
    }
    k.set_physical_options(EntityId(1), 2, 0x18000).unwrap();
    let mut first = None;
    for _ in 0..10 {
        k.step().unwrap();
        drain(&mut k);
        if let Some(p) = k.take_physical_launch() {
            first = Some(p);
            break;
        }
    }
    let first = first.expect("DAT aim completion");
    let velocity = first.flight.as_ref().unwrap().velocity;
    assert!(((velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt() - 25.0).abs() < 0.001);
    k.set_physical_options(EntityId(1), 2, 0).unwrap();
    k.retry_physical_launch(first.operation).unwrap();
    assert_eq!(
        k.take_physical_launch().unwrap().flight,
        first.flight,
        "retry retains launch-time options and accepted pose"
    );
    receipt(&mut k, &first);
    for _ in 0..3 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(
        k.world()
            .source_motion_token(EntityId(1))
            .is_some_and(|t| t.sequence == 3),
        "trusted reload owns action"
    );
    k.enqueue(combat(4, CombatRequest::CancelAttack)).unwrap();
    k.enqueue(combat(
        5,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 2,
            accuracy: 0.0,
        },
    ))
    .unwrap();
    for _ in 0..40 {
        k.step().unwrap();
        drain(&mut k);
        assert!(k.take_physical_launch().is_none(), "cannot bypass reload");
    }
    let mut second = None;
    for _ in 0..40 {
        k.step().unwrap();
        drain(&mut k);
        if let Some(p) = k.take_physical_launch() {
            second = Some(p);
            break;
        }
    }
    let second = second.expect("new aim after trusted reload");
    let velocity = second.flight.unwrap().velocity;
    assert!((velocity.into_iter().map(|v| v * v).sum::<f32>().sqrt() - 18.0).abs() < 0.001);
}

fn style_chain(from: u32, to: u32) -> Arc<PreparedMotionChain> {
    let action = chain_in_style(0x13000132, 1.0, 8, 3, to);
    let stop = action.stop_chain().unwrap().clone();
    let idle = stop.clips()[0].clone();
    Arc::new(
        PreparedMotionChain::prepare(
            vec![
                ExecutionClip {
                    animation: 0x03000003,
                    frame_count: 8,
                    low: 0,
                    high: 7,
                    framerate: 30.0,
                    frames: vec![RootFrame::default(); 8].into(),
                    hooks: vec![].into(),
                },
                idle,
            ],
            1,
            1,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            to,
            1.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: SourceMotionState {
                style: from,
                substate: 0x41000003,
                speed: 1.0,
            },
            after: SourceMotionState {
                style: to,
                substate: 0x41000003,
                speed: 1.0,
            },
            continues_cycle: false,
        })
        .unwrap()
        .with_stop_chain(stop)
        .unwrap(),
    )
}

#[test]
fn changed_quickness_reuses_source_rate_roles_without_a_cold_reload() {
    let mut k = connected(1.5);
    let mut p = prepared(true);
    p.quickness = 300;
    k.register_physical_combat(EntityId(1), Arc::new(p.clone()))
        .unwrap();
    let selected = bace_combat::physical::select_melee(&p, 2, 0.5, false).unwrap();
    let motion = p.maneuvers[selected.maneuver].motion;
    let original = chain(motion, selected.speed, 16, 9);
    let mut clips = original.clips().to_vec();
    clips[0].framerate = 30.0 * selected.speed;
    let template =
        PreparedMotionChain::prepare(clips, 1, 1, original.physics, motion, selected.speed)
            .unwrap()
            .with_source_transition(original.source_transition().unwrap())
            .unwrap()
            .with_stop_chain(original.stop_chain().unwrap().clone())
            .unwrap()
            .with_rate_model(bace_motion::MotionRateModel {
                clears_modifiers: false,
                clips: vec![
                    bace_motion::MotionClipRate {
                        base: 30.0,
                        role: bace_motion::MotionRate::Action,
                    },
                    bace_motion::MotionClipRate {
                        base: 30.0,
                        role: bace_motion::MotionRate::Current,
                    },
                ],
                cycle_rate: bace_motion::MotionRate::Current,
                cycle_physics: original.physics,
                modifiers: vec![],
                scale: 1.0,
            })
            .unwrap();
    k.register_physical_motion(EntityId(1), motion, selected.speed, Arc::new(template))
        .unwrap();
    for (sequence, quickness) in [(2, 200), (3, 150)] {
        p.quickness = quickness;
        k.register_physical_combat(EntityId(1), Arc::new(p.clone()))
            .unwrap();
        let expected = bace_combat::physical::select_melee(&p, 2, 0.5, false)
            .unwrap()
            .speed;
        k.enqueue(attack(sequence)).unwrap();
        let mut started = false;
        let mut hit = false;
        for _ in 0..45 {
            k.step().unwrap();
            for event in drain(&mut k) {
                match event {
                    bace_simulation::PhysicalCombatEvent::Motion { speed, .. } => {
                        assert_eq!(speed, expected);
                        started = true;
                    }
                    bace_simulation::PhysicalCombatEvent::Impact { .. } => hit = true,
                    bace_simulation::PhysicalCombatEvent::Fault { .. } => panic!("{event:?}"),
                    _ => {}
                }
            }
        }
        assert!(
            started && hit,
            "live positive rate retained prepared source semantics"
        );
    }
}
#[test]
fn physical_cast_physical_executes_both_style_links_before_actions() {
    let mut k = connected(1.5);
    for (from, to) in [(0x80000040, 0x80000049), (0x80000049, 0x80000040)] {
        k.register_physical_motion(EntityId(1), to, 1.0, style_chain(from, to))
            .unwrap();
    }
    k.configure_magic_random(bace_random::RandomRoot::new([7; 32], 1).unwrap())
        .unwrap();
    k.register_magic_caster(
        EntityId(1),
        bace_simulation::MagicCaster {
            player: true,
            known_spells: std::collections::BTreeSet::from([100]),
            school_skills: [1000; 5],
            magic_defense: 0,
            mana_conversion: 0,
            components_required: false,
            safe_components: false,
        },
    )
    .unwrap();
    k.register_magic_spell(bace_simulation::PreparedMagicSpell {
        spell: bace_magic::PreparedSpell {
            id: 100,
            school: bace_magic::MagicSchool::Life,
            power: 0,
            base_mana: 10,
            range_constant: 30.0,
            range_per_skill: 0.0,
            harmful: false,
            resistable: false,
            effect: bace_magic::SpellEffect::Boost {
                vital: bace_magic::Vital::Health,
                minimum: 10,
                maximum: 10,
            },
        },
        gestures: vec![bace_simulation::PreparedCastGesture {
            gesture: bace_magic::CastGesture {
                motion: 0x13000132,
                minimum_seconds: 0.1,
            },
            duration_seconds: 0.001,
            motion_chain: Some(chain_in_style(0x13000132, 2.0, 10, 3, 0x80000049)),
        }],
        components: vec![],
        component_modifiers: vec![],
        component_loss: 0.0,
        fast_resistable_pk_spell: false,
    })
    .unwrap();
    k.enqueue(attack(2)).unwrap();
    for _ in 0..40 {
        k.step().unwrap();
        drain(&mut k);
    }
    let health = k.world().combatant(EntityId(2)).unwrap().health();
    assert!(health < 1000);
    k.enqueue(Command::Cast {
        context: context(3),
        request: bace_gameplay_api::CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    let mut motions = vec![];
    let mut completed = false;
    for tick in 0..35 {
        k.step().unwrap();
        drain(&mut k);
        while let Some(outcome) = k.take_cast_outcome() {
            assert!(outcome.result.is_ok(), "{outcome:?}");
            completed |= matches!(
                outcome.result,
                Ok(bace_gameplay_api::CastChange::Completed { .. })
            );
        }
        while let Some(event) = k.take_magic_event() {
            if let bace_simulation::MagicEvent::Motion {
                motion, sequence, ..
            } = event
            {
                motions.push((motion, sequence));
            }
        }
        if tick < 5 {
            assert_eq!(
                k.world()
                    .vital(EntityId(1), bace_entity::EntityVital::Mana)
                    .unwrap()
                    .current,
                1000
            );
        }
    }
    assert!(completed);
    assert_eq!(motions[0], (0x80000049, 1));
    assert_eq!(motions[1], (0x13000132, 3));
    assert_eq!(
        k.world()
            .vital(EntityId(1), bace_entity::EntityVital::Mana)
            .unwrap()
            .current,
        990
    );
    k.enqueue(attack(4)).unwrap();
    for _ in 0..5 {
        k.step().unwrap();
        drain(&mut k);
        assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), health);
    }
    for _ in 0..35 {
        k.step().unwrap();
        drain(&mut k);
    }
    assert!(k.world().combatant(EntityId(2)).unwrap().health() < health);
    assert_eq!(
        k.world().source_motion_state(EntityId(1)).unwrap().style,
        0x80000040
    );
}

#[test]
fn last_missile_consumption_enters_peace_through_trusted_style_and_stops_repeat() {
    let mut k = missile_kernel();
    k.register_physical_motion(
        EntityId(1),
        0x8000003d,
        1.,
        style_chain(0x80000040, 0x8000003d),
    )
    .unwrap();
    k.enqueue(combat(
        3,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 2,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    let mut count = 0;
    let mut mode = false;
    let mut final_tick = None;
    for tick in 0..400 {
        k.step().unwrap();
        for event in drain(&mut k) {
            if let bace_simulation::PhysicalCombatEvent::ModeChanged {
                actor, mode: value, ..
            } = event
            {
                assert_eq!(actor, EntityId(1));
                assert_eq!(value, 1);
                mode = true;
            }
            assert!(
                !matches!(event, bace_simulation::PhysicalCombatEvent::Fault { .. }),
                "{event:?}"
            );
        }
        while let Some(proposal) = k.take_physical_launch() {
            count += 1;
            receipt(&mut k, &proposal);
            if proposal.expected_count == 1 {
                final_tick = Some(tick);
                assert_eq!(k.world().combatant(EntityId(1)).unwrap().mode(), 1);
            }
        }
        if final_tick.is_some_and(|at| tick > at + 45) {
            break;
        }
    }
    assert_eq!(count, 3);
    assert!(mode);
    assert_eq!(
        k.world().source_motion_state(EntityId(1)).unwrap().style,
        0x8000003d
    );
    assert!(!k.world().motion_busy(EntityId(1)));
    assert_eq!(
        k.physical_combat_profile(EntityId(1))
            .unwrap()
            .missile
            .unwrap()
            .ammunition_count,
        0
    );
}
