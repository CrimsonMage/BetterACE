use super::*;
fn movement_kernel(x: f32, cell: u32) -> (Kernel, bace_simulation::NpcProposal) {
    let mut k = kernel_with_turn_rate(16, false, 3.0);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 87,
                extent: 1.0,
                obj_cell_id: Some(cell),
                origin_x: Some(x),
                origin_y: Some(0.0),
                origin_z: Some(0.5),
                angles_w: Some(1.0),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    (k, proposal)
}
#[test]
fn movement_completion_requires_actual_swept_arrival_and_rejects_generic_receipts() {
    let (mut k, proposal) = movement_kernel(4.0, 1);
    let before = k.world().body(EntityId(2)).unwrap().accepted().position();
    k.begin_npc_movement(&proposal, None).unwrap();
    assert!(!k.poll_npc_movement(&proposal).unwrap());
    assert_eq!(
        k.world().body(EntityId(2)).unwrap().accepted().position(),
        before
    );
    assert!(
        k.complete_npc_service(
            &proposal,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 }
        )
        .is_err()
    );
    let mut arrived = false;
    for _ in 0..180 {
        assert!(k.step().unwrap().is_empty());
        if k.poll_npc_movement(&proposal).unwrap() {
            arrived = true;
            break;
        }
    }
    assert!(arrived);
    assert!((k.world().body(EntityId(2)).unwrap().accepted().position().x - 4.0).abs() <= 0.6);
    assert!(k.npc_pending_service(proposal.ticket).is_none());
}
#[test]
fn absent_cell_and_blocked_destination_never_become_teleports() {
    let (mut k, proposal) = movement_kernel(4.0, 2);
    let before = k.world().body(EntityId(2)).unwrap().accepted();
    assert!(k.begin_npc_movement(&proposal, None).is_err());
    assert_eq!(k.world().body(EntityId(2)).unwrap().accepted(), before);
    let (mut k, proposal) = movement_kernel(50.0, 1);
    k.begin_npc_movement(&proposal, None).unwrap();
    for _ in 0..360 {
        assert!(k.step().unwrap().is_empty());
        assert!(!k.poll_npc_movement(&proposal).unwrap());
    }
    assert!(k.world().body(EntityId(2)).unwrap().accepted().position().x < 10.0);
    k.cancel_npc_movement(&proposal).unwrap();
    assert_eq!(k.npc_pending_service(proposal.ticket), Some(&proposal));
    assert!(k.world().body(EntityId(2)).unwrap().server_move().is_none());
}
#[test]
fn detached_admission_preserves_authored_post_delay_across_recovery() {
    let mut k = kernel(16);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![
                EmoteAction {
                    r#type: 3,
                    weenie_class_id: Some(100),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 8,
                    message: Some("continued".into()),
                    delay: 0.5,
                    ..Default::default()
                },
            ],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let now = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
    let checkpoint = k
        .preview_npc_service_admission(&proposal, now, 1.0)
        .unwrap();
    assert!(checkpoint.pending[0].detached);
    assert!(!checkpoint.pending[0].adopted);
    assert_eq!(checkpoint.vm.work[0].due, now + 1.5);
    k.admit_npc_service(&proposal, 1.0).unwrap();
    while (k.ticks() as f64 / 30.0) < now + 1.5 {
        k.step().unwrap();
        assert!(k.take_npc_notification().is_none());
    }
    k.step().unwrap();
    assert!(k.take_npc_notification().is_some());
    assert_eq!(
        k.npc_pending_service(proposal.ticket),
        Some(&proposal),
        "source continuation is not the Give receipt"
    );
}

