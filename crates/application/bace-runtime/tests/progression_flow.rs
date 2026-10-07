//! Synthetic world integration, not stock-client/asset qualification.
use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, TraitProgress, TraitState,
};
use bace_gameplay_api::*;
use bace_replication::ProgressionProjector;
use bace_runtime::{
    network::*,
    simulation::{SimulationConfig, SimulationWorker, WorkerError},
};
use bace_session::{SessionState, decode_progression};
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, EntityId};
use bace_wire::*;
use std::{net::UdpSocket, sync::Arc, time::Duration};

fn binding(generation: u64) -> CharacterBinding {
    CharacterBinding {
        session: SessionId(generation),
        account: AccountId(1),
        actor: EntityId(1),
    }
}
fn kernel(binding: CharacterBinding) -> Kernel {
    let mut world = bace_world::World::default();
    let cell = bace_types::CellId(1);
    let scene = bace_physics::SyntheticScene::new(
        0.0,
        bace_geometry::Aabb::new(
            bace_geometry::Vec3::new(-10.0, -10.0, -1.0),
            bace_geometry::Vec3::new(10.0, 10.0, 10.0),
        )
        .unwrap(),
        vec![],
    )
    .unwrap();
    world.register_scene(cell, scene).unwrap();
    let body = bace_physics::Body::spawn(
        world.scene(cell).unwrap(),
        bace_geometry::Vec3::new(0.0, 0.0, 0.5),
        0.5,
        bace_motion::Capabilities {
            speed: 5.0,
            jump_impulse: 5.0,
        },
    )
    .unwrap();
    world
        .insert(bace_entity::Actor {
            id: binding.actor,
            cell,
            body,
        })
        .unwrap();
    let mut kernel = Kernel::with_gameplay_limits(world, 16, 1, 1).unwrap();
    let table = RankTable::new(&[0, 10, 20, 30, 100]).unwrap();
    let state = CharacterProgression::with_state(
        &[TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute {
                starting_value: 100,
            },
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap();
    kernel.register_character(binding, state).unwrap();
    kernel
}
fn command(binding: CharacterBinding, sequence: u32) -> Command {
    Command::RaiseProgression {
        context: ActionContext {
            session: binding.session,
            account: binding.account,
            actor: binding.actor,
            sequence,
        },
        request: RaiseProgression {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            amount: 10,
        },
    }
}
fn login() -> Vec<u8> {
    let hex = "0400313830320000E703000002000000010000003930000007004163636F756E7400000000000000130000001273796E7468657469632D70617373776F72640000";
    let body: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
        .collect();
    encode_server_packet(
        PacketHeader {
            flags: flags::LOGIN_REQUEST,
            ..Default::default()
        },
        &body,
        &[],
        0,
    )
    .unwrap()
}
fn socket() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s
}
fn net_event(net: &NetworkThread) -> NetworkEvent {
    net.events().recv_timeout(Duration::from_secs(3)).unwrap()
}

#[test]
fn udp_action_reaches_simulation_and_ordered_primary_projection_returns_to_peer() {
    let net = (0..20)
        .find_map(|_| {
            NetworkThread::spawn(NetworkThreadConfig {
                bind_address: "127.0.0.1:0".parse().unwrap(),
                ..Default::default()
            })
            .ok()
        })
        .expect("port pair");
    let client = socket();
    let receiver = socket();
    client.send_to(&login(), net.client_address).unwrap();
    let key = match net_event(&net) {
        NetworkEvent::Login { key, .. } => key,
        event => panic!("{event:?}"),
    };
    net.try_send(NetworkCommand::Authenticated {
        key,
        account_id: AccountId(1),
    })
    .unwrap();
    let mut bytes = [0u8; 1025];
    let (n, _) = client.recv_from(&mut bytes).unwrap();
    let challenge = ConnectRequest::decode(Datagram::decode(&bytes[..n]).unwrap().body).unwrap();
    let response = encode_server_packet(
        PacketHeader {
            flags: flags::CONNECT_RESPONSE,
            ..Default::default()
        },
        &challenge.cookie.to_le_bytes(),
        &[],
        0,
    )
    .unwrap();
    receiver.send_to(&response, net.server_address).unwrap();
    assert!(matches!(net_event(&net),NetworkEvent::Connected{key:k} if k==key));
    let binding = binding(key.generation);
    let sim = SimulationWorker::spawn(kernel(binding), SimulationConfig::default()).unwrap();
    net.try_send(NetworkCommand::EnterWorldCommitted { key })
        .unwrap();
    let payload = [1, 0, 0, 0, 10, 0, 0, 0];
    let action = GameActionEnvelope {
        sequence: 1,
        action: opcode::GameActionType::RaiseAttribute,
        payload: &payload,
    }
    .encode(1024)
    .unwrap();
    let fragments = bace_transport::fragment_message(1, 9, &action, 1024).unwrap();
    let mut isaac = Isaac::new(challenge.client_seed);
    let packet = encode_server_packet(
        PacketHeader {
            sequence: 2,
            flags: flags::ENCRYPTED_CHECKSUM | flags::BLOB_FRAGMENTS,
            id: key.id,
            iteration: 1,
            ..Default::default()
        },
        &[],
        &fragments,
        isaac.next_key(),
    )
    .unwrap();
    client.send_to(&packet, net.client_address).unwrap();
    let message = match net_event(&net) {
        NetworkEvent::Message { key: k, message } => {
            assert_eq!(k, key);
            message
        }
        other => panic!("{other:?}"),
    };
    let context = ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence: 0,
    };
    let decoded =
        decode_progression(SessionState::WorldConnected, context, &message.bytes, 1024).unwrap();
    sim.input()
        .try_submit(Command::RaiseProgression {
            context: decoded.context,
            request: decoded.request,
        })
        .unwrap();
    let outcome = sim.outcomes().recv_timeout(Duration::from_secs(3)).unwrap();
    let change = outcome.result.unwrap();
    assert_eq!(change.available_experience, 90);
    let mut projector = ProgressionProjector::new(binding, 0, 16).unwrap();
    let batch = projector.project(outcome.context, change).unwrap();
    net.try_send(NetworkCommand::SendBatch {
        key,
        queue: batch.queue,
        messages: batch.messages.into(),
    })
    .unwrap();
    let mut messages = Vec::new();
    let mut incoming_keys = ClientKeys::new(challenge.server_seed);
    while messages.len() < 2 {
        let (n, source) = receiver.recv_from(&mut bytes).unwrap();
        assert_eq!(source, net.server_address);
        let packet =
            bace_transport::decode_transport_packet(&bytes[..n], &mut incoming_keys).unwrap();
        messages.extend(packet.fragments.into_iter().map(|f| f.data));
    }
    let mut xp = Reader::new(&messages[0]);
    assert_eq!(
        xp.u32().unwrap(),
        opcode::GameMessageOpcode::PrivateUpdatePropertyInt64.0
    );
    assert_eq!(xp.take(1).unwrap(), [0]);
    assert_eq!(xp.u32().unwrap(), 2);
    assert_eq!(xp.u64().unwrap(), 90);
    let mut attribute = Reader::new(&messages[1]);
    assert_eq!(
        attribute.u32().unwrap(),
        opcode::GameMessageOpcode::PrivateUpdateAttribute.0
    );
    assert_eq!(attribute.take(1).unwrap(), [0]);
    assert_eq!(attribute.u32().unwrap(), 1);
    assert_eq!(attribute.u32().unwrap(), 1);
    assert_eq!(attribute.u32().unwrap(), 100);
    assert_eq!(attribute.u32().unwrap(), 10);
    let recovery = sim.shutdown_recover().unwrap();
    assert!(recovery.failure.is_none());
    assert_eq!(
        recovery.kernel.character(EntityId(1)).unwrap().revision(),
        1
    );
    assert_eq!(
        recovery
            .kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        90
    );
    net.shutdown().unwrap();
}

