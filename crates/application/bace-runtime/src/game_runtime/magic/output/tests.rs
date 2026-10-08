//! Production lifecycle fixture; source spell, inventory, motion and geometry
//! remain native. The fixture uses real PostgreSQL and reliable UDP transport.
use super::*;
use crate::game_runtime::tests::entered;

fn action(id: u32, fields: &[u32]) -> Vec<u8> {
    let mut writer = bace_wire::Writer::new();
    writer.u32(bace_wire::opcode::GameMessageOpcode::GameAction.0);
    writer.u32(1);
    writer.u32(id);
    for field in fields {
        writer.u32(*field);
    }
    writer.into_bytes()
}

fn use_done(message: &bace_transport::ReceivedMessage) -> Option<u32> {
    let bytes = &message.bytes;
    if bytes.get(..4)
        != Some(
            &bace_wire::opcode::GameMessageOpcode::GameEvent
                .0
                .to_le_bytes(),
        )
        || bytes.get(12..16) != Some(&bace_wire::opcode::GameEventType::UseDone.0.to_le_bytes())
    {
        return None;
    }
    bytes
        .get(16..20)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn native_player_cast_finishes_and_old_incarnation_vitals_are_discarded() {
    let mut fixture = Box::pin(entered::fixture()).await;
    let actor = fixture.binding.actor;
    let key = fixture.key;
    let loading = fixture.runtime.sessions[&key].loading.as_ref().unwrap();
    let table = loading.spell_table.as_ref().unwrap();
    let spell =
        loading
            .loaded
            .player
            .player
            .entity
            .state
            .properties
            .spell_book
            .iter()
            .filter_map(|row| u32::try_from(row.id).ok())
            .find(|id| {
                table.spells.get(id).is_some_and(|base| {
                    base.school == 2 && base.meta_type == 3 && base.flags & 8 != 0
                }) && fixture
                    .runtime
                    .assets
                    .spell_rows
                    .get(id)
                    .is_some_and(|row| row.boost.is_some_and(|boost| boost > 0))
            })
            .expect("native Life-trained creation grants a beneficial self boost");
    fixture
        .client
        .send_message(&fixture.runtime, key, &action(0x53, &[8]));
    let start = fixture.client.messages.len();
    fixture
        .client
        .send_message(&fixture.runtime, key, &action(0x4a, &[actor.0, spell]));
    entered::poll_until(
        &mut fixture.runtime,
        &mut fixture.client,
        |runtime, client| {
            !runtime.magic_ingress_blocked(key)
                && client
                    .messages
                    .iter()
                    .skip(start)
                    .any(|message| use_done(message).is_some())
        },
    )
    .await;
    assert!(fixture.runtime.magic.failures.is_empty());
    let results: Vec<_> = fixture
        .client
        .messages
        .iter()
        .skip(start)
        .filter_map(use_done)
        .collect();
    assert_eq!(
        results,
        [0],
        "native ordinary cast must complete successfully"
    );
    assert!(
        fixture
            .runtime
            .players
            .replication(actor)
            .unwrap()
            .vital_revisions[2]
            .is_some(),
        "accepted cast mana reaches the canonical output revision fence"
    );

    // A delayed output from a previous incarnation must not update the new
    // recipient, even when its scalar revision would otherwise be newer.
    let before = fixture
        .runtime
        .players
        .replication(actor)
        .unwrap()
        .vital_revisions;
    let queued = fixture.runtime.network_output.len();
    assert!(
        fixture
            .runtime
            .project_magic_event(&MagicEvent::Vital {
                actor,
                incarnation: key.generation.checked_add(1).unwrap(),
                vital: bace_entity::EntityVital::Mana,
                before: 1,
                after: 0,
                revision: u64::MAX,
            })
            .unwrap()
    );
    assert_eq!(
        fixture
            .runtime
            .players
            .replication(actor)
            .unwrap()
            .vital_revisions,
        before
    );
    assert_eq!(fixture.runtime.network_output.len(), queued);

    // Malformed unaccepted ingress must close this peer without turning a
    // recoverable protocol rejection into a permanent durability failure.
    let malformed = bace_transport::ReceivedMessage {
        sequence: u32::MAX,
        id: 1,
        queue: 9,
        bytes: action(0x4a, &[]),
    };
    assert!(matches!(
        fixture
            .runtime
            .handle_magic_message(key, &malformed)
            .unwrap(),
        MagicIngress::Accepted
    ));
    assert!(fixture.runtime.sessions[&key].terminated);
    assert!(fixture.runtime.sessions[&key].failure.is_none());
    fixture.shutdown().await;
}