#[test]
fn animation_completion_comes_from_the_exact_world_callback() {
    use bace_motion::{
        ExecutionClip, MotionExecutionEvent, MotionPhysics, PreparedMotionChain, RootFrame,
        SourceMotionState, SourceMotionTransition,
    };
    let ready = SourceMotionState {
        style: 0x8000003d,
        substate: 0x41000003,
        speed: 1.0,
    };
    let clip = |animation, frames| ExecutionClip {
        animation,
        frame_count: frames,
        low: 0,
        high: frames as i32 - 1,
        framerate: 30.0,
        frames: vec![RootFrame::default(); frames as usize].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(
            vec![clip(0x03000002, 2)],
            0,
            0,
            physics,
            ready.substate,
            1.0,
        )
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before: ready,
            after: ready,
            continues_cycle: true,
        })
        .unwrap(),
    );
    let motion = 0x130000aa;
    let chain = Arc::new(
        PreparedMotionChain::prepare(
            vec![clip(0x03000001, 10), clip(0x03000002, 2)],
            1,
            1,
            physics,
            motion,
            1.0,
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
    );
    let mut k = kernel(16);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 5,
                motion: Some(motion),
                extent: 1.0,
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let token = k.begin_npc_motion(&proposal, chain).unwrap();
    assert!(k.poll_npc_motion(&proposal).unwrap().is_none());
    let fake = bace_world::WorldMotionEvent {
        actor: EntityId(2),
        epoch: k.world().body(EntityId(2)).unwrap().accepted().epoch(),
        token,
        event: MotionExecutionEvent::Completed,
    };
    assert!(k.adopt_npc_motion_completion(&proposal, fake).is_err());
    let mut terminal = None;
    for _ in 0..20 {
        k.step().unwrap();
        while let Some(event) = k.poll_npc_motion(&proposal).unwrap() {
            if matches!(
                event.event,
                MotionExecutionEvent::Completed | MotionExecutionEvent::Cancelled
            ) {
                terminal = Some(event);
                break;
            }
        }
        if terminal.is_some() {
            break;
        }
    }
    let event = terminal.expect("source motion completion");
    assert_eq!(event.event, MotionExecutionEvent::Completed);
    assert_eq!(event.token, token);
    k.adopt_npc_motion_completion(&proposal, event).unwrap();
    assert!(k.adopt_npc_motion_completion(&proposal, event).is_err());
}

#[test]
fn local_signal_enters_only_enabled_receiver_vm_after_owner_dispatch() {
    let mut k = kernel(16);
    k.register_native_npc(
        EntityId(1),
        Arc::new(
            NativeProgram::prepare(
                vec![set(
                    37,
                    Some("Open"),
                    vec![EmoteAction {
                        r#type: 8,
                        message: Some("heard".into()),
                        ..Default::default()
                    }],
                )],
                NativeLimits::default(),
            )
            .unwrap(),
        ),
        3.0,
    )
    .unwrap();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![
                EmoteAction {
                    r#type: 53,
                    stat: Some(290),
                    amount: Some(1),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 53,
                    stat: Some(291),
                    amount: Some(2),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 88,
                    message: Some("open".into()),
                    ..Default::default()
                },
            ],
        )],
    );
    for _ in 0..2 {
        k.step().unwrap();
        let property = k.take_npc_proposal().unwrap();
        k.confirm_npc_committed(&property).unwrap();
    }
    k.step().unwrap();
    let signal = k.take_npc_proposal().unwrap();
    assert!(k.take_npc_notification().is_none());
    assert_eq!(k.apply_npc_signal(&signal).unwrap(), 1);
    assert!(k.apply_npc_signal(&signal).is_err());
    k.step().unwrap();
    let heard = k.take_npc_notification().unwrap();
    assert_eq!(heard.context.source, EntityId(1));
    assert_eq!(heard.context.target, Some(EntityId(2)));
    assert!(
        matches!(heard.operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="heard")
    );
    assert!(k.take_npc_notification().is_none());
}

