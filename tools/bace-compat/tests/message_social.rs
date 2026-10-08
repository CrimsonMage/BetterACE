//! Social output goldens and source-derived input-reader comparisons.
use bace_wire::*;
use serde_json::{Value, json};
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
fn bytes(value: &Value) -> Vec<u8> {
    hex(value["bytes"].as_str().unwrap())
}
fn limits() -> SocialCodecLimits {
    SocialCodecLimits {
        max_message_bytes: 4096,
        max_entries: 128,
        max_filters: 8,
        max_string_bytes: 128,
    }
}
fn check(event: SocialEvent<'_>, name: &str, fixture: &Value) {
    assert_eq!(
        event.encode(0x50000001, 42, limits()).unwrap(),
        bytes(&fixture["events"][name]),
        "{name}"
    );
    assert_eq!(fixture["events"][name]["group"], 9);
}

#[test]
fn turbine_events_keep_pinned_plus_eight_lengths_and_refuse_lossy_long_prefixes() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    for vector in social["turbine_events"].as_array().unwrap() {
        let event = TurbineChatEvent {
            channel: 2,
            sender_name: vector["sender_name"].as_str().unwrap().into(),
            text: vector["text"].as_str().unwrap().into(),
            sender_id: 0x50000001,
            chat_type: 2,
        };
        let expected = bytes(&vector["message"]);
        if vector["supported"].as_bool().unwrap() {
            assert_eq!(event.encode(4096, 255).unwrap(), expected);
            let frame = TurbineFrame::decode(&expected, 4096).unwrap();
            assert_eq!(frame.header.blob_type, 1);
            assert_eq!(frame.header.dispatch, 1);
            assert_eq!(frame.header.declared_message_bytes as usize, expected.len());
            assert_eq!(
                frame.header.declared_payload_bytes as usize,
                frame.payload.len() + 8
            );
            assert_eq!(frame.header.target_id, 0x000b00b5);
            assert_eq!(frame.header.transport_id, 0x000b00b5);
        } else {
            assert_eq!(event.encode(4096, 1024), Err(WireError::LimitExceeded));
            if event.sender_name.encode_utf16().count() == 128 {
                assert_eq!(&expected[44..46], &[0x80, 3]);
            } else {
                let prefix = 44 + 1 + event.sender_name.encode_utf16().count() * 2;
                assert_eq!(&expected[prefix..prefix + 2], &[0x80, 0]);
            }
        }
        assert_eq!(vector["message"]["group"], 4);
    }
}
#[test]
fn turbine_response_dispatch_is_by_name_with_by_id_payload_fields() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    for vector in social["turbine_responses"].as_array().unwrap() {
        let context_id = vector["context_id"].as_u64().unwrap() as u32;
        let encoded = TurbineChatResponse { context_id }.encode();
        assert_eq!(encoded, bytes(&vector["message"]));
        let frame = TurbineFrame::decode(&encoded, 56).unwrap();
        assert_eq!(frame.header.blob_type, 5);
        assert_eq!(frame.header.dispatch, 1);
        assert_eq!(frame.header.declared_message_bytes, 56);
        assert_eq!(frame.header.declared_payload_bytes, 24);
        let mut reader = Reader::new(frame.payload);
        assert_eq!(reader.u32().unwrap(), context_id);
        assert_eq!(reader.u32().unwrap(), 2);
        assert_eq!(reader.u32().unwrap(), 2);
        assert_eq!(reader.u32().unwrap(), 0);
    }
}
#[test]
fn turbine_requests_bound_real_bytes_and_match_ignored_size_metadata_and_unicode() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    for vector in social["turbine_requests"].as_array().unwrap() {
        let source = bytes(vector);
        let actual = TurbineChatRequest::decode(&source, 4096, 1024);
        if !vector["valid_utf16"].as_bool().unwrap() {
            assert_eq!(actual, Err(WireError::InvalidEncoding));
            assert_eq!(vector["decoded"]["text"], "�");
            continue;
        }
        let request = actual.unwrap();
        let expected = &vector["decoded"];
        assert_eq!(
            request.header.blob_type,
            expected["blob_type"].as_u64().unwrap() as u32
        );
        assert_eq!(
            request.header.dispatch,
            expected["dispatch"].as_u64().unwrap() as u32
        );
        assert_eq!(
            request.context_id,
            expected["context_id"].as_u64().unwrap() as u32
        );
        assert_eq!(
            request.channel,
            expected["channel"].as_u64().unwrap() as u32
        );
        assert_eq!(request.text, expected["text"].as_str().unwrap());
        assert_eq!(
            request.claimed_sender_id,
            expected["sender_id"].as_u64().unwrap() as u32
        );
        assert_eq!(
            request.chat_type,
            expected["chat_type"].as_u64().unwrap() as u32
        );
        assert_eq!(request.header.declared_message_bytes, u32::MAX);
        assert_eq!(request.header.declared_payload_bytes, 0);
        assert_eq!(
            (
                request.response_id,
                request.method_id,
                request.extra_data_size,
                request.result
            ),
            (2, 2, 12, 0)
        );
        assert_eq!(
            request.trailing_bytes as u64,
            vector["trailing_bytes"].as_u64().unwrap()
        );
        let consumed = source.len() - request.trailing_bytes;
        assert_eq!(consumed as u64, vector["consumed"].as_u64().unwrap());
        for length in 0..consumed {
            assert!(TurbineChatRequest::decode(&source[..length], 4096, 1024).is_err());
        }
    }
}
#[test]
fn sixteen_social_action_layouts_match_original_handlers_or_verbatim_reader_prefixes() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    for vector in social["actions"].as_array().unwrap() {
        let source = bytes(vector);
        let envelope = GameActionEnvelope::decode(&source, 256).unwrap();
        let request = SocialRequest::decode(envelope.action, envelope.payload, 256, 128).unwrap();
        assert_eq!(
            request.trailing_bytes as u64,
            vector["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            (source.len() - request.trailing_bytes) as u64,
            vector["consumed"].as_u64().unwrap()
        );
        let captured = match &request.action {
            SocialAction::Talk(text)
            | SocialAction::Emote(text)
            | SocialAction::SoulEmote(text)
            | SocialAction::SetAfkMessage(text) => json!({"text":text}),
            SocialAction::Tell { text, target_name } => {
                json!({"text":text,"target_name":target_name})
            }
            SocialAction::TalkDirect { text, target_id } => {
                json!({"text":text,"target_id":target_id})
            }
            SocialAction::ChatChannel { channel, text } => json!({"channel":channel,"text":text}),
            SocialAction::SetAfkMode(enabled) => json!({"enabled":enabled}),
            SocialAction::AddFriend(name) => {
                assert_eq!(name, "  Élodie  ");
                json!({"name":name.trim()})
            }
            SocialAction::RemoveFriend(object_id) => json!({"object_id":object_id}),
            SocialAction::RemoveAllFriends => json!({"empty":true}),
            SocialAction::AddChannel(channel) | SocialAction::RemoveChannel(channel) => {
                json!({"channel":channel})
            }
            SocialAction::ModifyGlobalSquelch {
                enabled,
                message_type,
            } => json!({"enabled":enabled,"message_type":message_type}),
            SocialAction::ModifyCharacterSquelch {
                enabled,
                object_id,
                name,
                message_type,
            } => {
                json!({"enabled":enabled,"object_id":object_id,"name":name,"message_type":message_type})
            }
            SocialAction::ModifyAccountSquelch { enabled, name } => {
                json!({"enabled":enabled,"name":name})
            }
            SocialAction::ClearPlayerConsentList | SocialAction::DisplayPlayerConsentList => {
                json!({"empty":true})
            }
            SocialAction::RemoveFromPlayerConsentList(name)
            | SocialAction::AddPlayerPermission(name)
            | SocialAction::RemovePlayerPermission(name) => json!({"name":name}),
        };
        assert_eq!(captured, vector["captured"]);
        let consumed = envelope.payload.len() - request.trailing_bytes;
        for length in 0..consumed {
            assert!(
                SocialRequest::decode(envelope.action, &envelope.payload[..length], 256, 128)
                    .is_err()
            );
        }
    }
}
#[test]
fn tell_channel_and_transient_events_match_original_serializers() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    check(
        SocialEvent::Tell {
            text: "Hello €",
            sender_name: "Élodie",
            sender_id: 0x50000002,
            target_id: 0x50000001,
            chat_type: 3,
        },
        "tell",
        social,
    );
    check(
        SocialEvent::ChannelBroadcast {
            channel: 2,
            sender_name: "Élodie",
            text: "Hello €",
        },
        "channel_broadcast",
        social,
    );
    check(SocialEvent::Transient("Hello €"), "transient", social);
    check(
        SocialEvent::TurbineChannels {
            allegiance: 0x12345678,
            society: 0x23456789,
        },
        "turbine_channels",
        social,
    );
    let names = vec!["Élodie".into(), "Hidden friend".into()];
    check(SocialEvent::ChannelList(&names), "channel_list", social);
    let admin = vec![
        "Abuse", "Admin", "Audit", "Av1", "Av2", "Av3", "Sentinel", "Help",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    let sentinel = vec!["Abuse", "Audit", "Av1", "Av2", "Av3", "Sentinel", "Help"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    let advocate = vec!["Abuse", "Av1", "Av2", "Av3", "Help"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    for (name, lists) in [
        ("none", vec![]),
        ("admin", vec![admin.clone()]),
        ("sentinel", vec![sentinel.clone()]),
        ("advocate", vec![advocate.clone()]),
        ("overlap", vec![admin, sentinel, advocate]),
    ] {
        check(
            SocialEvent::ChannelIndex(&lists),
            &format!("channel_index_{name}"),
            social,
        );
    }
}
#[test]
fn friend_full_delta_and_status_updates_match_projected_source_behavior() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    for (name, kind, friends) in [
        (
            "friends_full",
            FriendsUpdateKind::Full,
            vec![
                FriendEntry {
                    object_id: 0x50000003,
                    online: false,
                    name: "Hidden friend".into(),
                },
                FriendEntry {
                    object_id: 0x50000002,
                    online: true,
                    name: "Élodie".into(),
                },
                FriendEntry {
                    object_id: 0x50000004,
                    online: false,
                    name: "Offline".into(),
                },
                FriendEntry {
                    object_id: 0x500000ff,
                    online: false,
                    name: "".into(),
                },
            ],
        ),
        ("friends_empty", FriendsUpdateKind::Full, vec![]),
        (
            "friend_added",
            FriendsUpdateKind::Added,
            vec![FriendEntry {
                object_id: 0x50000004,
                online: true,
                name: "Offline".into(),
            }],
        ),
        (
            "friend_removed",
            FriendsUpdateKind::Removed,
            vec![FriendEntry {
                object_id: 0x500000ff,
                online: false,
                name: "".into(),
            }],
        ),
        (
            "friend_status",
            FriendsUpdateKind::StatusChanged,
            vec![FriendEntry {
                object_id: 0x50000003,
                online: true,
                name: "Hidden friend".into(),
            }],
        ),
    ] {
        check(
            SocialEvent::Friends(&FriendsUpdate { kind, friends }),
            name,
            social,
        );
    }
}
#[test]
fn squelch_tables_preserve_empty_account_section_bucket_order_and_filter_counts() {
    let fixture = fixtures();
    let social = &fixture["vectors"]["social"];
    let mut db = SquelchDatabase {
        characters: vec![
            SquelchEntry {
                object_id: 0x50000021,
                info: SquelchInfo {
                    filters: vec![0x12345678; 4],
                    player_name: "Élodie".into(),
                    account: false,
                },
            },
            SquelchEntry {
                object_id: 0x50000020 + 32,
                info: SquelchInfo {
                    filters: vec![1, 2],
                    player_name: "Account alias".into(),
                    account: true,
                },
            },
            SquelchEntry {
                object_id: 0x50000020,
                info: SquelchInfo {
                    filters: vec![],
                    player_name: "Offline".into(),
                    account: false,
                },
            },
        ],
        global: SquelchInfo {
            filters: vec![64],
            player_name: "".into(),
            account: false,
        },
    };
    check(SocialEvent::Squelch(&db), "squelch", social);
    db.characters.clear();
    db.global.filters.clear();
    check(SocialEvent::Squelch(&db), "squelch_empty", social);
}
