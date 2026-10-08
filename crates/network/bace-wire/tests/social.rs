use bace_wire::opcode::GameActionType;
use bace_wire::*;
fn limits() -> SocialCodecLimits {
    SocialCodecLimits {
        max_message_bytes: 1024,
        max_entries: 2,
        max_filters: 4,
        max_string_bytes: 16,
    }
}
fn request() -> Vec<u8> {
    let mut writer = Writer::new();
    for value in [0xf7de, u32::MAX, 3, 2, 1, 0, 0, 0, 0, u32::MAX, 42, 2, 2, 2] {
        writer.u32(value);
    }
    writer.bytes(&[1]);
    writer.u16(b'x'.into());
    for value in [12, 0xdeadbeef, 0, 2] {
        writer.u32(value);
    }
    writer.into_bytes()
}
#[test]
fn turbine_headers_and_packed_string_lengths_cannot_control_allocation_or_escape_limits() {
    let bytes = request();
    assert!(TurbineChatRequest::decode(&bytes, bytes.len(), 1).is_ok());
    assert_eq!(
        TurbineChatRequest::decode(&bytes, bytes.len() - 1, 1),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        TurbineChatRequest::decode(&bytes, bytes.len(), 0),
        Err(WireError::LimitExceeded)
    );
    for length in 0..40 {
        assert!(TurbineFrame::decode(&bytes[..length], 100).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[56] = 0xff;
    corrupt[57] = 0xff;
    assert_eq!(
        TurbineChatRequest::decode(&corrupt, 100, 255),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        TurbineChatRequest::decode(&corrupt, 100, 32767),
        Err(WireError::Truncated)
    );
    let mut corrupt = bytes;
    corrupt[57..59].copy_from_slice(&0xdc00u16.to_le_bytes());
    assert_eq!(
        TurbineChatRequest::decode(&corrupt, 100, 255),
        Err(WireError::InvalidEncoding)
    );
    assert!(
        TurbineChatRequest::decode(&TurbineChatResponse { context_id: 42 }.encode(), 100, 255)
            .is_err()
    );
}
#[test]
fn turbine_output_limits_do_not_emit_truncated_utf16_prefixes() {
    let mut event = TurbineChatEvent {
        channel: 2,
        sender_name: "Sender".into(),
        text: "🙂".into(),
        sender_id: 1,
        chat_type: 2,
    };
    assert_eq!(event.encode(1024, 1), Err(WireError::LimitExceeded));
    let valid = event.encode(1024, 2).unwrap();
    assert_eq!(
        event.encode(valid.len() - 1, 2),
        Err(WireError::LimitExceeded)
    );
    event.text = "x".repeat(256);
    assert_eq!(event.encode(1024, 1024), Err(WireError::LimitExceeded));
    event.text = "".into();
    event.sender_name = "n".repeat(128);
    assert_eq!(event.encode(1024, 1024), Err(WireError::LimitExceeded));
}
#[test]
fn social_actions_bound_utf8_units_and_require_complete_strings() {
    assert!(SocialRequest::decode(GameActionType::Talk, &[1, 0, 0xff, 0], 4, 16).is_err());
    assert!(SocialRequest::decode(GameActionType::Talk, &[1, 0, b'a'], 4, 16).is_err());
    assert_eq!(
        SocialRequest::decode(GameActionType::Talk, &[1, 0, b'a', 0], 4, 0),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        SocialRequest::decode(GameActionType::Talk, &[1, 0, b'a', 0], 3, 16),
        Err(WireError::LimitExceeded)
    );
    assert!(SocialRequest::decode(GameActionType::Jump, &[], 0, 0).is_err());
}

#[test]
fn pinned_corpse_consent_actions_read_exact_string16l_prefixes() {
    // Official ACE GameAction{Add,Remove}PlayerPermission and
    // GameActionRemoveFromPlayerConsentList each read one String16L.
    let name = [3, 0, b'A', b'd', b'a', 0, 0, 0];
    for (opcode, expected) in [
        (
            GameActionType::AddPlayerPermission,
            SocialAction::AddPlayerPermission("Ada".into()),
        ),
        (
            GameActionType::RemovePlayerPermission,
            SocialAction::RemovePlayerPermission("Ada".into()),
        ),
        (
            GameActionType::RemoveFromPlayerConsentList,
            SocialAction::RemoveFromPlayerConsentList("Ada".into()),
        ),
    ] {
        let decoded = SocialRequest::decode(opcode, &name, name.len(), 16).unwrap();
        assert_eq!(decoded.action, expected);
        assert_eq!(decoded.trailing_bytes, 0);
        assert_eq!(
            SocialRequest::decode(opcode, &name[..name.len() - 1], name.len(), 16),
            Err(WireError::Truncated)
        );
    }
    for (opcode, expected) in [
        (
            GameActionType::DisplayPlayerConsentList,
            SocialAction::DisplayPlayerConsentList,
        ),
        (
            GameActionType::ClearPlayerConsentList,
            SocialAction::ClearPlayerConsentList,
        ),
    ] {
        let decoded = SocialRequest::decode(opcode, &[9], 1, 16).unwrap();
        assert_eq!(decoded.action, expected);
        assert_eq!(decoded.trailing_bytes, 1);
    }
}
#[test]
fn social_event_tables_validate_nested_limits_duplicate_ids_and_delta_cardinality() {
    let friend = FriendsUpdate {
        kind: FriendsUpdateKind::Added,
        friends: vec![],
    };
    assert_eq!(
        SocialEvent::Friends(&friend).encode(1, 2, limits()),
        Err(WireError::InvalidEncoding)
    );
    let info = SquelchInfo {
        filters: vec![0; 5],
        player_name: "".into(),
        account: false,
    };
    let mut database = SquelchDatabase {
        characters: vec![],
        global: info,
    };
    assert_eq!(
        SocialEvent::Squelch(&database).encode(1, 2, limits()),
        Err(WireError::LimitExceeded)
    );
    database.global.filters.clear();
    let entry = SquelchEntry {
        object_id: 3,
        info: database.global.clone(),
    };
    database.characters = vec![entry.clone(), entry.clone()];
    assert_eq!(
        SocialEvent::Squelch(&database).encode(1, 2, limits()),
        Err(WireError::InvalidEncoding)
    );
    database.characters.push(entry);
    assert_eq!(
        SocialEvent::Squelch(&database).encode(1, 2, limits()),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        SocialEvent::ChannelIndex(&vec![vec![]; 4]).encode(1, 2, limits()),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        SocialEvent::Transient("abcdefghijklmnopq").encode(1, 2, limits()),
        Err(WireError::LimitExceeded)
    );
    let mut short = limits();
    short.max_message_bytes = 16;
    assert_eq!(
        SocialEvent::Transient("").encode(1, 2, short),
        Err(WireError::LimitExceeded)
    );
}
