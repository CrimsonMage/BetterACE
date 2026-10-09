use super::*;
use bace_motion::{ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame};

fn use_message(object: EntityId, sequence: u32) -> bace_transport::ReceivedMessage {
    bace_transport::ReceivedMessage {
        sequence,
        id: 1,
        queue: 9,
        bytes: GameActionEnvelope {
            sequence,
            action: bace_wire::opcode::GameActionType::Use,
            payload: &object.0.to_le_bytes(),
        }
        .encode(4096)
        .unwrap(),
    }
}

fn output_motion() -> Arc<PreparedMotionChain> {
    let clip = ExecutionClip {
        animation: 0x0300_0001,
        frame_count: 3,
        low: 0,
        high: -1,
        framerate: 30.,
        frames: vec![RootFrame::default(); 3].into(),
        hooks: vec![].into(),
    };
    Arc::new(
        PreparedMotionChain::prepare(
            vec![clip],
            0,
            0,
            MotionPhysics {
                velocity: bace_geometry::Vec3::ZERO,
                omega: bace_geometry::Vec3::ZERO,
            },
            0x1000_0057,
            1.,
        )
        .unwrap(),
    )
}

async fn admitted_binding() -> (
    crate::player_service::tests::cluster::Cluster,
    tempfile::TempDir,
    Box<GameRuntime>,
    SessionKey,
    CharacterBinding,
    EntityId,
) {
    let (cluster, directory, mut runtime, key, binding) =
        crate::game_runtime::portals::tests::output_runtime().await;
    runtime.players.test_mark_entered(key, binding).unwrap();
    let object = EntityId(0x7000_0051);
    runtime
        .register_binding_presentation(object, BindingKind::Lifestone)
        .unwrap();
    assert!(matches!(
        runtime
            .handle_binding_message(key, &use_message(object, 71))
            .unwrap(),
        ProgressionIngress::Accepted
    ));
    assert_eq!(runtime.recalls.bindings.pending[&key].context.sequence, 71);
    (cluster, directory, runtime, key, binding, object)
}

#[tokio::test]
async fn authenticated_stone_use_projects_source_sound_action_and_exact_durable_completion() {
    // This exercises the output state machine after authenticated Use; the
    // simulation test qualifies real motion admission and durable mutation.
    let (_cluster, _directory, mut runtime, key, binding, object) = admitted_binding().await;
    let context = runtime.recalls.bindings.pending[&key].context;
    let p = runtime.recalls.bindings.pending.get_mut(&key).unwrap();
    p.phase = BindingPhase::Submitted;
    p.prepared = Some(BindingPrepared {
        revision: 1,
        style: None,
        motion: output_motion(),
        seconds: 0.1,
    });
    runtime.recalls.event = Some(RecallEvent::BindingStarted {
        context,
        object,
        until_tick: 3,
    });
    runtime.project_recall_output().unwrap();
    let NetworkCommand::SendOrderedBatch {
        key: recipient,
        messages,
    } = runtime.network_output.pop_front().unwrap()
    else {
        panic!("source binding start batch");
    };
    assert_eq!(recipient, key);
    let opcodes: Vec<_> = messages
        .iter()
        .map(|(_, b)| u32::from_le_bytes(b[..4].try_into().unwrap()))
        .collect();
    assert_eq!(opcodes, vec![0xf750, 0xf74c]);
    assert_eq!(
        u32::from_le_bytes(messages[0].1[8..12].try_into().unwrap()),
        0x51
    );
    let observer = runtime.pending_reward_observers().unwrap();
    assert_eq!(observer.messages.len(), 2);
    runtime
        .acknowledge_reward_observers(observer.sequence)
        .unwrap();
    assert!(matches!(
        runtime.recalls.bindings.pending[&key].phase,
        BindingPhase::Running
    ));

    let message: Arc<str> = Arc::from("You have attuned to the Lifestone.");
    runtime.recalls.event = Some(RecallEvent::BindingStaged {
        context,
        object,
        operation: 41,
        allegiance: false,
        use_message: message.clone(),
        stamina_after: Some(50),
    });
    runtime.project_recall_output().unwrap();
    assert!(runtime.network_output.is_empty());
    runtime.record_binding_portal_completion(41, true).unwrap();
    runtime.project_completed_bindings().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("durable binding completion");
    };
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].1,
        bace_wire::ChatMessage::System {
            text: &message,
            chat_type: 7
        }
        .encode()
        .unwrap()
    );
    assert_eq!(
        u32::from_le_bytes(messages[1].1[5..9].try_into().unwrap()),
        4
    );
    assert_eq!(
        u32::from_le_bytes(messages[1].1[9..13].try_into().unwrap()),
        50
    );
    assert!(!runtime.recalls.bindings.pending.contains_key(&key));
    assert!(runtime.players.replication(binding.actor).is_some());
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

#[tokio::test]
async fn stale_and_disconnected_binding_terminals_do_not_wait_for_private_output() {
    let (_cluster, _directory, mut runtime, key, binding, object) = admitted_binding().await;
    let context = runtime.recalls.bindings.pending[&key].context;
    runtime
        .recalls
        .bindings
        .pending
        .get_mut(&key)
        .unwrap()
        .phase = BindingPhase::Submitted;
    runtime.limits.messages = 0;
    runtime.recalls.event = Some(RecallEvent::Rejected {
        context,
        error: RecallError::Stale,
    });
    runtime.project_recall_output().unwrap();
    assert!(!runtime.recalls.bindings.pending.contains_key(&key));
    assert!(runtime.network_output.is_empty());

    assert!(matches!(
        runtime
            .handle_binding_message(key, &use_message(object, 72))
            .unwrap(),
        ProgressionIngress::Accepted
    ));
    let context = runtime.recalls.bindings.pending[&key].context;
    runtime
        .recalls
        .bindings
        .pending
        .get_mut(&key)
        .unwrap()
        .phase = BindingPhase::Submitted;
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.poll_bindings().unwrap();
    assert!(runtime.recalls.bindings.pending[&key].cancelling);
    runtime.recalls.event = Some(RecallEvent::BindingStarted {
        context,
        object,
        until_tick: 3,
    });
    runtime.project_recall_output().unwrap();
    assert!(matches!(
        runtime.recalls.bindings.pending[&key].phase,
        BindingPhase::Running
    ));
    runtime.recalls.event = Some(RecallEvent::Cancelled { context });
    runtime.project_recall_output().unwrap();
    assert!(!runtime.recalls.bindings.pending.contains_key(&key));
    assert!(runtime.network_output.is_empty());
    assert!(matches!(
        runtime
            .handle_binding_message(key, &use_message(object, 73))
            .unwrap(),
        ProgressionIngress::Blocked
    ));
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn native_entered_region_admits_authored_lifestone_source() {
    let mut fixture = Box::pin(crate::game_runtime::tests::entered::fixture()).await;
    let cell = fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .player
        .player
        .entity
        .state
        .properties
        .positions
        .iter()
        .find(|p| p.id == 1)
        .unwrap()
        .value
        .obj_cell_id;
    let block = (cell >> 16) as u16;
    crate::game_runtime::tests::entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, _| {
            runtime
                .world
                .as_ref()
                .and_then(|world| world.regions.prepared_region(block))
                .is_some_and(|prepared| {
                    prepared.content.instances.iter().any(|instance| {
                        instance.template.weenie_type == 25
                            && runtime
                                .recalls
                                .bindings
                                .objects
                                .get(&EntityId(instance.source.guid))
                                == Some(&BindingKind::Lifestone)
                    })
                })
        },
    )
    .await;
    fixture.shutdown().await;
}
