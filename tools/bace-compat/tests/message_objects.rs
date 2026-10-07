//! Object bytes from the pinned ACE serializer bodies and unchanged wrappers.
mod message_object_support;
use bace_wire::*;
use message_object_support::*;

#[test]
fn all_model_list_presence_combinations_and_byte_count_boundary_match_ace() {
    let fixtures = fixtures();
    let objects = &fixtures["vectors"]["objects"];
    for vector in objects["models"].as_array().unwrap() {
        let value = model(&vector["model"]);
        let expected = bytes(vector);
        assert_eq!(
            value.encode(765).unwrap(),
            expected,
            "mode {}",
            vector["mode"]
        );
        assert_eq!(ObjectModel::decode(&expected, 765).unwrap(), value);
        for length in 0..expected.len() {
            assert!(ObjectModel::decode(&expected[..length], 765).is_err());
        }
    }
}
#[test]
fn every_physics_conditional_field_and_embedded_motion_match_ace() {
    let fixtures = fixtures();
    let objects = &fixtures["vectors"]["objects"];
    for vector in objects["physics"].as_array().unwrap() {
        let flags = vector["flags"].as_u64().unwrap() as u32;
        let value = physics(flags, objects);
        assert_eq!(value.options.flags(), flags);
        assert_eq!(
            value.encode(16, 16).unwrap(),
            bytes(vector),
            "flags {flags:#x}"
        );
    }
}
#[test]
fn all_weenie_header_bits_and_second_header_fields_match_ace_width_and_order() {
    let fixtures = fixtures();
    let objects = &fixtures["vectors"]["objects"];
    for vector in objects["game"].as_array().unwrap() {
        let flags = vector["flags"].as_u64().unwrap() as u32;
        let flags2 = vector["flags2"].as_u64().unwrap() as u32;
        let value = game(flags, flags2);
        assert_eq!(value.options.header_flags(), (flags, flags2));
        let mut expected = 0x50000001u32.to_le_bytes().to_vec();
        expected.extend(value.encode(128, 16).unwrap());
        assert_eq!(expected, bytes(vector), "flags {flags:#x}/{flags2:#x}");
    }
    assert_eq!(
        restrictions().encode(16).unwrap(),
        hex(objects["restrictions"].as_str().unwrap())
    );
}
#[test]
fn complete_create_update_and_model_change_preserve_source_composition() {
    let fixtures = fixtures();
    let objects = &fixtures["vectors"]["objects"];
    let full = ObjectDescription {
        object_id: 0x50000001,
        model: model(&objects["models"][7]["model"]),
        physics: physics(0x5fbff, objects),
        game: game(u32::MAX, 15),
    };
    assert_eq!(
        full.encode_create(limits()).unwrap(),
        bytes(&objects["messages"]["create"])
    );
    assert_eq!(
        full.encode_update(limits()).unwrap(),
        bytes(&objects["messages"]["update"])
    );
    assert_eq!(
        AppearanceUpdate {
            object_id: full.object_id,
            model: full.model,
            instance_sequence: 0x1122,
            visual_sequence: 0x4568
        }
        .encode(765)
        .unwrap(),
        bytes(&objects["messages"]["appearance"])
    );
}
#[test]
fn fixed_object_controls_match_official_serializers_and_queue_ids() {
    let fixtures = fixtures();
    let objects = &fixtures["vectors"]["objects"];
    for (name, value, queue) in [
        (
            "delete",
            ObjectControl::Delete {
                object_id: 0x50000001,
                instance_sequence: 0x1122,
            },
            10,
        ),
        (
            "player_create",
            ObjectControl::PlayerCreate {
                object_id: 0x50000001,
            },
            10,
        ),
        (
            "set_state",
            ObjectControl::SetState {
                object_id: 0x50000001,
                state: 0x12345678,
                instance_sequence: 0x1122,
                state_sequence: 0x3457,
            },
            10,
        ),
        (
            "parent",
            ObjectControl::Parent {
                parent_id: 0x50000001,
                child_id: 0x50000002,
                parent_location: 19,
                placement: 8,
                instance_sequence: 0x1122,
                position_sequence: 0x3344,
            },
            10,
        ),
        (
            "pickup",
            ObjectControl::Pickup {
                object_id: 0x50000001,
                instance_sequence: 0x1122,
                position_sequence: 0x3344,
            },
            10,
        ),
        (
            "inventory_remove",
            ObjectControl::InventoryRemove {
                object_id: 0x50000001,
            },
            9,
        ),
        (
            "stack",
            ObjectControl::StackSize {
                object_id: 0x50000001,
                sequence: 0x7f,
                stack_size: 0x3456,
                value: 123456,
            },
            9,
        ),
        ("teleport", ObjectControl::Teleport { sequence: 0x5567 }, 10),
    ] {
        assert_eq!(value.encode(), bytes(&objects["messages"][name]), "{name}");
        assert_eq!(objects["messages"][name]["group"].as_u64().unwrap(), queue);
    }
}

#[test]
fn vendor_listing_reuses_exact_game_descriptions_with_default_then_unique_stock() {
    let fixtures = fixtures();
    for vector in fixtures["vectors"]["inventory"]["vendors"]
        .as_array()
        .unwrap()
    {
        let alternate = vector["alternate"].as_bool().unwrap();
        let listing = VendorListing {
            vendor_id: 0x50000020,
            merchandise_types: 0x12345678,
            minimum_value: 1,
            maximum_value: 999999,
            deal_magical: true,
            buy_price: 0.75,
            sell_price: 1.25,
            alternate_currency: alternate.then(|| VendorCurrency {
                class_id: 123,
                amount: 16,
                plural_name: "Trade tokens".into(),
            }),
            items: vec![
                VendorListingItem {
                    quantity: -1,
                    object_id: 0x80000031,
                    game: game(u32::MAX, 15),
                },
                VendorListingItem {
                    quantity: 5,
                    object_id: 0x80000032,
                    game: game(u32::MAX, 15),
                },
            ],
        };
        assert_eq!(
            listing.encode(0x50000001, 42, 2, limits()).unwrap(),
            bytes(&vector["message"])
        );
        assert_eq!(vector["message"]["group"], 9);
    }
}
