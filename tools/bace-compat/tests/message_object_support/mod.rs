use bace_wire::*;
use serde_json::Value;
pub fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../fixtures/messages.json")).unwrap()
}
pub fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
pub fn bytes(value: &Value) -> Vec<u8> {
    hex(value["bytes"].as_str().unwrap())
}
pub fn model(value: &Value) -> ObjectModel {
    let palettes = value["SubPalettes"].as_array().unwrap();
    ObjectModel {
        palette_id: (!palettes.is_empty()).then(|| value["PaletteID"].as_u64().unwrap() as u32),
        palettes: palettes
            .iter()
            .map(|p| ModelPalette {
                palette_id: p["SubPaletteId"].as_u64().unwrap() as u32,
                offset: p["Offset"].as_u64().unwrap() as u8,
                length: p["Length"].as_u64().unwrap() as u8,
            })
            .collect(),
        textures: value["TextureChanges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| ModelTexture {
                part_index: p["PartIndex"].as_u64().unwrap() as u8,
                old_texture: p["OldTexture"].as_u64().unwrap() as u32,
                new_texture: p["NewTexture"].as_u64().unwrap() as u32,
            })
            .collect(),
        parts: value["AnimPartChanges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| ModelPart {
                part_index: p["Index"].as_u64().unwrap() as u8,
                animation_id: p["AnimationId"].as_u64().unwrap() as u32,
            })
            .collect(),
    }
}
pub fn restrictions() -> ObjectRestrictions {
    ObjectRestrictions {
        open: true,
        monarch_id: 0x50000004,
        permissions: vec![
            RestrictionPermission {
                object_id: 0x50000021,
                permissions: 1,
            },
            RestrictionPermission {
                object_id: 0x50000020 + 89,
                permissions: 1,
            },
            RestrictionPermission {
                object_id: 0x50000020,
                permissions: 0,
            },
        ],
    }
}
pub fn game(flags: u32, flags2: u32) -> ObjectGameData {
    ObjectGameData {
        name: "Café object".into(),
        class_id: 0x123456,
        icon_id: 0x06008000,
        item_type: 0x12345678,
        description_flags: 0x10,
        options: ObjectGameOptions {
            plural_name: (flags & 1 != 0).then(|| "Café objects".into()),
            item_capacity: (flags & 2 != 0).then_some(7),
            container_capacity: (flags & 4 != 0).then_some(8),
            value: (flags & 8 != 0).then_some(123456),
            usable: (flags & 0x10 != 0).then_some(0x12345678),
            use_radius: (flags & 0x20 != 0).then_some(1.25),
            monarch: (flags & 0x40 != 0).then_some(0x50000004),
            ui_effects: (flags & 0x80 != 0).then_some(0xf00),
            ammo_type: (flags & 0x100 != 0).then_some(0x1122),
            combat_use: (flags & 0x200 != 0).then_some(-2),
            structure: (flags & 0x400 != 0).then_some(0x1234),
            max_structure: (flags & 0x800 != 0).then_some(0x2345),
            stack_size: (flags & 0x1000 != 0).then_some(0x3456),
            max_stack_size: (flags & 0x2000 != 0).then_some(0x4567),
            container: (flags & 0x4000 != 0).then_some(0x80000001),
            wielder: (flags & 0x8000 != 0).then_some(0x50000002),
            valid_locations: (flags & 0x10000 != 0).then_some(0x12345678),
            wielded_location: (flags & 0x20000 != 0).then_some(0x23456789),
            clothing_priority: (flags & 0x40000 != 0).then_some(0x34567890),
            target_type: (flags & 0x80000 != 0).then_some(8),
            radar_color: (flags & 0x100000 != 0).then_some(0xab),
            burden: (flags & 0x200000 != 0).then_some(0x5678),
            spell: (flags & 0x400000 != 0).then_some(0x6789),
            radar_behavior: (flags & 0x800000 != 0).then_some(0xcd),
            workmanship: (flags & 0x1000000 != 0).then_some(2.5),
            house_owner: (flags & 0x2000000 != 0).then_some(0x50000003),
            house_restrictions: (flags & 0x4000000 != 0).then(restrictions),
            script: (flags & 0x8000000 != 0).then_some(0xccdd),
            hook_type: (flags & 0x10000000 != 0).then_some(0xabcd),
            hook_item_types: (flags & 0x20000000 != 0).then_some(0x45678901),
            icon_overlay: (flags & 0x40000000 != 0).then_some(0x06007fff),
            material_type: (flags & 0x80000000 != 0).then_some(0x56789012),
            icon_underlay: (flags2 & 1 != 0).then_some(0x06008000),
            cooldown: (flags2 & 2 != 0).then_some(-123),
            cooldown_duration: (flags2 & 4 != 0).then_some(12.25),
            pet_owner: (flags2 & 8 != 0).then_some(0x50000005),
        },
    }
}
pub fn physics(flags: u32, objects: &Value) -> PhysicsDescription {
    let motion = MovementDescription {
        autonomous: true,
        motion_flags: 3,
        current_style: 61,
        body: MotionBody::State {
            state: InterpretedMotion {
                current_style: Some(61),
                forward_command: Some(objects["run_command"].as_u64().unwrap() as u16),
                forward_speed: Some(1.5),
                commands: vec![MotionCommandItem {
                    raw_command: objects["wave_command"].as_u64().unwrap() as u16,
                    sequence: 0x4567,
                    autonomous: true,
                    speed: 1.25,
                }],
                ..InterpretedMotion::default()
            },
            sticky_object: Some(0x80000001),
        },
    };
    PhysicsDescription {
        state: 0x01020304,
        options: PhysicsOptions {
            movement: if flags & 0x10000 != 0 {
                Some(PhysicsMovement::Motion(motion))
            } else {
                (flags & 0x20000 != 0).then_some(PhysicsMovement::AnimationFrame(8))
            },
            position: (flags & 0x8000 != 0).then_some(WirePosition {
                cell: 0x12340001,
                origin: [1.25, -2.5, 3.75],
                rotation: [0.1, 0.2, 0.3, 0.4],
            }),
            setup: (flags & 1 != 0).then_some(0x02000004),
            motion_table: (flags & 2 != 0).then_some(0x09000001),
            velocity: (flags & 4 != 0).then_some([1.0, 2.0, 3.0]),
            acceleration: (flags & 8 != 0).then_some([4.0, 5.0, 6.0]),
            omega: (flags & 16 != 0).then_some([7.0, 8.0, 9.0]),
            parent: (flags & 32 != 0).then_some(PhysicsParent {
                object_id: 0x50000002,
                location: 19,
            }),
            children: if flags & 64 != 0 {
                vec![
                    PhysicsChild {
                        object_id: 0x80000001,
                        location: 19,
                    },
                    PhysicsChild {
                        object_id: 0x80000002,
                        location: 34,
                    },
                ]
            } else {
                vec![]
            },
            scale: (flags & 128 != 0).then_some(1.25),
            friction: (flags & 256 != 0).then_some(0.5),
            elasticity: (flags & 512 != 0).then_some(0.75),
            sound_table: (flags & 0x800 != 0).then_some(0x20000002),
            physics_table: (flags & 0x1000 != 0).then_some(0x34000003),
            default_script: (flags & 0x2000 != 0).then_some(0x33000001),
            default_script_intensity: (flags & 0x4000 != 0).then_some(1.5),
            translucency: (flags & 0x40000 != 0).then_some(0.25),
        },
        sequences: PhysicsSequences {
            position: 0,
            movement: 0,
            state: 0,
            vector: 0,
            teleport: 0x5566,
            server_control: 0x2345,
            force_position: 0x7788,
            visual_description: 0,
            instance: 0x1122,
        },
    }
}
pub fn limits() -> ObjectCodecLimits {
    ObjectCodecLimits {
        max_message_bytes: 4096,
        max_model_entries: 765,
        max_children: 16,
        max_restrictions: 16,
        max_motion_commands: 16,
        max_string_bytes: 128,
    }
}
