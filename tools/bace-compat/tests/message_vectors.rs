//! Golden bytes produced by unmodified pinned official C# serializers.
use bace_wire::opcode::*;
use bace_wire::*;
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
fn check(name: &str, actual: &[u8], group: GameMessageGroup) {
    let data = fixtures();
    let vector = &data["vectors"]["messages"][name];
    assert_eq!(actual, hex(vector["bytes"].as_str().unwrap()), "{name}");
    assert_eq!(
        u64::from(group.0),
        vector["group"].as_u64().unwrap(),
        "{name} group"
    );
}
fn ui(name: &str, actual: &[u8]) {
    check(name, actual, GameMessageGroup::UIQueue);
}
fn db(name: &str, actual: &[u8]) {
    check(name, actual, GameMessageGroup::DatabaseQueue);
}

#[test]
fn every_official_identifier_and_alias_is_preserved() {
    let data = fixtures();
    assert_eq!(data["commit"], "47edade3bd3f6044b676d4eb877c4965c7eda62b");
    let catalogs = &data["vectors"]["catalogs"];
    let collections = [
        (
            "messages",
            GameMessageOpcode::NAMED
                .iter()
                .map(|(name, id)| (*name, id.0))
                .collect::<Vec<_>>(),
        ),
        (
            "actions",
            GameActionType::NAMED
                .iter()
                .map(|(name, id)| (*name, id.0))
                .collect(),
        ),
        (
            "events",
            GameEventType::NAMED
                .iter()
                .map(|(name, id)| (*name, id.0))
                .collect(),
        ),
        (
            "character_errors",
            CharacterError::NAMED
                .iter()
                .map(|(name, id)| (*name, id.0))
                .collect(),
        ),
        (
            "groups",
            GameMessageGroup::NAMED
                .iter()
                .map(|(name, id)| (*name, id.0))
                .collect(),
        ),
    ];
    for (catalog, entries) in collections {
        assert_eq!(entries.len(), catalogs[catalog].as_object().unwrap().len());
        for (name, id) in entries {
            assert_eq!(
                catalogs[catalog][name].as_u64().unwrap(),
                u64::from(id),
                "{catalog}:{name}"
            );
        }
    }
    assert_eq!(
        GameMessageOpcode::CharacterRestoreResponse,
        GameMessageOpcode::CharacterCreateResponse
    );
    assert_eq!(GameMessageOpcode::Motion, GameMessageOpcode::MovementEvent);
}

#[test]
fn character_messages_match_official_serializers() {
    for (name, error) in CharacterError::NAMED {
        ui(
            &format!("error_{name}"),
            &CharacterReply::Error(*error).encode().unwrap(),
        );
    }
    for (name, reply) in [
        (
            "created",
            CharacterReply::Created {
                object_id: 0x50000001,
                name: "Élodie".into(),
            },
        ),
        ("create_failed", CharacterReply::CreateFailed(3)),
        (
            "restored",
            CharacterReply::Restored {
                object_id: 0x50000002,
                name: "Restored".into(),
                seconds_disabled: 3600,
            },
        ),
        ("deleted", CharacterReply::Deleted),
        ("logged_off", CharacterReply::LoggedOff),
        ("world_ready", CharacterReply::WorldServerReady),
    ] {
        ui(name, &reply.encode().unwrap());
    }
    let mut list = CharacterList {
        characters: vec![
            CharacterListEntry {
                object_id: 0x50000001,
                name: "Élodie".into(),
                seconds_disabled: 0,
            },
            CharacterListEntry {
                object_id: 0x50000002,
                name: "Former".into(),
                seconds_disabled: 10,
            },
        ],
        slot_count: 11,
        account: "synthetic-account".into(),
        use_turbine_chat: true,
        has_throne_of_destiny: true,
    };
    ui("character_list", &list.encode(11).unwrap());
    let data = fixtures();
    let bytes = hex(data["vectors"]["messages"]["character_list"]["bytes"]
        .as_str()
        .unwrap());
    assert_eq!(CharacterList::decode(&bytes, 11, 128).unwrap(), list);
    list.characters.clear();
    ui("empty_character_list", &list.encode(11).unwrap());
    ui(
        "server_name",
        &ServerName {
            name: "BACE €",
            current_connections: 3,
            max_connections: -1,
        }
        .encode()
        .unwrap(),
    );
}

