use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer};
use bace_types::{AccountId, EntityId};
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 2,
        max_bytes: 4096,
        max_message_bytes: 2048,
        max_string_bytes: 256,
    }
}
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
    }
}
#[test]
fn pinned_remove_then_expire_sound_is_atomic_and_uses_one_event_sequence() {
    let b = binding();
    let mut events = EventSequencer::new(b, 9);
    let mut small = limits();
    small.max_messages = 1;
    assert!(
        events
            .project_enchantment_expiry(b, 10, 2, None, true, small)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
    let out = events
        .project_enchantment_expiry(b, 10, 2, None, true, limits())
        .unwrap();
    // ACE GameEventMagicRemoveEnchantment + GameMessageSound: little-endian envelope/ushort IDs.
    assert_eq!(
        out.messages[0].bytes,
        vec![
            0xb0, 0xf7, 0, 0, 1, 0, 0, 0, 9, 0, 0, 0, 0xc3, 2, 0, 0, 10, 0, 2, 0
        ]
    );
    assert_eq!(out.messages[0].queue, 9);
    assert_eq!(
        out.messages[1].bytes,
        vec![
            0x50, 0xf7, 0, 0, 1, 0, 0, 0, 0x96, 0, 0, 0, 0, 0, 0x80, 0x3f
        ]
    );
    assert_eq!(out.messages[1].queue, 10);
    assert_eq!(events.next_sequence(), 10);
    let cooldown = events
        .project_enchantment_expiry(b, 0x8001, 1, None, false, limits())
        .unwrap();
    assert_eq!(cooldown.messages.len(), 1);
}
#[test]
fn item_expiry_uses_owner_text_and_sound_without_player_registry_removal() {
    let b = binding();
    let mut events = EventSequencer::new(b, 9);
    let out = events
        .project_enchantment_expiry(b, 10, 2, Some(("Strength", "Test Ring")), true, limits())
        .unwrap();
    assert_eq!(out.messages.len(), 2);
    assert_eq!(
        out.messages[0].bytes,
        bace_wire::ChatMessage::System {
            text: "The spell Strength on Test Ring has expired.",
            chat_type: 7
        }
        .encode()
        .unwrap()
    );
    assert_eq!(events.next_sequence(), 9);
    let mut small = limits();
    small.max_string_bytes = 8;
    assert!(
        events
            .project_enchantment_expiry(b, 10, 2, Some(("Strength", "Test Ring")), true, small)
            .is_err()
    );
    assert_eq!(events.next_sequence(), 9);
}
