use super::*;
fn checkpoint(k: &Kernel) -> bace_simulation::NpcSourceCheckpoint {
    let context = bace_gameplay_api::NpcContext {
        source: EntityId(3),
        target: Some(EntityId(1)),
        operation: 77,
    };
    let proposal = bace_simulation::NpcProposal {
        ticket: 5,
        context,
        effect: NpcEffect::Service(bace_gameplay_api::NpcOperation::DeleteSelf),
    };
    bace_simulation::NpcSourceCheckpoint {
        inventory: None,
        location: None,
        properties: None,
        source_quests: None,
        archive: Some(bace_simulation::NpcSourceArchive {
            source: EntityId(3),
            facts: bace_emotes::NpcActorFacts {
                player: false,
                creature: true,
            },
            cell: CellId(1),
            position: Vec3::new(2.0, 0.0, 0.5),
            heading: 0.0,
            properties: k.world().properties(EntityId(2)).unwrap().clone(),
        }),
        source: EntityId(3),
        active_operation: 77,
        invocations: vec![bace_simulation::NpcInvocationCheckpoint {
            operation: 77,
            event_id: [4; 16],
            key_version: 1,
            random_position: 1,
        }],
        logical_now: 1.0,
        event_id: [4; 16],
        key_version: 1,
        random_position: 1,
        vm: bace_emotes::NativeCheckpoint {
            order: 1,
            remaining: 100,
            work: vec![],
            pending: vec![bace_emotes::NativePendingRow {
                ticket: 5,
                row: bace_emotes::NativeScheduledRow {
                    inline: true,
                    depth: 0,
                    set: 0,
                    action: 0,
                    due: 1.0,
                    order: 1,
                    context,
                },
            }],
            detached: vec![],
        },
        pending: vec![bace_simulation::NpcPendingCheckpoint {
            proposal,
            completion: bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            adopted: true,
            detached: false,
        }],
    }
}
fn program(actions: Vec<EmoteAction>) -> Arc<NativeProgram> {
    Arc::new(NativeProgram::prepare(vec![set(7, None, actions)], NativeLimits::default()).unwrap())
}
#[test]
fn deleted_source_restores_dialogue_and_target_reward_without_restoring_a_body() {
    let mut k = xp_kernel();
    let saved = checkpoint(&k);
    let p = program(vec![
        EmoteAction {
            r#type: 77,
            ..Default::default()
        },
        EmoteAction {
            r#type: 8,
            message: Some("%n thanks %s".into()),
            ..Default::default()
        },
        EmoteAction {
            r#type: 47,
            amount: Some(2),
            ..Default::default()
        },
    ]);
    k.register_archived_npc(saved, p, 3.0).unwrap();
    assert!(k.world().body(EntityId(3)).is_err());
    k.step().unwrap();
    assert!(k.take_npc_notification().is_none());
    assert!(k.take_npc_proposal().is_none());
    assert!(k.release_npc_archive(EntityId(3)).is_err());
    k.acknowledge_npc_recovery_ready(EntityId(3)).unwrap();
    k.step().unwrap();
    let text = k.take_npc_notification().unwrap();
    assert!(
        matches!(text.operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="Keeper thanks Player")
    );
    let reward = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_training_credits(&reward).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(5)
    );
    k.confirm_npc_training_credits_committed(&ticket).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(7)
    );
    assert!(k.world().body(EntityId(3)).is_err());
    assert!(
        k.start_npc_emote(
            EntityId(3),
            Some(EntityId(1)),
            NativeTrigger {
                category: 7,
                ..Default::default()
            },
            [9; 16],
            88,
            false
        )
        .is_err()
    );
    k.release_npc_archive(EntityId(3)).unwrap();
    assert!(k.checkpoint_npc_source(EntityId(3)).is_err());
}
#[test]
fn deleted_source_cannot_authorize_new_physical_movement() {
    let mut k = xp_kernel();
    let saved = checkpoint(&k);
    let p = program(vec![
        EmoteAction {
            r#type: 77,
            ..Default::default()
        },
        EmoteAction {
            r#type: 87,
            obj_cell_id: Some(1),
            origin_x: Some(4.0),
            origin_y: Some(0.0),
            origin_z: Some(0.5),
            ..Default::default()
        },
    ]);
    k.register_archived_npc(saved, p, 3.0).unwrap();
    k.acknowledge_npc_recovery_ready(EntityId(3)).unwrap();
    k.step().unwrap();
    let movement = k.take_npc_proposal().unwrap();
    assert!(matches!(
        k.begin_npc_movement(&movement, None),
        Err(NpcFailure::MissingActor)
    ));
    assert!(k.world().body(EntityId(3)).is_err());
    assert!(k.release_npc_archive(EntityId(3)).is_err());
}

