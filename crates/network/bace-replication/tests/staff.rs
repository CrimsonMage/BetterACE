use bace_gameplay_api::{ActionContext, CharacterBinding, SessionId, staff::StaffEvent};
use bace_replication::{
    BatchLimits, SequenceKind, Sequences, project_staff_heal, project_staff_text,
};
use bace_types::{AccountId, EntityId};
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
    }
}
fn context() -> ActionContext {
    let b = binding();
    ActionContext {
        actor: b.actor,
        account: b.account,
        session: b.session,
        sequence: 4,
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 3,
        max_bytes: 512,
        max_message_bytes: 256,
        max_string_bytes: 64,
    }
}
#[test]
fn broadcast_uses_source_system_chat_world_broadcast_type_and_bounds() {
    // GameMessageSystemChat: F7E0, WriteString16L (aligned), ChatMessageType.WorldBroadcast=0x14.
    let expected = vec![0xe0, 0xf7, 0, 0, 2, 0, b'h', b'i', 0x14, 0, 0, 0];
    assert_eq!(
        bace_replication::project_staff_broadcast("hi", 12).unwrap(),
        expected
    );
    assert!(bace_replication::project_staff_broadcast("hi", 11).is_err());
    assert!(bace_replication::project_staff_broadcast(&"x".repeat(4097), 8192).is_err());
}
#[test]
fn whole_vital_batch_preflights_before_sequence_changes() {
    let event = StaffEvent::Healed {
        context: context(),
        target: EntityId(1),
        vitals: vec![(2, 100), (4, 200), (6, 300)],
    };
    let mut sequences = Sequences::new(3).unwrap();
    let mut small = limits();
    small.max_messages = 2;
    assert!(project_staff_heal(binding(), &event, &mut sequences, small).is_err());
    assert_eq!(sequences.current(SequenceKind::Vital, 2), 255);
    let batch = project_staff_heal(binding(), &event, &mut sequences, limits()).unwrap();
    assert_eq!(batch.messages.len(), 3);
    for id in [2, 4, 6] {
        assert_eq!(sequences.current(SequenceKind::Vital, id), 0);
    }
    let mut wrong = binding();
    wrong.actor = EntityId(9);
    assert!(project_staff_heal(wrong, &event, &mut sequences, limits()).is_err());
}
#[test]
fn inspection_output_preserves_order_and_exact_requester_binding() {
    let event = StaffEvent::Inspection {
        context: context(),
        target: EntityId(2),
        lines: vec!["first".into(), "second".into()],
    };
    let batch = project_staff_text(binding(), &event, limits()).unwrap();
    assert_eq!(batch.messages.len(), 2);
    let mut wrong = binding();
    wrong.session = SessionId(4);
    assert!(project_staff_text(wrong, &event, limits()).is_err());
    let mut small = limits();
    small.max_messages = 1;
    assert!(project_staff_text(binding(), &event, small).is_err());
}

#[test]
fn ace_heal_nonplayer_rejection_uses_private_broadcast_chat() {
    // AdminCommands.HandleHeal sends GameMessageSystemChat to the invoking
    // session with ChatMessageType.Broadcast when wo is not a Player.
    let event = StaffEvent::Inspection {
        context: context(),
        target: EntityId(2),
        lines: vec!["You cannot heal Drudge because it is not a player.".into()],
    };
    let mut bounds = limits();
    bounds.max_string_bytes = 128;
    let batch = project_staff_text(binding(), &event, bounds).unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(batch.messages[0].queue, 9);
    let bytes = &batch.messages[0].bytes;
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 0xf7e0);
    assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
    assert!(
        bytes
            .windows(b"You cannot heal Drudge because it is not a player.".len())
            .any(|window| window == b"You cannot heal Drudge because it is not a player.")
    );
}
