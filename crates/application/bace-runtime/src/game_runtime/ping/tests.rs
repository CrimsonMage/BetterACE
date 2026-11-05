use super::*;
use crate::game_runtime::tests::{entered, entered::poll_until, fixture};

use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::EventSequencer;
use bace_types::{AccountId, EntityId};

/// ACE GameEventPingResponse for object 0x50000001 at session sequence 42 and 43,
/// from tests/fixtures/ping.csv (oracle/ping_generate.py).
const ACE_PING_42: &str = "B0F70000010000502A000000EA010000";
const ACE_PING_43: &str = "B0F70000010000502B000000EA010000";

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(7),
        account: AccountId(1),
        actor: EntityId(0x5000_0001),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn ping_responses(
    messages: &[bace_transport::ReceivedMessage],
    start: usize,
    actor: u32,
) -> Vec<u32> {
    messages
        .iter()
        .skip(start)
        .filter(|message| message.queue == 9)
        .filter(|message| {
            message.bytes.get(..4)
                == Some(
                    &bace_wire::opcode::GameMessageOpcode::GameEvent
                        .0
                        .to_le_bytes(),
                )
                && message.bytes.get(12..16)
                    == Some(
                        &bace_wire::opcode::GameEventType::PingResponse
                            .0
                            .to_le_bytes(),
                    )
        })
        .map(|message| {
            assert_eq!(
                message.bytes.get(4..8),
                Some(actor.to_le_bytes().as_slice())
            );
            u32::from_le_bytes(message.bytes[8..12].try_into().unwrap())
        })
        .collect()
}

#[test]
fn ping_gate_table_drops_before_entry_and_blocks_only_for_retained_capacity() {
    let entered = PingState {
        closing: false,
        world_entered: true,
        replication_ready: true,
        output_capacity: true,
    };
    assert_eq!(ping_gate(entered), PingIngress::Accepted);
    assert_eq!(
        ping_gate(PingState {
            closing: true,
            ..entered
        }),
        PingIngress::Dropped
    );
    assert_eq!(
        ping_gate(PingState {
            world_entered: false,
            replication_ready: false,
            ..entered
        }),
        PingIngress::Dropped
    );
    assert_eq!(
        ping_gate(PingState {
            replication_ready: false,
            ..entered
        }),
        PingIngress::Blocked
    );
    assert_eq!(
        ping_gate(PingState {
            output_capacity: false,
            ..entered
        }),
        PingIngress::Blocked
    );
}

#[test]
fn ping_projection_uses_shared_counter_and_matches_ace_bytes() {
    let binding = binding();
    let mut events = EventSequencer::new(binding, 42);
    let first = project_ping_response(&mut events, binding, 4096).unwrap();
    let second = project_ping_response(&mut events, binding, 4096).unwrap();
    assert_eq!(first.messages.len(), 1);
    assert_eq!(first.messages[0].queue, 9);
    assert_eq!(hex(&first.messages[0].bytes), ACE_PING_42);
    assert_eq!(second.messages.len(), 1);
    assert_eq!(hex(&second.messages[0].bytes), ACE_PING_43);
    assert_eq!(events.next_sequence(), 44);
}

#[test]
fn ping_projection_rejects_foreign_binding_without_advancing_counter() {
    let binding = binding();
    let mut events = EventSequencer::new(binding, 42);
    let foreign = CharacterBinding {
        actor: EntityId(0x5000_0002),
        ..binding
    };
    assert!(project_ping_response(&mut events, foreign, 4096).is_err());
    assert_eq!(events.next_sequence(), 42);
}

#[tokio::test]
async fn ping_before_world_entry_is_dropped_without_reply_or_failure() {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    let (_cluster, _directory, mut runtime) = fixture::fixture().await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("driver-ping").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let key = SessionKey {
        id: 9,
        generation: 13,
    };
    runtime.players.authenticated(key, &account).unwrap();
    runtime.sessions.insert(
        key,
        Session {
            account,
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    let request = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::PingRequest,
        payload: &[],
    }
    .encode(1024)
    .unwrap();
    runtime.input.push_back(NetworkEvent::Message {
        key,
        message: bace_transport::ReceivedMessage {
            sequence: 1,
            id: 1,
            queue: 9,
            bytes: request.clone(),
        },
    });
    runtime.input_bytes = request.len();
    runtime.poll_ingress().unwrap();
    assert!(runtime.input.is_empty(), "ping was retained");
    assert!(!runtime.sessions[&key].terminated);
    assert!(runtime.sessions[&key].failure.is_none());
    assert!(
        runtime
            .network_output
            .iter()
            .all(|command| !matches!(command, NetworkCommand::SendBatch { .. })),
        "pre-entry ping was answered"
    );
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn entered_ping_request_receives_exactly_one_ping_response() {
    let mut fixture = Box::pin(entered::fixture()).await;
    let key = fixture.key;
    let actor = fixture.binding.actor.0;
    let start = fixture.client.messages.len();
    let request = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::PingRequest,
        payload: &[],
    }
    .encode(1024)
    .unwrap();
    fixture.client.send_message(&fixture.runtime, key, &request);
    poll_until(&mut fixture.runtime, &mut fixture.client, |_, client| {
        ping_responses(&client.messages, start, actor).len() == 1
    })
    .await;
    fixture.client.send_message(&fixture.runtime, key, &request);
    poll_until(&mut fixture.runtime, &mut fixture.client, |_, client| {
        ping_responses(&client.messages, start, actor).len() == 2
    })
    .await;
    let sequences = ping_responses(&fixture.client.messages, start, actor);
    assert_eq!(sequences.len(), 2, "one reply per request");
    assert_eq!(sequences[1], sequences[0].wrapping_add(1));
    assert_eq!(fixture.runtime.progression_failure(key), None);
}