#[test]
fn delete_source_freezes_pose_until_receipt_then_continues_without_body() {
    let mut k = xp_kernel();
    k.spawn_npc(
        EntityId(3),
        bace_simulation::NpcBlueprint {
            cell: CellId(1),
            position: Vec3::new(3., 0., 4.),
            radius: 0.5,
            capabilities: Capabilities {
                speed: 1.,
                jump_impulse: 1.,
            },
            combat: CombatantProfile {
                maximum_health: 10,
                melee_damage: 1,
                melee_range: 1.,
                attack_duration: 1.,
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
    let properties = k.world().properties(EntityId(2)).unwrap().clone();
    k.register_native_npc_with_properties(
        EntityId(3),
        program(vec![
            EmoteAction {
                r#type: 77,
                ..Default::default()
            },
            EmoteAction {
                r#type: 8,
                message: Some("%n remains".into()),
                ..Default::default()
            },
            EmoteAction {
                r#type: 47,
                amount: Some(2),
                ..Default::default()
            },
        ]),
        5.,
        properties,
    )
    .unwrap();
    k.start_npc_emote(
        EntityId(3),
        Some(EntityId(1)),
        NativeTrigger {
            category: 7,
            ..Default::default()
        },
        [61; 16],
        77,
        false,
    )
    .unwrap();
    k.step().unwrap();
    let proposal = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_deletion(&proposal).unwrap();
    let frozen = k.world().body(EntityId(3)).unwrap().accepted();
    for _ in 0..90 {
        k.step().unwrap();
    }
    assert_eq!(k.world().body(EntityId(3)).unwrap().accepted(), frozen);
    assert!(k.take_npc_notification().is_none());
    let mut wrong = ticket.clone();
    wrong.hold.operation += 1;
    assert!(k.confirm_npc_transient_deletion(&wrong).is_err());
    k.reject_npc_deletion(&ticket).unwrap();
    k.step().unwrap();
    assert_ne!(k.world().body(EntityId(3)).unwrap().accepted(), frozen);
    let ticket = k.prepare_npc_deletion(&proposal).unwrap();
    let saved = k
        .preview_npc_deletion_checkpoint(&ticket, k.ticks() as f64 / 30.)
        .unwrap();
    assert_eq!(
        saved.archive.as_ref().unwrap().position,
        k.world().body(EntityId(3)).unwrap().accepted().position()
    );
    assert!(saved.pending[0].adopted);
    k.confirm_npc_transient_deletion(&ticket).unwrap();
    assert!(k.world().body(EntityId(3)).is_err());
    assert!(k.reject_npc_deletion(&ticket).is_err());
    k.step().unwrap();
    let text = k.take_npc_notification().unwrap();
    assert!(
        matches!(text.operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="Keeper remains")
    );
    let reward = k.take_npc_proposal().unwrap();
    let credits = k.prepare_npc_training_credits(&reward).unwrap();
    k.confirm_npc_training_credits_committed(&credits).unwrap();
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(7)
    );
    k.release_npc_archive(EntityId(3)).unwrap();
}

#[test]
fn current_world_ticket_epoch_cannot_collide_with_late_restored_sources() {
    let mut k = xp_kernel();
    k.seed_npc_ticket_epoch(7).unwrap();
    let remap = |mut snapshot: bace_simulation::NpcSourceCheckpoint, source: u32, ticket: u64| {
        snapshot.source = EntityId(source);
        snapshot.archive.as_mut().unwrap().source = EntityId(source);
        snapshot.vm.pending[0].ticket = ticket;
        snapshot.vm.pending[0].row.context.source = EntityId(source);
        snapshot.pending[0].proposal.ticket = ticket;
        snapshot.pending[0].proposal.context.source = EntityId(source);
        snapshot
    };
    let original = checkpoint(&k);
    let script = program(vec![EmoteAction {
        r#type: 77,
        ..Default::default()
    }]);
    k.register_archived_npc(original.clone(), script.clone(), 3.)
        .unwrap();
    k.register_native_npc(EntityId(2), script.clone(), 3.)
        .unwrap();
    k.start_npc_emote(
        EntityId(2),
        None,
        NativeTrigger {
            category: 7,
            ..Default::default()
        },
        [91; 16],
        90,
        false,
    )
    .unwrap();
    k.step().unwrap();
    let first = k.take_npc_proposal().unwrap();
    assert_eq!(first.ticket, (7u64 << 32) + 1);
    k.register_archived_npc(remap(original.clone(), 4, 15), script.clone(), 3.)
        .unwrap();
    assert_eq!(
        k.checkpoint_npc_source(EntityId(3)).unwrap().pending[0]
            .proposal
            .ticket,
        5
    );
    assert_eq!(
        k.checkpoint_npc_source(EntityId(4)).unwrap().pending[0]
            .proposal
            .ticket,
        15
    );
    assert!(
        k.register_archived_npc(
            remap(original.clone(), 5, (8u64 << 32) + 1),
            script.clone(),
            3.
        )
        .is_err()
    );
    assert!(k.checkpoint_npc_source(EntityId(5)).is_err());
    k.register_archived_npc(remap(original, 5, 25), script, 3.)
        .unwrap();
    assert!(k.seed_npc_ticket_epoch(8).is_err());
}
