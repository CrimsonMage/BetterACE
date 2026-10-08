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
fn failed_batches_preserve_event_counter_and_generation_fences() {
    let mut seq = EventSequencer::new(binding(), u32::MAX);
    let invalid = CombatEvent::KillerNotification("🦀");
    assert!(
        seq.project_combat(
            binding(),
            &[CombatEvent::CommenceAttack, invalid],
            batch_limits()
        )
        .is_err()
    );
    assert_eq!(seq.next_sequence(), u32::MAX);
    assert_eq!(
        seq.project_combat(
            CharacterBinding {
                session: SessionId(2),
                ..binding()
            },
            &[],
            batch_limits()
        ),
        Err(SessionProjectionError::WrongBinding)
    );
    assert_eq!(
        seq.project_combat(
            binding(),
            &[CombatEvent::CommenceAttack],
            BatchLimits {
                max_bytes: 15,
                ..batch_limits()
            }
        ),
        Err(SessionProjectionError::Limit)
    );
    assert_eq!(seq.next_sequence(), u32::MAX);
    let result = seq
        .project_combat(
            binding(),
            &[CombatEvent::CommenceAttack, CombatEvent::AttackDone(0)],
            batch_limits(),
        )
        .unwrap();
    assert_eq!(seq.next_sequence(), 1);
    assert_eq!(
        GameEventEnvelope::decode(&result.messages[0].bytes, 128)
            .unwrap()
            .sequence,
        u32::MAX
    );
    assert_eq!(
        GameEventEnvelope::decode(&result.messages[1].bytes, 128)
            .unwrap()
            .sequence,
        0
    );
}
#[test]
fn login_is_all_or_nothing_and_advances_only_real_event_envelopes() {
    let mut seq = EventSequencer::new(binding(), 42);
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
    let projection = || LoginProjection {
        description: &description,
        titles: &titles,
        friends: &friends,
        self_object: &self_object,
        possessions: &possessions,
    };
    assert!(
        seq.project_login(
            binding(),
            projection(),
            LoginProjectionLimits {
                batch: BatchLimits {
                    max_bytes: 20,
                    ..batch_limits()
                },
                ..limits()
            }
        )
        .is_err()
    );
    assert_eq!(seq.next_sequence(), 42);
    let batch = seq
        .project_login(binding(), projection(), limits())
        .unwrap();
    assert_eq!(seq.next_sequence(), 46);
    assert_eq!(
        batch.messages.iter().map(|m| m.queue).collect::<Vec<_>>(),
        [9, 9, 9, 10, 10, 10, 9, 10]
    );
    assert_eq!(
        batch
            .messages
            .iter()
            .map(|m| u32::from_le_bytes(m.bytes[..4].try_into().unwrap()))
            .collect::<Vec<_>>(),
        [
            0xf7b0, 0xf7b0, 0xf7b0, 0xf746, 0xf745, 0xf745, 0xf7b0, 0xf745
        ]
    );
}
