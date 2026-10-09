use bace_wire::opcode::{GameActionType, GameEventType, GameMessageOpcode};
use bace_wire::*;

#[test]
fn envelopes_bound_borrowed_payloads_and_reject_wrong_family_and_truncated_header() {
    let bytes = GameActionEnvelope {
        sequence: 1,
        action: GameActionType::Talk,
        payload: &[1, 2, 3, 4],
    }
    .encode(4)
    .unwrap();
    assert_eq!(
        GameActionEnvelope::decode(&bytes, 3),
        Err(WireError::LimitExceeded)
    );
    assert!(GameEventEnvelope::decode(&bytes, 16).is_err());
    for length in 0..12 {
        assert!(GameActionEnvelope::decode(&bytes[..length], 4).is_err());
    }
    let event = GameEventEnvelope {
        object_id: 3,
        sequence: u32::MAX,
        event: GameEventType(0xffffffff),
        payload: &[1],
    };
    assert_eq!(event.encode(0), Err(WireError::LimitExceeded));
    let bytes = event.encode(1).unwrap();
    for length in 0..16 {
        assert!(GameEventEnvelope::decode(&bytes[..length], 1).is_err());
    }
    assert_eq!(GameEventEnvelope::decode(&bytes, 1).unwrap(), event);
}

