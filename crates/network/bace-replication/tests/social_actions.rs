use bace_gameplay_api::{
    CharacterBinding, SessionId,
    social::{
        AcceptedChat, ChatChannel, ChatDelivery, FellowSnapshot, FellowshipSnapshot, SocialEvent,
    },
};
use bace_replication::{BatchLimits, EventSequencer, SequenceKind, Sequences};
use bace_types::{AccountId, EntityId};
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 4,
        max_bytes: 4096,
        max_message_bytes: 2048,
        max_string_bytes: 256,
    }
}
#[test]
fn accepted_chat_route_recipient_and_property_counters_are_transactional() {
    let b = binding();
    let mut events = EventSequencer::new(b, 9);
    let mut seq = Sequences::new(8).unwrap();
    let chat = SocialEvent::Chat {
        accepted: AcceptedChat {
            sequence: 1,
            unix_seconds: 0,
            sender: EntityId(2),
            sender_name: "Rune".into(),
            channel: ChatChannel::Allegiance,
            text: "hi".into(),
        },
        recipients: vec![b.actor],
        wire: ChatDelivery::Turbine {
            channel: 700,
            chat_type: 1,
        },
    };
    let batch = events
        .project_social_event(b, &chat, &mut seq, limits())
        .unwrap();
    assert_eq!(batch.messages[0].queue, 4);
    assert_eq!(
        u32::from_le_bytes(batch.messages[0].bytes[40..44].try_into().unwrap()),
        700
    );
    assert_eq!(events.next_sequence(), 9);
    let afk = SocialEvent::Afk {
        recipient: b.actor,
        enabled: true,
    };
    let mut small = limits();
    small.max_bytes = 1;
    assert!(
        events
            .project_social_event(b, &afk, &mut seq, small)
            .is_err()
    );
    assert_eq!(seq.current(SequenceKind::PropertyBool, 110), 255);
    events
        .project_social_event(b, &afk, &mut seq, limits())
        .unwrap();
    assert_eq!(seq.current(SequenceKind::PropertyBool, 110), 0);
    assert_eq!(events.next_sequence(), 9);
    let denied = SocialEvent::System {
        recipient: EntityId(99),
        text: "private".into(),
        chat_type: 3,
    };
    assert!(
        events
            .project_social_event(b, &denied, &mut seq, limits())
            .is_err()
    );
}
#[test]
fn whole_fellowship_batch_preserves_event_sequence_on_pressure() {
    let b = binding();
    let mut events = EventSequencer::new(b, 9);
    let mut seq = Sequences::new(8).unwrap();
    let event = SocialEvent::Fellowship {
        recipients: vec![b.actor],
        snapshot: FellowshipSnapshot {
            id: 1,
            revision: 1,
            name: "Rune".into(),
            leader: b.actor,
            members: vec![FellowSnapshot {
                actor: b.actor,
                name: "Leader".into(),
                level: 10,
                maximum: [100; 3],
                current: [99; 3],
            }],
            share_xp: true,
            even_share: true,
            open: false,
            locked: false,
            departed: vec![],
            locks: vec![],
        },
    };
    let mut small = limits();
    small.max_messages = 0;
    assert!(
        events
            .project_social_event(b, &event, &mut seq, small)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
    let batch = events
        .project_social_event(b, &event, &mut seq, limits())
        .unwrap();
    assert_eq!(batch.messages.len(), 1);
    assert_eq!(events.next_sequence(), 10);
}

#[test]
fn transient_and_empty_global_squelch_match_official_message_oracle() {
    let b = CharacterBinding {
        actor: EntityId(0x50000001),
        account: AccountId(2),
        session: SessionId(3),
    };
    for line in include_str!("fixtures/social_policy.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (name, hex) = line.split_once(',').unwrap();
        let expected: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let event = if name == "transient" {
            SocialEvent::Transient {
                recipient: b.actor,
                text: "Hello €".into(),
            }
        } else {
            SocialEvent::Squelches {
                recipient: b.actor,
                entries: vec![],
                global_mask: 0,
            }
        };
        let mut events = EventSequencer::new(b, 42);
        let mut seq = Sequences::new(8).unwrap();
        let batch = events
            .project_social_event(b, &event, &mut seq, limits())
            .unwrap();
        assert_eq!(batch.messages[0].bytes, expected, "{name}");
        assert_eq!(batch.messages[0].queue, 9);
    }
}

#[test]
fn source_mana_depletion_notice_is_one_bounded_message_then_sound_batch() {
    let b = binding();
    let mut events = EventSequencer::new(b, 12);
    let mut seq = Sequences::new(8).unwrap();
    let notice = SocialEvent::EquipmentMana {
        recipient: b.actor,
        name: "Test".into(),
        depleted: true,
    };
    let mut limited = limits();
    limited.max_messages = 1;
    assert!(
        events
            .project_social_event(b, &notice, &mut seq, limited)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 12);
    let batch = events
        .project_social_event(b, &notice, &mut seq, limits())
        .unwrap();
    assert_eq!(batch.messages.len(), 2);
    assert_eq!(
        batch.messages[0].bytes,
        bace_wire::ChatMessage::System {
            text: "Your Test is out of Mana.",
            chat_type: 7
        }
        .encode()
        .unwrap()
    );
    // Pinned GameMessageSound payload: F750, owner GUID, Sound.ItemManaDepleted=97, volume1.
    assert_eq!(
        batch.messages[1].bytes,
        vec![
            0x50, 0xf7, 0, 0, 1, 0, 0, 0, 0x97, 0, 0, 0, 0, 0, 0x80, 0x3f
        ]
    );
    assert_eq!(events.next_sequence(), 12);
}
