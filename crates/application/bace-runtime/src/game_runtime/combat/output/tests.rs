use super::*;
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_types::AccountId;
#[test]
fn physical_done_commence_preserve_source_order_and_counters_under_pressure_and_reentry() {
    let key = SessionKey {
        id: 1,
        generation: 9,
    };
    let binding = CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(9),
    };
    let mut replica = crate::player_service::SessionReplication {
        key,
        binding,
        events: bace_replication::EventSequencer::new(binding, 1),
        properties: bace_replication::Sequences::new(16).unwrap(),
        item_properties: Default::default(),
        public_physics_state: None,
        vital_revisions: [None; 3],
    };
    let mut output = VecDeque::new();
    assert!(
        queue_private_notice(
            &mut replica,
            9,
            bace_wire::CombatEvent::AttackDone(0),
            &mut output,
            1,
            4096
        )
        .unwrap()
    );
    let sequence = replica.events.next_sequence();
    assert!(
        !queue_private_notice(
            &mut replica,
            9,
            bace_wire::CombatEvent::CommenceAttack,
            &mut output,
            1,
            4096
        )
        .unwrap()
    );
    assert_eq!(replica.events.next_sequence(), sequence);
    // A delayed event from the old admission is terminal even while the current
    // peer queue is full; it cannot consume the new session's sequence.
    assert!(
        queue_private_notice(
            &mut replica,
            8,
            bace_wire::CombatEvent::AttackDone(0),
            &mut output,
            1,
            4096
        )
        .unwrap()
    );
    assert_eq!(replica.events.next_sequence(), sequence);
    let first = output.pop_front().unwrap();
    assert!(
        queue_private_notice(
            &mut replica,
            9,
            bace_wire::CombatEvent::CommenceAttack,
            &mut output,
            1,
            4096
        )
        .unwrap()
    );
    let second = output.pop_front().unwrap();
    let NetworkCommand::SendOrderedBatch {
        key: sent,
        messages: first,
    } = first
    else {
        panic!("ordered source notification")
    };
    let NetworkCommand::SendOrderedBatch {
        messages: second, ..
    } = second
    else {
        panic!("ordered source repeat")
    };
    assert_eq!(sent, key);
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(
        u32::from_le_bytes(first[0].1[8..12].try_into().unwrap()) + 1,
        u32::from_le_bytes(second[0].1[8..12].try_into().unwrap())
    );
    assert_eq!(
        u32::from_le_bytes(first[0].1[12..16].try_into().unwrap()),
        0x1a7
    );
    assert_eq!(
        u32::from_le_bytes(second[0].1[12..16].try_into().unwrap()),
        0x1b8
    );
}