#[test]
fn npc_teleport_joins_exact_portal_receipt_and_waits_for_world_owner() {
    let mut k = xp_kernel();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 99,
                obj_cell_id: Some(1),
                origin_x: Some(6.0),
                origin_y: Some(0.0),
                origin_z: Some(0.5),
                angles_w: Some(1.0),
                angles_x: Some(0.0),
                angles_y: Some(0.0),
                angles_z: Some(0.0),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let before = k.world().actor_state(EntityId(1)).unwrap();
    let ticket = k.prepare_npc_teleport(&proposal).unwrap();
    assert!(
        k.take_portal_proposal().is_none(),
        "NPC journal owns this portal proposal"
    );
    let receipt = bace_simulation::PortalServiceReceipt {
        operation: ticket.portal.operation,
        actor: ticket.portal.actor,
        after_revision: ticket.portal.after_revision,
        revisions: ticket
            .portal
            .participants
            .iter()
            .map(|(actor, _, after)| (*actor, *after))
            .collect(),
    };
    assert!(k.confirm_portal_committed(&receipt).is_err());
    assert!(k.reject_portal_proposal(receipt.operation).is_err());
    for _ in 0..3 {
        k.step().unwrap();
    }
    assert_eq!(
        k.world().actor_state(EntityId(1)).unwrap().1.position(),
        before.1.position()
    );
    let mut wrong = receipt.clone();
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_npc_teleport_committed(&ticket, &wrong).is_err());
    k.confirm_npc_teleport_committed(&ticket, &receipt).unwrap();
    assert_eq!(
        k.world().actor_state(EntityId(1)).unwrap().1.position(),
        before.1.position(),
        "durability receipt is not a teleport callback"
    );
    let mut teleported = false;
    for _ in 0..4 {
        k.step().unwrap();
        while let Some(event) = k.take_portal_event() {
            if matches!(event,bace_simulation::PortalServiceEvent::Teleported{operation,..}if operation==receipt.operation)
            {
                teleported = true;
            }
        }
    }
    assert!(teleported);
    assert_eq!(
        k.world().actor_state(EntityId(1)).unwrap().1.position().x,
        6.0
    );
    assert!(k.confirm_npc_teleport_committed(&ticket, &receipt).is_err());
}

#[test]
fn reset_home_and_kill_self_use_population_and_death_owners() {
    let mut k = kernel(32);
    k.spawn_npc(
        EntityId(3),
        bace_simulation::NpcBlueprint {
            cell: CellId(1),
            position: Vec3::new(4.0, 0.0, 0.5),
            radius: 0.5,
            capabilities: Capabilities {
                speed: 1.0,
                jump_impulse: 1.0,
            },
            combat: CombatantProfile {
                maximum_health: 10,
                melee_damage: 1,
                melee_range: 1.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player: false,
            },
            visual_range: 0.01,
            think_interval: 30,
            corpse_template: 9000,
            xp_override: Some(0),
            loot: vec![],
            death_animation_ticks: 2,
            respawn_ticks: 30,
            corpse_decay_ticks: 30,
        },
    )
    .unwrap();
    k.register_native_npc(
        EntityId(3),
        Arc::new(
            NativeProgram::prepare(
                vec![set(
                    7,
                    None,
                    vec![
                        EmoteAction {
                            r#type: 57,
                            ..Default::default()
                        },
                        EmoteAction {
                            r#type: 78,
                            ..Default::default()
                        },
                    ],
                )],
                NativeLimits::default(),
            )
            .unwrap(),
        ),
        5.0,
    )
    .unwrap();
    k.start_npc_emote(
        EntityId(3),
        Some(EntityId(1)),
        NativeTrigger {
            category: 7,
            ..Default::default()
        },
        [80; 16],
        1,
        true,
    )
    .unwrap();
    k.step().unwrap();
    let reset = k.take_npc_proposal().unwrap();
    let home = k.apply_npc_reset_home(&reset).unwrap();
    assert_eq!(home.cell, Some(CellId(1)));
    assert_eq!(
        home.position,
        k.world().actor_state(EntityId(3)).unwrap().1.position()
    );
    k.step().unwrap();
    let kill = k.take_npc_proposal().unwrap();
    assert_eq!(k.apply_npc_kill_self(&kill).unwrap(), 10);
    assert_eq!(k.world().combatant(EntityId(3)).unwrap().health(), 0);
    assert!(k.apply_npc_kill_self(&kill).is_err());
    for _ in 0..4 {
        k.step().unwrap();
    }
    assert!(
        k.take_death_proposal().is_none(),
        "missing fixture RNG retains the lethal event"
    );
    k.supply_loot_random(&[0.5]).unwrap();
    for _ in 0..4 {
        k.step().unwrap();
    }
    let death = k
        .take_death_proposal()
        .expect("existing corpse/reward durability stage");
    assert_eq!(death.victim, EntityId(3));
    assert!(
        k.world().actor_state(EntityId(3)).is_ok(),
        "death proposal does not fabricate corpse persistence"
    );
}

