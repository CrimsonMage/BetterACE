use bace_content::{Property, SparseProperties, WeenieV1};
use bace_runtime::player_entry::{EntryObjectState, PreparedEntryModel, prepare_entry_object};
use bace_wire::{
    MotionBody, MovementDescription, ObjectModel, PhysicsChild, PhysicsMovement, PhysicsParent,
    PhysicsSequences, WirePosition,
};
mod treasure_table_support;
fn prop<T>(kind: &str, name: &str, value: T) -> Property<T> {
    Property {
        id: bace_loot::ace_tables::enum_value(kind, name)
            .unwrap_or_else(|| panic!("missing {kind}.{name}")) as u32,
        value,
    }
}
fn source() -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "fixture".into(),
        weenie_type: 1,
        last_modified: None,
        properties: SparseProperties::default(),
    }
}
fn state() -> EntryObjectState {
    EntryObjectState {
        is_player: false,
        is_creature: false,
        physics_state: 0x404410,
        position: None,
        movement: None,
        parent: None,
        children: vec![],
        velocity: [0.; 3],
        acceleration: [0.; 3],
        omega: [0.; 3],
        sequences: PhysicsSequences {
            position: 1,
            movement: 2,
            state: 3,
            vector: 4,
            teleport: 5,
            server_control: 6,
            force_position: 7,
            visual_description: 8,
            instance: 9,
        },
        admin_vision: false,
        change_no_draw: false,
        cloak_status: 0,
    }
}
#[test]
fn original_csharp_header_and_physics_presence_flags() {
    treasure_table_support::install_tables();
    for line in include_str!("fixtures/entry_object.csv")
        .lines()
        .filter(|v| !v.starts_with('#'))
    {
        let row = line
            .split(',')
            .map(|v| v.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let c = row[0];
        let mut w = source();
        let mut live = state();
        if c != 0 {
            for name in [
                "ItemsCapacity",
                "ContainersCapacity",
                "AmmoType",
                "Value",
                "ItemUseable",
                "TargetType",
                "UiEffects",
                "CombatUse",
                "Structure",
                "MaxStructure",
                "StackSize",
                "MaxStackSize",
                "ValidLocations",
                "CurrentWieldedLocation",
                "ClothingPriority",
                "RadarBlipColor",
                "ShowableOnRadar",
                "EncumbranceVal",
                "HookType",
                "MaterialType",
                "SharedCooldown",
                "HookItemType",
                "Placement",
            ] {
                w.properties
                    .ints
                    .push(prop("PropertyInt", name, if c == 2 { 0 } else { 9 }));
            }
            for name in ["Container", "Wielder", "HouseOwner", "Monarch", "PetOwner"] {
                w.properties.instance_ids.push(prop(
                    "PropertyInstanceId",
                    name,
                    if c == 2 { 0 } else { 9 },
                ));
            }
            for name in ["Spell", "IconOverlay", "IconUnderlay", "PhysicsScript"] {
                w.properties.data_ids.push(prop(
                    "PropertyDataId",
                    name,
                    if c == 2 { 0 } else { 9 },
                ));
            }
            for (name, value) in [
                ("MotionTable", 1),
                ("SoundTable", 2),
                ("PhysicsEffectTable", 3),
                ("Setup", 4),
            ] {
                w.properties
                    .data_ids
                    .push(prop("PropertyDataId", name, value));
            }
            for name in [
                "UseRadius",
                "DefaultScale",
                "Friction",
                "Elasticity",
                "Translucency",
                "PhysicsScriptIntensity",
                "CooldownDuration",
            ] {
                w.properties.floats.push(prop(
                    "PropertyFloat",
                    name,
                    if c == 2 { 0.0 } else { 1.5 },
                ));
            }
            for name in [
                "Locked",
                "Inscribable",
                "Stuck",
                "HiddenAdmin",
                "UiHidden",
                "IgnoreHouseBarriers",
                "RequiresBackpackSlot",
                "Retained",
                "WieldOnUse",
                "AutowieldLeft",
                "Attackable",
            ] {
                w.properties.bools.push(prop("PropertyBool", name, true));
            }
            w.properties
                .strings
                .push(prop("PropertyString", "PluralName", "Things".to_owned()));
            if c != 2 {
                w.properties
                    .ints
                    .push(prop("PropertyInt", "ItemWorkmanship", 9));
                w.properties
                    .ints
                    .push(prop("PropertyInt", "NumItemsInMaterial", 6));
            }
            live.parent = Some(PhysicsParent {
                object_id: if c == 2 { 0 } else { 9 },
                location: if c == 2 { 0 } else { 9 },
            });
        }
        if c == 3 {
            for (name, value) in [
                ("DefaultScale", 0.0009),
                ("Translucency", -0.0009),
                ("CooldownDuration", 0.0009),
            ] {
                w.properties
                    .floats
                    .iter_mut()
                    .find(|p| p.id == prop("PropertyFloat", name, 0).id)
                    .unwrap()
                    .value = value;
            }
            w.properties
                .ints
                .iter_mut()
                .find(|p| p.id == 19)
                .unwrap()
                .value = -9;
            w.properties
                .instance_ids
                .iter_mut()
                .find(|p| p.id == 3)
                .unwrap()
                .value = 0;
        }
        if c == 4 {
            w.weenie_type = 10;
            live.is_player = true;
            live.is_creature = true;
        }
        if c == 5 {
            w.weenie_type = 55;
            live.is_creature = true;
        }
        if c == 6 {
            w.weenie_type = 11;
            live.cloak_status = 3;
            w.properties
                .ints
                .push(prop("PropertyInt", "PlayerKillerStatus", 4));
        }
        if c == 7 {
            w.weenie_type = 20;
            w.properties
                .bools
                .iter_mut()
                .find(|p| p.id == prop("PropertyBool", "Locked", false).id)
                .unwrap()
                .value = false;
            w.properties
                .ints
                .push(prop("PropertyInt", "PlayerKillerStatus", 64));
            live.velocity = [1., 2., 3.];
            live.position = Some(WirePosition {
                cell: 0x12340001,
                origin: [1., 2., 3.],
                rotation: [1., 0., 0., 0.],
            });
            live.children.push(PhysicsChild {
                object_id: 2,
                location: 1,
            });
            live.movement = Some(PhysicsMovement::Motion(MovementDescription {
                autonomous: false,
                motion_flags: 0,
                current_style: 0,
                body: MotionBody::State {
                    state: bace_wire::InterpretedMotion::default(),
                    sticky_object: None,
                },
            }));
        }
        let d = prepare_entry_object(
            1,
            &w,
            PreparedEntryModel {
                model: ObjectModel::default(),
                icon_override: None,
            },
            live,
        )
        .unwrap();
        let (first, second) = d.game.options.header_flags();
        let desc = d.game.description_flags | if second != 0 { 0x04000000 } else { 0 };
        assert_eq!(
            [first, second, desc, d.physics.options.flags()],
            row[1..],
            "source case {c}"
        );
    }
}
#[test]
fn accepted_state_and_admin_view_are_explicit() {
    treasure_table_support::install_tables();
    let mut w = source();
    w.properties.positions.push(Property {
        id: 1,
        value: bace_content::Position {
            obj_cell_id: 0xffffffff,
            position_x: f32::NAN,
            ..Default::default()
        },
    });
    let mut live = state();
    live.physics_state = 0x100020;
    live.is_player = true;
    live.admin_vision = true;
    live.cloak_status = 2;
    live.change_no_draw = true;
    let d = prepare_entry_object(
        1,
        &w,
        PreparedEntryModel {
            model: ObjectModel::default(),
            icon_override: Some(0x06000055),
        },
        live,
    )
    .unwrap();
    assert!(d.physics.options.position.is_none());
    assert_eq!(d.physics.state, 0);
    assert_eq!(d.physics.options.translucency, Some(0.5));
    assert_eq!(d.physics.sequences.instance, 9);
    assert_eq!(d.game.icon_id, 0x06000055);
}
#[test]
fn original_csharp_attachment_locations_and_private_item_resting() {
    treasure_table_support::install_tables();
    for line in include_str!("fixtures/entry_attachments.csv")
        .lines()
        .filter(|v| !v.starts_with('#'))
    {
        let v = line
            .split(',')
            .map(|n| n.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let mut item = source();
        item.properties
            .ints
            .push(prop("PropertyInt", "ItemType", v[1] as i32));
        item.properties
            .ints
            .push(prop("PropertyInt", "DefaultCombatStyle", v[2] as i32));
        let (children, attached) =
            bace_runtime::player_entry::prepare_entry_attachments(1, &[(2, &item, v[0])], false)
                .unwrap();
        if v[0] & 0x03700000 != 0 {
            assert_eq!(attached[0].placement, v[3]);
            assert_eq!(attached[0].parent.unwrap().location, v[4]);
            assert_eq!(children[0].location, v[4]);
        } else {
            assert!(children.is_empty());
            assert!(attached[0].parent.is_none());
            assert_eq!(attached[0].placement, 101);
        }
    }
    let mut bow = source();
    bow.weenie_type = 3;
    let ammo = source();
    let (children, items) = bace_runtime::player_entry::prepare_entry_attachments(
        1,
        &[(2, &bow, 0x00400000), (3, &ammo, 0x00800000)],
        true,
    )
    .unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(items[1].parent.unwrap().location, 0);
    assert_eq!(items[1].placement, 0);
    assert!(
        bace_runtime::player_entry::prepare_entry_attachments(
            1,
            &[(2, &bow, 1), (2, &ammo, 2)],
            false
        )
        .is_err()
    );
}
