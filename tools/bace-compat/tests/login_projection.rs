use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::*;
use bace_types::{AccountId, EntityId};
use bace_wire::*;
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(1),
        account: AccountId(2),
        actor: EntityId(0x50000001),
    }
}
fn batch_limits() -> BatchLimits {
    BatchLimits {
        max_messages: 16,
        max_bytes: 8192,
        max_message_bytes: 4096,
        max_string_bytes: 128,
    }
}
fn limits() -> LoginProjectionLimits {
    LoginProjectionLimits {
        batch: batch_limits(),
        description: PlayerDescriptionLimits {
            max_table_entries: 64,
            max_string_bytes: 128,
            max_gameplay_options_bytes: 1024,
            max_message_bytes: 4096,
        },
        objects: ObjectCodecLimits {
            max_message_bytes: 4096,
            max_model_entries: 255,
            max_children: 16,
            max_restrictions: 16,
            max_motion_commands: 16,
            max_string_bytes: 128,
        },
        social: SocialCodecLimits {
            max_message_bytes: 4096,
            max_entries: 16,
            max_filters: 16,
            max_string_bytes: 128,
        },
        max_titles: 16,
        max_container_items: 16,
    }
}
fn object(id: u32) -> ObjectDescription {
    ObjectDescription {
        object_id: id,
        model: ObjectModel::default(),
        physics: PhysicsDescription {
            state: 0x400,
            options: PhysicsOptions::default(),
            sequences: PhysicsSequences {
                position: 0,
                movement: 0,
                state: 0,
                vector: 0,
                teleport: 0,
                server_control: 0,
                force_position: 0,
                visual_description: 0,
                instance: 1,
            },
        },
        game: ObjectGameData {
            name: "Synthetic".into(),
            class_id: 1,
            icon_id: 0x06000001,
            item_type: 1,
            description_flags: 0,
            options: ObjectGameOptions::default(),
        },
    }
}
#[test]
fn actual_login_projection_matches_extracted_official_send_self_order() {
    let self_object = object(binding().actor.0);
    let bag = object(0x80000001);
    let equipped = object(0x80000002);
    let description = PlayerDescription::default();
    let titles = CharacterTitle {
        current: 0,
        titles: vec![],
    };
    let friends = FriendsUpdate {
        kind: FriendsUpdateKind::Full,
        friends: vec![],
    };
    let possessions = [
        LoginPossession::Create(&bag),
        LoginPossession::Contents {
            container_id: bag.object_id,
            items: &[],
        },
        LoginPossession::Create(&equipped),
    ];
    let mut sequence = EventSequencer::new(binding(), 42);
    let batch = sequence
        .project_login(
            binding(),
            LoginProjection {
                description: &description,
                titles: &titles,
                friends: &friends,
                self_object: &self_object,
                possessions: &possessions,
            },
            limits(),
        )
        .unwrap();
    let observed: Vec<_> = batch
        .messages
        .iter()
        .map(|m| {
            let opcode = u32::from_le_bytes(m.bytes[..4].try_into().unwrap());
            let event_sequence = if opcode == 0xf7b0 {
                Some(GameEventEnvelope::decode(&m.bytes, 4096).unwrap().sequence)
            } else {
                None
            };
            serde_json::json!({"opcode":opcode,"group":m.queue,"event_sequence":event_sequence})
        })
        .collect();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap();
    assert_eq!(
        serde_json::json!(observed),
        fixture["vectors"]["login_order"]
    );
}
