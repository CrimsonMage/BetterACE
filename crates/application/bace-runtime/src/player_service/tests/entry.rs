use bace_replication::*;
use bace_wire::*;
fn batch_limits() -> BatchLimits {
    BatchLimits {
        max_messages: 16,
        max_bytes: 8192,
        max_message_bytes: 4096,
        max_string_bytes: 128,
    }
}
pub(super) fn limits() -> LoginProjectionLimits {
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
pub(super) fn object(id: u32) -> ObjectDescription {
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
