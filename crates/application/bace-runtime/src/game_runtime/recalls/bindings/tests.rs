use super::*;
use bace_motion::{ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame};
#[path = "tests/allegiance.rs"]
mod allegiance;

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
async fn disconnect_during_binding_motion_preparation_drains_exact_completion() {
    let (_cluster, _directory, mut runtime, key, binding, _object) = admitted_binding().await;
    let context = runtime.recalls.bindings.pending[&key].context;
    runtime
        .recalls
        .bindings
        .pending
        .get_mut(&key)
        .unwrap()
        .phase = BindingPhase::Preparing {
        correlation: 99,
        revision: 7,
    };
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.poll_bindings().unwrap();
    assert!(matches!(
        runtime.recalls.bindings.pending[&key].phase,
        BindingPhase::Preparing { .. }
    ));
    runtime
        .accept_binding_motion_completion(BindingMotionPreparationCompletion {
            correlation: 99,
            context,
            before_revision: 7,
            result: Ok(
                crate::player_preparation_worker::PreparedBindingMotionData {
                    style: None,
                    motion: output_motion(),
                    seconds: 0.1,
                },
            ),
        })
        .unwrap();
    runtime.poll_bindings().unwrap();
    assert!(!runtime.recalls.bindings.pending.contains_key(&key));
    assert!(runtime.recalls.bindings.unmatched.is_none());
    assert!(runtime.network_output.is_empty());
    runtime.sessions.remove(&key);
    runtime.quiesce(std::time::Duration::ZERO).unwrap();
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn native_entered_lifestone_use_keeps_stone_stationary_and_commits_sanctuary() {
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
    let region = fixture
        .runtime
        .world
        .as_ref()
        .unwrap()
        .regions
        .prepared_region(block)
        .unwrap();
    let stone = region
        .content
        .instances
        .iter()
        .find(|instance| instance.template.weenie_type == 25)
        .unwrap();
    let stone_id = EntityId(stone.source.guid);
    let stone_cell = stone.source.obj_cell_id;
    let stone_pose = bace_geometry::Vec3::new(
        stone.source.origin_x,
        stone.source.origin_y,
        stone.source.origin_z,
    );
    let use_message = stone
        .template
        .properties
        .strings
        .iter()
        .find(|p| p.id == 18)
        .map_or("", |p| p.value.as_str())
        .to_owned();
    let actor = &fixture.runtime.sessions[&fixture.key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .player
        .player
        .entity
        .state;
    let mut assets =
        crate::region_activation::VerifiedRegionAssets::open(&fixture.runtime.bootstrap.assets)
            .unwrap();
    let avatar = assets.prepare_avatar_dat(actor).unwrap();
    assert_eq!(stone_cell, 0x8602_0134, "accepted authored source cell");
    assert!(
        region
            .geometry
            .validate_placement(
                stone_cell,
                stone_pose,
                &avatar.shape,
                &[],
                fixture.binding.actor.0
            )
            .is_err(),
        "authored stone floor is not player placement"
    );
    let before = fixture
        .runtime
        .online_saves
        .baseline(fixture.binding.actor.0)
        .unwrap()
        .0
        .clone();
    let stamina_before = before
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter()
        .find(|p| p.id == 3)
        .unwrap()
        .value
        .current_level;
    let mut attempted = 0_u64;
    let mut accepted = None;
    for offset in [
        [1.5, 0.0, 0.5],
        [2.0, 0.0, 0.5],
        [2.5, 0.0, 0.5],
        [-1.5, 0.0, 0.5],
        [-2.0, 0.0, 0.5],
        [-2.5, 0.0, 0.5],
        [0.0, 1.5, 0.5],
        [0.0, 2.0, 0.5],
        [0.0, -1.5, 0.5],
        [0.0, -2.0, 0.5],
        [1.5, 0.0, 1.0],
        [-1.5, 0.0, 1.0],
    ] {
        let near = stone_pose + bace_geometry::Vec3::new(offset[0], offset[1], offset[2]);
        if region
            .geometry
            .validate_placement(
                stone_cell,
                near,
                &avatar.shape,
                &[],
                fixture.binding.actor.0,
            )
            .is_err()
        {
            continue;
        }
        attempted += 1;
        fixture
            .runtime
            .simulation
            .input()
            .try_submit(bace_simulation::Command::ServerTeleport {
                actor: fixture.binding.actor,
                cell: bace_types::CellId(stone_cell),
                position: near,
            })
            .unwrap();
        let correlation = u64::MAX - 10 - attempted;
        fixture
            .runtime
            .simulation
            .input()
            .try_submit(bace_simulation::Command::ObjectView(Box::new(
                bace_gameplay_api::visibility::ObjectViewRequest {
                    correlation,
                    binding: fixture.binding,
                    entities: vec![fixture.binding.actor, stone_id],
                },
            )))
            .unwrap();
        let view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Ok(outcome) = fixture.runtime.simulation.object_view_outcomes().try_recv()
                    && outcome.correlation == correlation
                {
                    break outcome.result.clone().unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        if view.views[0].1.as_ref().is_ok_and(|player| {
            player.cell == stone_cell && player.position == [near.x, near.y, near.z]
        }) {
            accepted = Some((near, view));
            break;
        }
    }
    let (near, view) = accepted
        .unwrap_or_else(|| panic!("no live placement in {attempted} source-valid candidates"));
    let player_view = view.views[0].1.as_ref().unwrap();
    let stone_view = view.views[1].1.as_ref().unwrap();
    assert_eq!(
        (player_view.cell, player_view.position),
        (stone_cell, [near.x, near.y, near.z])
    );
    assert_eq!(
        (stone_view.cell, stone_view.position),
        (stone_cell, [stone_pose.x, stone_pose.y, stone_pose.z]),
        "stone pose remains an owned copy"
    );
    let output_start = fixture.client.messages.len();
    let use_packet = GameActionEnvelope {
        sequence: 2,
        action: bace_wire::opcode::GameActionType::Use,
        payload: &stone_id.0.to_le_bytes(),
    }
    .encode(4096)
    .unwrap();
    fixture
        .client
        .send_message(&fixture.runtime, fixture.key, &use_packet);
    let source_chat = bace_wire::ChatMessage::System {
        text: &use_message,
        chat_type: 7,
    }
    .encode()
    .unwrap();
    let completion = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let elapsed = fixture.runtime.clock.monotonic.elapsed();
            fixture.runtime.poll(elapsed).unwrap();
            fixture.client.drain(&fixture.runtime);
            if fixture.runtime.recalls.bindings.pending.is_empty()
                && fixture.client.messages[output_start..]
                    .iter()
                    .any(|m| m.bytes == source_chat)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await;
    let phase = fixture
        .runtime
        .recalls
        .bindings
        .pending
        .get(&fixture.key)
        .map(|p| match &p.phase {
            BindingPhase::Capture => "Capture",
            BindingPhase::Capturing(_) => "Capturing",
            BindingPhase::Captured(..) => "Captured",
            BindingPhase::Preparing { .. } => "Preparing",
            BindingPhase::Ready => "Ready",
            BindingPhase::Submitted => "Submitted",
            BindingPhase::Running => "Running",
            BindingPhase::Staged => "Staged",
            BindingPhase::Failed(_) => "Failed",
        });
    assert!(
        completion.is_ok(),
        "authenticated Use did not complete: phase={phase:?}, failure={:?}, output opcodes={:?}",
        fixture.runtime.binding_failure(fixture.key),
        fixture.client.messages[output_start..]
            .iter()
            .filter_map(|m| m
                .bytes
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_le_bytes))
            .collect::<Vec<_>>()
    );
    fixture
        .runtime
        .simulation
        .input()
        .try_submit(bace_simulation::Command::ObjectView(Box::new(
            bace_gameplay_api::visibility::ObjectViewRequest {
                correlation: u64::MAX - 100,
                binding: fixture.binding,
                entities: vec![fixture.binding.actor, stone_id],
            },
        )))
        .unwrap();
    let final_view = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Ok(outcome) = fixture.runtime.simulation.object_view_outcomes().try_recv()
                && outcome.correlation == u64::MAX - 100
            {
                break outcome.result.clone().unwrap();
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let accepted_player = final_view.views[0].1.as_ref().unwrap();
    let accepted_stone = final_view.views[1].1.as_ref().unwrap();
    assert_eq!(accepted_player.cell, stone_cell);
    assert_eq!(
        (accepted_stone.cell, accepted_stone.position),
        (stone_cell, [stone_pose.x, stone_pose.y, stone_pose.z]),
        "binding must not alias or move the stone pose"
    );
    let after = fixture
        .runtime
        .online_saves
        .baseline(fixture.binding.actor.0)
        .unwrap()
        .0;
    let sanctuary = after
        .player
        .entity
        .state
        .properties
        .positions
        .iter()
        .find(|p| p.id == 4)
        .unwrap();
    assert_eq!(sanctuary.value.obj_cell_id, stone_cell);
    assert_eq!(
        [
            sanctuary.value.position_x,
            sanctuary.value.position_y,
            sanctuary.value.position_z
        ],
        accepted_player.position
    );
    let stamina_after = after
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter()
        .find(|p| p.id == 3)
        .unwrap()
        .value
        .current_level;
    assert!(stamina_after < stamina_before);
    let output = &fixture.client.messages[output_start..];
    let sound = output
        .iter()
        .position(|m| m.bytes.starts_with(&0xf750_u32.to_le_bytes()))
        .expect("source lifestone sound");
    let motion = output
        .iter()
        .position(|m| m.bytes.starts_with(&0xf74c_u32.to_le_bytes()))
        .expect("source binding action motion");
    let chat = output
        .iter()
        .position(|m| m.bytes == source_chat)
        .expect("source binding use message");
    assert!(
        sound < motion && motion < chat,
        "source sound/action/chat order"
    );
    assert!(fixture.client.messages[output_start..].iter().any(|m| {
        m.bytes.starts_with(
            &bace_wire::opcode::GameMessageOpcode::PrivateUpdateAttribute2ndLevel
                .0
                .to_le_bytes(),
        )
    }));
    let stored = fixture
        .runtime
        .bootstrap
        .store
        .load(fixture.binding.actor.0)
        .await
        .unwrap()
        .unwrap();
    let durable = bace_storage_codec::PlayerSaveV6::decode_or_migrate(&stored.bytes).unwrap();
    assert_eq!(
        durable
            .player
            .entity
            .state
            .properties
            .positions
            .iter()
            .find(|p| p.id == 4)
            .unwrap()
            .value,
        sanctuary.value
    );
    assert_eq!(
        durable
            .player
            .entity
            .state
            .properties
            .secondary_attributes
            .iter()
            .find(|p| p.id == 3)
            .unwrap()
            .value
            .current_level,
        stamina_after
    );
    fixture.shutdown().await;
}
