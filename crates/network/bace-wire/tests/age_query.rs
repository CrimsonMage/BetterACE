//! QueryAge (0x1C2) and QueryBirth (0x1C4) payload decoding.
//!
//! Fixture source: retail client `CM_Character::Event_QueryAge` (CM_Character.cpp, lines 497-544)
//! and `CM_Character::Event_QueryBirth` (lines 546-592) write the GameAction opcode as a
//! `u32` (`450` = 0x1C2 and `452` = 0x1C4), then `*(u32*)buf = i_target` in little-endian
//! order, then pad to 4 bytes. The payload after the action opcode is therefore exactly four
//! bytes. The pinned ACE `GameActionQueryAge.cs` reads a `String16L` that it never uses; that
//! is not the retail layout and is not accepted here. GDLE `ClientEvents.cpp` (QUERY_AGE /
//! QUERY_BIRTH) reads one `ReadUInt32`, which matches the client.
use bace_wire::{GameActionEnvelope, TargetQueryInput, opcode::GameActionType};

/// Builds the retail GameAction envelope for an age/birth query: `0xF7B1`, sequence, action, u32.
fn envelope(action: u32, target: u32, sequence: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&0xF7B1u32.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&action.to_le_bytes());
    bytes.extend_from_slice(&target.to_le_bytes());
    bytes
}

#[test]
fn retail_age_and_birth_payloads_decode_one_u32_target() {
    for (action, expected) in [
        (GameActionType::QueryAge, TargetQueryInput::Age(0x1234_5678)),
        (
            GameActionType::QueryBirth,
            TargetQueryInput::Birth(0x1234_5678),
        ),
    ] {
        // Little-endian payload exactly as the client's `*v5 = i_target` writes it.
        let payload = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(
            TargetQueryInput::decode(action, &payload, 32).unwrap(),
            expected
        );
    }
}

#[test]
fn full_envelope_from_client_layout_routes_to_u32_query() {
    for (action, opcode) in [
        (0x01C2u32, GameActionType::QueryAge),
        (0x01C4, GameActionType::QueryBirth),
    ] {
        let target = 0x8000_0001;
        let bytes = envelope(action, target, 7);
        let envelope = GameActionEnvelope::decode(&bytes, 32).unwrap();
        assert_eq!(envelope.action, opcode);
        assert_eq!(envelope.sequence, 7);
        let decoded = TargetQueryInput::decode(envelope.action, envelope.payload, 32).unwrap();
        assert!(matches!(
            decoded,
            TargetQueryInput::Age(v) | TargetQueryInput::Birth(v) if v == target
        ));
    }
}

#[test]
fn truncated_age_and_birth_payloads_are_rejected() {
    for action in [GameActionType::QueryAge, GameActionType::QueryBirth] {
        for length in 0..4 {
            assert!(
                TargetQueryInput::decode(action, &[0xAA; 4][..length], 32).is_err(),
                "{action:?} length {length} must fail"
            );
        }
    }
}

#[test]
fn oversized_age_and_birth_payloads_are_rejected() {
    for action in [GameActionType::QueryAge, GameActionType::QueryBirth] {
        // Trailing bytes after the single u32 are not a retail shape.
        for length in 5..=12 {
            assert!(
                TargetQueryInput::decode(action, &vec![0u8; length], 32).is_err(),
                "{action:?} length {length} must fail"
            );
        }
        // Over the caller's bound is rejected before any read.
        assert!(TargetQueryInput::decode(action, &[0; 4], 3).is_err());
    }
}

#[test]
fn health_and_item_mana_readers_are_unchanged() {
    assert_eq!(
        TargetQueryInput::decode(GameActionType::QueryHealth, &[1, 0, 0, 0], 32).unwrap(),
        TargetQueryInput::Health(1)
    );
    assert_eq!(
        TargetQueryInput::decode(GameActionType::QueryItemMana, &[2, 0, 0, 0], 32).unwrap(),
        TargetQueryInput::ItemMana(2)
    );
}
