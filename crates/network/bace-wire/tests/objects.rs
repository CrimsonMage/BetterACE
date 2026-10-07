use bace_wire::*;
fn sequences() -> PhysicsSequences {
    PhysicsSequences {
        position: 0,
        movement: 0,
        state: 0,
        vector: 0,
        teleport: 0,
        server_control: 0,
        force_position: 0,
        visual_description: 0,
        instance: 1,
    }
}
fn description() -> ObjectDescription {
    ObjectDescription {
        object_id: 1,
        model: ObjectModel::default(),
        physics: PhysicsDescription {
            state: 0x400,
            options: PhysicsOptions::default(),
            sequences: sequences(),
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
fn limits() -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: 1024,
        max_model_entries: 765,
        max_children: 2,
        max_restrictions: 2,
        max_motion_commands: 2,
        max_string_bytes: 128,
    }
}
#[test]
fn model_counts_palette_presence_and_typed_ids_are_checked() {
    let mut model = ObjectModel {
        parts: vec![
            ModelPart {
                part_index: 0,
                animation_id: 0x01000001
            };
            256
        ],
        ..ObjectModel::default()
    };
    assert_eq!(model.encode(1000), Err(WireError::LimitExceeded));
    model.parts.truncate(1);
    assert_eq!(model.encode(0), Err(WireError::LimitExceeded));
    model.parts[0].animation_id = 0x05000001;
    assert_eq!(model.encode(1), Err(WireError::InvalidEncoding));
    model.parts.clear();
    model.palette_id = Some(0x04000001);
    assert_eq!(model.encode(1), Err(WireError::InvalidEncoding));
    model.palette_id = None;
    model.palettes.push(ModelPalette {
        palette_id: 0x04000002,
        offset: 0,
        length: 0,
    });
    assert_eq!(model.encode(1), Err(WireError::InvalidEncoding));
    assert!(ObjectModel::decode(&[0x10, 0, 0, 0], 0).is_err());
    assert_eq!(
        ObjectModel::decode(&[0x11, 255, 255, 255], 10),
        Err(WireError::LimitExceeded)
    );
    assert!(ObjectModel::decode(&[0x11, 0, 0, 0, 0], 0).is_err());
    assert!(ObjectModel::decode(&[0x11, 1, 0, 0, 0, 0], 765).is_err());
}
#[test]
fn restrictions_bound_wire_count_and_reject_duplicate_ids() {
    let mut restrictions = ObjectRestrictions {
        open: false,
        monarch_id: 0,
        permissions: vec![
            RestrictionPermission {
                object_id: 3,
                permissions: 0,
            },
            RestrictionPermission {
                object_id: 3,
                permissions: 1,
            },
        ],
    };
    assert_eq!(restrictions.encode(1), Err(WireError::LimitExceeded));
    assert_eq!(restrictions.encode(2), Err(WireError::InvalidEncoding));
    restrictions.permissions = vec![
        RestrictionPermission {
            object_id: 3,
            permissions: 0
        };
        65536
    ];
    assert_eq!(restrictions.encode(65536), Err(WireError::LimitExceeded));
}
#[test]
fn object_message_limits_cover_nested_children_commands_strings_and_final_size() {
    let mut object = description();
    let bounded = limits();
    let good = object.encode_create(bounded).unwrap();
    let mut short = bounded;
    short.max_message_bytes = good.len() - 1;
    assert_eq!(object.encode_create(short), Err(WireError::LimitExceeded));
    short.max_message_bytes = good.len();
    assert_eq!(object.encode_create(short).unwrap(), good);
    object.physics.options.children = vec![
        PhysicsChild {
            object_id: 2,
            location: 3
        };
        3
    ];
    assert_eq!(object.encode_update(bounded), Err(WireError::LimitExceeded));
    object.physics.options.children.clear();
    object.physics.options.movement = Some(PhysicsMovement::Motion(MovementDescription {
        autonomous: false,
        motion_flags: 0,
        current_style: 0,
        body: MotionBody::State {
            state: InterpretedMotion {
                commands: vec![
                    MotionCommandItem {
                        raw_command: 7,
                        sequence: 0,
                        autonomous: false,
                        speed: 1.0
                    };
                    3
                ],
                ..InterpretedMotion::default()
            },
            sticky_object: None,
        },
    }));
    assert_eq!(object.encode_create(bounded), Err(WireError::LimitExceeded));
    object.physics.options.movement = None;
    object.game.name = "a".repeat(129);
    assert_eq!(object.encode_create(bounded), Err(WireError::LimitExceeded));
    object.game.name = "🙂".into();
    assert_eq!(
        object.encode_create(bounded),
        Err(WireError::InvalidEncoding)
    );
    object.game.name = "Synthetic".into();
    object.game.icon_id = 0x05000001;
    assert_eq!(
        object.encode_create(bounded),
        Err(WireError::InvalidEncoding)
    );
    object.game.icon_id = 0x06000001;
    object.game.class_id = u32::MAX;
    assert_eq!(object.encode_create(bounded), Err(WireError::InvalidLength));
}
#[test]
fn secondary_header_and_physics_movement_alternatives_are_explicit() {
    let mut object = description();
    object.game.options.cooldown_duration = Some(0.0);
    let (first, second) = object.game.options.header_flags();
    assert_eq!((first, second), (0, 4));
    let encoded = object.game.encode(128, 2).unwrap();
    let mut reader = Reader::new(&encoded);
    assert_eq!(reader.u32().unwrap(), 0);
    reader.string16(128).unwrap();
    reader.packed_u32().unwrap();
    reader.packed_u32().unwrap();
    reader.u32().unwrap();
    assert_eq!(reader.u32().unwrap() & 0x04000000, 0x04000000);
    reader.take((4 - reader.position() % 4) % 4).unwrap();
    assert_eq!(reader.u32().unwrap(), 4);
    assert_eq!(reader.f64().unwrap(), 0.0);
    object.physics.options.movement = Some(PhysicsMovement::AnimationFrame(0));
    assert_eq!(object.physics.options.flags(), 0x20000);
}