#[test]
fn chat_and_boot_optional_strings_match_official_serializers() {
    let entries = [
        (
            "system_chat",
            ChatMessage::System {
                text: "Hello €",
                chat_type: 7,
            },
        ),
        (
            "speech",
            ChatMessage::Speech {
                text: "Hello €",
                sender_name: "Élodie",
                sender_id: 0x50000001,
                chat_type: 2,
            },
        ),
        (
            "ranged_speech",
            ChatMessage::RangedSpeech {
                text: "Hello €",
                sender_name: "Élodie",
                sender_id: 0x50000001,
                chat_type: 2,
                range: 12.5,
            },
        ),
        (
            "emote",
            ChatMessage::Emote {
                sender_id: 0x50000001,
                sender_name: "Élodie",
                text: "waves",
            },
        ),
        (
            "soul_emote",
            ChatMessage::SoulEmote {
                sender_id: 0x50000001,
                sender_name: "Élodie",
                text: "waves",
            },
        ),
    ];
    for (name, message) in entries {
        ui(name, &message.encode().unwrap());
    }
    for (name, reason) in [
        ("boot_null", None),
        ("boot_empty", Some("")),
        ("boot_reason", Some(" because maintenance")),
    ] {
        ui(name, &AccountControl::Boot { reason }.encode().unwrap());
    }
}

#[test]
fn ddd_output_matches_official_database_messages() {
    db(
        "ddd_interrogation",
        &DddControl::Interrogation {
            allow_highres: false,
        }
        .encode(),
    );
    db(
        "ddd_interrogation_highres",
        &DddControl::Interrogation {
            allow_highres: true,
        }
        .encode(),
    );
    db("ddd_end", &DddControl::End.encode());
    db(
        "ddd_error",
        &DddControl::Error {
            resource_type: 7,
            object_id: 0x12345678,
            error_type: 1,
        }
        .encode(),
    );
    let begin = DddBegin {
        total_file_size: 12345,
        iterations: vec![
            DddIteration {
                database: DddDatabase::Cell,
                iteration: 8,
                files: vec![0x1234fffe, 0x1234ffff],
            },
            DddIteration {
                database: DddDatabase::HighRes,
                iteration: 9,
                files: vec![0x06000001],
            },
            DddIteration {
                database: DddDatabase::Language,
                iteration: 7,
                files: vec![0x25000001],
            },
            DddIteration {
                database: DddDatabase::Portal,
                iteration: 6,
                files: vec![0x01000001, 0x02000002],
            },
            DddIteration {
                database: DddDatabase::Portal,
                iteration: 5,
                files: vec![0x03000003],
            },
        ],
    };
    db("ddd_begin", &begin.encode(5, 7).unwrap());
    for (database, name) in [
        (DddDatabase::Portal, "Portal"),
        (DddDatabase::Cell, "Cell"),
        (DddDatabase::Language, "Language"),
        (DddDatabase::HighRes, "HighRes"),
    ] {
        db(
            &format!("ddd_data_{name}"),
            &DddData {
                database,
                resource_type: 7,
                object_id: 0x12345678,
                iteration: 19,
                compressed: false,
                data: &[1, 2, 3, 4, 5],
            }
            .encode(5)
            .unwrap(),
        );
    }
    // Fixture checks compression FLAG/layout only. Compression is an asset-service responsibility.
    db(
        "ddd_data_compressed",
        &DddData {
            database: DddDatabase::Portal,
            resource_type: 7,
            object_id: 0x12345678,
            iteration: 19,
            compressed: true,
            data: &[1, 2, 3, 4, 5],
        }
        .encode(5)
        .unwrap(),
    );
}