#[test]
fn character_list_rejects_count_bombs_truncation_strings_and_trailing_data() {
    let list = CharacterList {
        characters: vec![CharacterListEntry {
            object_id: 1,
            name: "Synthetic".into(),
            seconds_disabled: 0,
        }],
        slot_count: 11,
        account: "Account".into(),
        use_turbine_chat: true,
        has_throne_of_destiny: true,
    };
    let bytes = list.encode(1).unwrap();
    for length in 0..bytes.len() {
        assert!(
            CharacterList::decode(&bytes[..length], 11, 100).is_err(),
            "length {length}"
        );
    }
    assert_eq!(list.encode(0), Err(WireError::LimitExceeded));
    assert_eq!(
        CharacterList::decode(&bytes, 0, 100),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        CharacterList::decode(&bytes, 11, 0),
        Err(WireError::LimitExceeded)
    );
    let mut bomb = bytes.clone();
    bomb[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(CharacterList::decode(&bomb, usize::MAX, 100).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        CharacterList::decode(&trailing, 11, 100),
        Err(WireError::InvalidLength)
    );
    let mut invalid_bool = bytes;
    let index = invalid_bool.len() - 4;
    invalid_bool[index..].copy_from_slice(&2u32.to_le_bytes());
    assert_eq!(
        CharacterList::decode(&invalid_bool, 11, 100),
        Err(WireError::InvalidEncoding)
    );
    assert_eq!(
        CharacterReply::CreateFailed(1).encode(),
        Err(WireError::InvalidEncoding)
    );
}

fn response(iterations: u32, runs: &[i32]) -> Vec<u8> {
    let mut writer = Writer::new();
    for word in [
        GameMessageOpcode::DDD_InterrogationResponse.0,
        1,
        1,
        0,
        1,
        iterations,
    ] {
        writer.u32(word);
    }
    for run in runs {
        writer.u32(*run as u32);
    }
    writer.into_bytes()
}
#[test]
fn iteration_decoder_rejects_overflow_overshoot_unbounded_no_progress_and_count_bombs() {
    // ACE loops until equality and Math.Abs(MIN) throws. Harden without expanding runs.
    for (iterations, runs) in [
        (1, vec![i32::MIN]),
        (1, vec![-3]),
        (u32::MAX, vec![]),
        (1, vec![-1, -1, -1, 1]),
    ] {
        assert!(DddInterrogationResponse::decode(&response(iterations, &runs), 4, 100, 3).is_err());
    }
    let bytes = response(2, &[1, 2]);
    for length in 0..bytes.len() {
        assert!(DddInterrogationResponse::decode(&bytes[..length], 4, 100, 3).is_err());
    }
    assert!(DddInterrogationResponse::decode(&bytes, 0, 100, 3).is_err());
    assert!(DddInterrogationResponse::decode(&bytes, 4, 1, 3).is_err());
    let mut bytes = bytes;
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(DddInterrogationResponse::decode(&bytes, usize::MAX, u32::MAX, usize::MAX).is_err());
}

#[test]
fn ddd_limits_are_checked_before_emitting_output_and_requests_are_exact() {
    let begin = DddBegin {
        total_file_size: 12,
        iterations: vec![DddIteration {
            database: DddDatabase::Portal,
            iteration: 1,
            files: vec![1, 2],
        }],
    };
    assert_eq!(begin.encode(0, 2), Err(WireError::LimitExceeded));
    assert_eq!(begin.encode(1, 1), Err(WireError::LimitExceeded));
    let data = DddData {
        database: DddDatabase::Portal,
        resource_type: 1,
        object_id: 2,
        iteration: 3,
        compressed: false,
        data: &[0; 2],
    };
    assert_eq!(data.encode(1), Err(WireError::LimitExceeded));
    let mut writer = Writer::new();
    for word in [GameMessageOpcode::DDD_RequestDataMessage.0, 7, 0x12345678] {
        writer.u32(word);
    }
    let mut bytes = writer.into_bytes();
    assert_eq!(
        DddRequestData::decode(&bytes).unwrap(),
        DddRequestData {
            resource_type: 7,
            object_id: 0x12345678
        }
    );
    for length in 0..bytes.len() {
        assert!(DddRequestData::decode(&bytes[..length]).is_err());
    }
    bytes.push(0);
    assert!(DddRequestData::decode(&bytes).is_err());
}

#[test]
fn ddd_client_end_accepts_only_the_pinned_opcode_only_message() {
    // GameMessageDDDEndDDD at the ACE pin writes no fields after 0xF7EA.
    let end = GameMessageOpcode::DDD_EndDDD.0.to_le_bytes();
    assert_eq!(DddControl::decode_end(&end), Ok(()));
    for length in 0..end.len() {
        assert!(DddControl::decode_end(&end[..length]).is_err());
    }
    let mut trailing = end.to_vec();
    trailing.push(0);
    assert_eq!(
        DddControl::decode_end(&trailing),
        Err(WireError::InvalidLength)
    );
    assert_eq!(
        DddControl::decode_end(&GameMessageOpcode::DDD_Interrogation.0.to_le_bytes()),
        Err(WireError::UnexpectedOpcode(
            GameMessageOpcode::DDD_Interrogation.0
        ))
    );
}

#[test]
fn progression_truncation_limits_and_attribute_alias_hardening() {
    for action in [
        GameActionType::RaiseAttribute,
        GameActionType::RaiseVital,
        GameActionType::RaiseSkill,
        GameActionType::TrainSkill,
    ] {
        for length in 0..8 {
            assert!(ProgressionRequest::decode(action, &vec![0; length], 16).is_err());
        }
        assert_eq!(
            ProgressionRequest::decode(action, &[0; 8], 7),
            Err(WireError::LimitExceeded)
        );
    }
    assert!(ProgressionRequest::decode(GameActionType::Talk, &[0; 8], 8).is_err());
    let mut payload = [0; 8];
    payload[..4].copy_from_slice(&0x10001u32.to_le_bytes());
    assert_eq!(
        ProgressionRequest::decode(GameActionType::RaiseAttribute, &payload, 8),
        Err(WireError::InvalidEncoding)
    );
    assert_eq!(
        ProgressionRequest::decode(GameActionType::RaiseVital, &payload, 8),
        Err(WireError::InvalidEncoding)
    );
    assert!(ProgressionRequest::decode(GameActionType::RaiseSkill, &payload, 8).is_ok());
}

#[test]
fn client_string16_preserves_utf16_units_and_rejects_invalid_utf8() {
    // Two UTF-16 units, four UTF-8 bytes, no padding: server CP1252 is not used here.
    assert_eq!(
        Reader::new(&[2, 0, 0xf0, 0x9f, 0x99, 0x82])
            .client_string16(2)
            .unwrap(),
        "🙂"
    );
    assert!(
        Reader::new(&[1, 0, 0xf0, 0x9f, 0x99, 0x82, 0])
            .client_string16(2)
            .is_err()
    );
    assert!(Reader::new(&[1, 0, 0xff, 0]).client_string16(2).is_err());
    assert!(Reader::new(&[2, 0, b'a', b'b']).client_string16(1).is_err());
}