#[test]
fn stalled_outcome_reader_retains_results_commands_and_character_for_recovery() {
    let binding = binding(10);
    let mut kernel = kernel(binding);
    for sequence in 1..=4 {
        kernel.enqueue(command(binding, sequence)).unwrap();
    }
    let sim = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 1,
            tick_limit: Some(5),
            real_time: false,
        },
        1,
    )
    .unwrap();
    let mut recovery = sim.wait_recover().unwrap();
    assert_eq!(recovery.report.ticks, 5);
    assert!(recovery.failure.is_none());
    while let Some(outcome) = recovery.kernel.take_progression_outcome() {
        recovery.undelivered_outcomes.push(outcome);
    }
    assert_eq!(recovery.undelivered_outcomes.len(), 3);
    assert_eq!(recovery.kernel.queued_commands(), 1);
    assert_eq!(
        recovery
            .kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        70
    );
    recovery.kernel.step().unwrap();
    recovery
        .undelivered_outcomes
        .push(recovery.kernel.take_progression_outcome().unwrap());
    for (index, outcome) in recovery.undelivered_outcomes.iter().enumerate() {
        assert_eq!(outcome.context.sequence, index as u32 + 1);
        assert!(outcome.result.is_ok());
    }
    assert_eq!(
        recovery
            .kernel
            .character(EntityId(1))
            .unwrap()
            .available_experience(),
        60
    );
}

#[test]
fn report_only_shutdown_returns_state_instead_of_discarding_characters() {
    let sim = SimulationWorker::spawn(
        kernel(binding(1)),
        SimulationConfig {
            tick_limit: Some(1),
            real_time: false,
            ..Default::default()
        },
    )
    .unwrap();
    match sim.wait() {
        Err(WorkerError::RecoveryRequired(state)) => assert_eq!(
            state
                .kernel
                .character(EntityId(1))
                .unwrap()
                .available_experience(),
            100
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn invalid_worker_configuration_returns_unsaved_character_ownership() {
    let owner = binding(5);
    let mut kernel = kernel(owner);
    kernel.enqueue(command(owner, 1)).unwrap();
    kernel.step().unwrap();
    match SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 0,
            ..Default::default()
        },
    ) {
        Err(WorkerError::StartupRecovery(recovered)) => {
            assert_eq!(
                recovered.kernel.character(owner.actor).unwrap().revision(),
                1
            );
            assert_eq!(
                recovered
                    .kernel
                    .character(owner.actor)
                    .unwrap()
                    .available_experience(),
                90
            );
            assert_eq!(recovered.kernel.pending_progression_outcomes(), 1);
        }
        _ => panic!("failed startup must return the original kernel"),
    }
}