#[test]
fn official_iteration_run_decoder_matches_without_expanding_runs() {
    for vector in fixtures()["vectors"]["iteration_sets"].as_array().unwrap() {
        let mut bytes = Writer::new();
        bytes.u32(GameMessageOpcode::DDD_InterrogationResponse.0);
        bytes.u32(1);
        bytes.bytes(&hex(vector["bytes"].as_str().unwrap()));
        bytes.bytes(&[0; 8]); // ignored second-list/flags trailer, as in ACE handler.
        let response = DddInterrogationResponse::decode(&bytes.into_bytes(), 4, 100, 100).unwrap();
        assert_eq!(response.trailing_bytes, 8);
        assert_eq!(response.client_language, 1);
        let set = &response.with_keys[0];
        assert_eq!(
            i64::from(set.dat_file_type),
            vector["type"].as_i64().unwrap()
        );
        assert_eq!(i64::from(set.dat_file_id), vector["id"].as_i64().unwrap());
        assert_eq!(
            u64::from(set.iterations),
            vector["iterations"].as_u64().unwrap()
        );
        assert_eq!(
            set.runs,
            vector["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as i32)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn official_event_envelopes_include_object_and_sequence_even_when_payload_empty() {
    for (name, event, sequence, object_id) in [
        (
            "fellow_update_done",
            SimpleGameEvent::FellowshipFellowUpdateDone,
            42,
            0x50000001,
        ),
        (
            "weenie_error",
            SimpleGameEvent::WeenieError(0x1234),
            43,
            0x50000001,
        ),
        ("use_done", SimpleGameEvent::UseDone(0x36), 44, 0x50000001),
        (
            "ping_response",
            SimpleGameEvent::PingResponse,
            45,
            0x50000001,
        ),
        ("ping_without_player", SimpleGameEvent::PingResponse, 46, 0),
    ] {
        let bytes = event.encode(object_id, sequence);
        ui(name, &bytes);
        let data = fixtures();
        let official = hex(data["vectors"]["messages"][name]["bytes"].as_str().unwrap());
        let envelope = GameEventEnvelope::decode(&official, 4).unwrap();
        assert_eq!(
            (envelope.object_id, envelope.sequence),
            (object_id, sequence)
        );
        assert_eq!(envelope.encode(4).unwrap(), official);
    }
}

#[test]
fn official_property_messages_preserve_public_string_field_order_and_alignment() {
    let values = [
        ("int", PropertyValue::Int(-1234)),
        ("int64", PropertyValue::Int64(-1234567890123)),
        ("bool", PropertyValue::Bool(true)),
        ("float", PropertyValue::Float(-12.25)),
        ("string", PropertyValue::String("Café €")),
        ("dataid", PropertyValue::DataId(0x06001234)),
        ("instanceid", PropertyValue::InstanceId(0x50000002)),
    ];
    for (name, value) in values {
        for (scope, object_id) in [("private", None), ("public", Some(0x50000001))] {
            ui(
                &format!("{scope}_{name}"),
                &PropertyUpdate {
                    sequence: 0x7f,
                    object_id,
                    property: 42,
                    value: value.clone(),
                }
                .encode()
                .unwrap(),
            );
        }
    }
}

#[test]
fn official_progression_messages_preserve_widths() {
    ui(
        "attribute",
        &AttributeUpdate {
            sequence: 0x7f,
            attribute: 2,
            ranks: 3,
            starting_value: 4,
            experience_spent: 5,
        }
        .encode(),
    );
    for (name, object_id) in [("private_vital", None), ("public_vital", Some(0x50000001))] {
        ui(
            name,
            &VitalUpdate {
                sequence: 0x7f,
                object_id,
                vital: 2,
                ranks: 3,
                starting_value: 4,
                experience_spent: 5,
                current: 6,
            }
            .encode(),
        );
    }
    ui(
        "skill",
        &SkillUpdate {
            sequence: 0x7f,
            skill: 2,
            ranks: 3,
            advancement_class: 4,
            experience_spent: 5,
            initial_level: 6,
            resistance_at_last_check: 7,
            last_used_time: 12.25,
        }
        .encode(),
    );
    ui(
        "current_vital",
        &CurrentVitalUpdate {
            sequence: 0x7f,
            vital: 2,
            current: 6,
        }
        .encode(),
    );
}

#[test]
fn action_envelopes_match_official_client_message_and_action_dispatch_decoder() {
    for vector in fixtures()["vectors"]["actions"].as_array().unwrap() {
        let bytes = hex(vector["bytes"].as_str().unwrap());
        let decoded = GameActionEnvelope::decode(&bytes, 128).unwrap();
        assert_eq!(
            u64::from(decoded.sequence),
            vector["sequence"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(decoded.action.0),
            vector["action"].as_u64().unwrap()
        );
        assert_eq!(decoded.payload, hex(vector["payload"].as_str().unwrap()));
        assert_eq!(vector["consumed"].as_u64().unwrap(), 12);
        assert_eq!(
            vector["opcode"].as_u64().unwrap(),
            u64::from(GameMessageOpcode::GameAction.0)
        );
        assert_eq!(decoded.encode(128).unwrap(), bytes);
    }
}

#[test]
fn banned_message_uses_explicit_remaining_seconds_and_optional_reason() {
    for (name, reason) in [
        ("ban_null", None),
        ("ban_empty", Some("")),
        ("ban_reason", Some(" because synthetic policy")),
    ] {
        ui(
            name,
            &AccountControl::Banned {
                seconds_remaining: 3600,
                reason,
            }
            .encode()
            .unwrap(),
        );
    }
}