#[test]
fn journal_preview_pauses_detached_source_clock_until_exact_release() {
    use bace_simulation::{NpcServiceAction as A, NpcServiceCommand, NpcServiceResult as R};
    let mut k = kernel(16);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![
                EmoteAction {
                    r#type: 3,
                    weenie_class_id: Some(100),
                    ..Default::default()
                },
                EmoteAction {
                    r#type: 8,
                    message: Some("after commit latency".into()),
                    delay: 1.,
                    ..Default::default()
                },
            ],
        )],
    );
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    k.admit_npc_service(&proposal, 0.).unwrap();
    let outcome = k.dispatch_npc_service(NpcServiceCommand {
        correlation: 1,
        action: A::PreviewOwnerCompletion {
            proposal: proposal.clone(),
        },
    });
    assert!(matches!(outcome.result, Ok(R::Checkpoint(_))));
    let frozen = k.checkpoint_npc_source(EntityId(2)).unwrap();
    for _ in 0..90 {
        k.step().unwrap();
    }
    let held = k.checkpoint_npc_source(EntityId(2)).unwrap();
    assert_eq!(held.vm, frozen.vm);
    assert_eq!(held.logical_now, frozen.logical_now);
    assert!(k.take_npc_notification().is_none());
    let mut wrong = proposal.clone();
    wrong.ticket += 1;
    assert!(
        k.dispatch_npc_service(NpcServiceCommand {
            correlation: 2,
            action: A::ReleaseJournal { proposal: wrong }
        })
        .result
        .is_err()
    );
    assert!(
        k.dispatch_npc_service(NpcServiceCommand {
            correlation: 3,
            action: A::ReleaseJournal {
                proposal: proposal.clone()
            }
        })
        .result
        .is_ok()
    );
    for _ in 0..29 {
        k.step().unwrap();
        assert!(k.take_npc_notification().is_none());
    }
    for _ in 0..3 {
        k.step().unwrap();
    }
    assert!(
        k.take_npc_notification().is_some(),
        "{:?}",
        k.checkpoint_npc_source(EntityId(2)).unwrap()
    );
    assert_eq!(k.npc_pending_service(proposal.ticket), Some(&proposal));
}

#[test]
fn idle_checkpoint_holds_new_invocations_until_exact_release() {
    use bace_simulation::{NpcServiceAction as A, NpcServiceCommand, NpcServiceResult as R};
    let mut k = kernel(16);
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 8,
                message: Some("done".into()),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    assert!(k.take_npc_notification().is_some());
    let outcome = k.dispatch_npc_service(NpcServiceCommand {
        correlation: 1,
        action: A::FreezeIdle {
            source: EntityId(2),
            operation: 55,
        },
    });
    let Ok(R::Checkpoint(saved)) = outcome.result else {
        panic!("idle VM checkpoint")
    };
    assert!(saved.pending.is_empty() && saved.vm.work.is_empty());
    for _ in 0..30 {
        k.step().unwrap();
    }
    assert_eq!(
        k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now,
        saved.logical_now
    );
    assert!(
        k.start_npc_emote(
            EntityId(2),
            Some(EntityId(1)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [92; 16],
            91,
            false
        )
        .is_err()
    );
    assert!(
        k.dispatch_npc_service(NpcServiceCommand {
            correlation: 2,
            action: A::ReleaseIdle {
                source: EntityId(2),
                operation: 56
            }
        })
        .result
        .is_err()
    );
    assert!(
        k.dispatch_npc_service(NpcServiceCommand {
            correlation: 3,
            action: A::ReleaseIdle {
                source: EntityId(2),
                operation: 55
            }
        })
        .result
        .is_ok()
    );
    assert!(
        k.start_npc_emote(
            EntityId(2),
            Some(EntityId(1)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [92; 16],
            91,
            false
        )
        .unwrap()
    );
}
