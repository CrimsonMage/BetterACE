use super::*;
use bace_content::{Attribute, Property, SecondaryAttribute};
use bace_storage_codec::*;
fn player() -> PlayerSaveV6 {
    let mut state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "entry_fixture".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.attributes = [1, 2, 4, 3, 5, 6]
        .into_iter()
        .enumerate()
        .map(|(slot, id)| Property {
            id,
            value: Attribute {
                level_from_cp: 3,
                init_level: (slot as u32 + 1) * 10,
                cp_spent: 5,
            },
        })
        .collect();
    state.properties.secondary_attributes = [1, 3, 5]
        .into_iter()
        .enumerate()
        .map(|(slot, id)| Property {
            id,
            value: SecondaryAttribute {
                level_from_cp: 3,
                init_level: 4,
                cp_spent: 5,
                current_level: (slot as u32 + 7) * 10,
            },
        })
        .collect();
    let mut saved = PlayerSaveV6::decode_or_migrate(
        &PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x50000001,
                template_revision: 1,
                mutation_revision: 1,
                state,
            },
            account_id: 1,
            name: "Entry Fixture".into(),
            metadata: Default::default(),
            quests: vec![],
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    saved.ui.spellbook_filters = 0;
    saved
}
#[test]
fn source_visibility_lists_are_exact_and_reject_server_only_or_unknown_properties() {
    let mut saved = player();
    let p = &mut saved.player.entity.state.properties;
    for id in 0..=1000 {
        p.ints.push(Property { id, value: 0 });
        p.int64s.push(Property { id, value: 0 });
        p.bools.push(Property { id, value: false });
        p.floats.push(Property { id, value: 0. });
        p.strings.push(Property {
            id,
            value: "Private".into(),
        });
        p.data_ids.push(Property { id, value: 0 });
        p.instance_ids.push(Property { id, value: 0 });
    }
    let result = prepare_player_description(&saved, &[], &[], false).unwrap();
    let lists = [
        (
            "PropertyInt",
            result.integers.iter().map(|v| v.0).collect::<Vec<_>>(),
        ),
        (
            "PropertyInt64",
            result.integers64.iter().map(|v| v.0).collect(),
        ),
        (
            "PropertyBool",
            result.booleans.iter().map(|v| v.0).collect(),
        ),
        (
            "PropertyFloat",
            result.doubles.iter().map(|v| v.0).collect(),
        ),
        (
            "PropertyString",
            result.strings.iter().map(|v| v.0).collect(),
        ),
        (
            "PropertyDataId",
            result.data_ids.iter().map(|v| v.0).collect(),
        ),
        (
            "PropertyInstanceId",
            result.instance_ids.iter().map(|v| v.0).collect(),
        ),
    ];
    let fixture = include_str!("../../tests/fixtures/player_login_properties.csv");
    for (name, actual) in lists {
        let expected = fixture
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}|")))
            .unwrap()
            .split(',')
            .map(|v| v.parse::<u16>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name}");
    }
    let plus = prepare_player_description(&saved, &[], &[], true).unwrap();
    assert_eq!(
        plus.strings.iter().find(|p| p.0 == 1).unwrap().1,
        "+Private"
    );
    assert_eq!(
        result.strings.iter().find(|p| p.0 == 1).unwrap().1,
        "Private"
    );
}
#[test]
fn prepared_traits_and_empty_options_match_original_ace_packet() {
    let projection = prepare_player_description(&player(), &[], &[], false).unwrap();
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tools/bace-compat/fixtures/messages.json"
    ))
    .unwrap();
    let hex = vectors["vectors"]["player_description"]["fresh"]["bytes"]
        .as_str()
        .unwrap();
    let expected = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let bytes = projection
        .encode(
            0x50000001,
            42,
            bace_wire::PlayerDescriptionLimits {
                max_table_entries: 256,
                max_string_bytes: 256,
                max_gameplay_options_bytes: 1024,
                max_message_bytes: 65536,
            },
        )
        .unwrap();
    assert_eq!(bytes, expected);
    let mut missing = player();
    missing.player.entity.state.properties.attributes.remove(0);
    assert!(prepare_player_description(&missing, &[], &[], false).is_err());
}

#[test]
fn backpacks_foci_and_ordinary_items_use_source_container_discriminants() {
    // ACE GameEventPlayerDescription.cs: WeenieType.Container (21) takes
    // precedence within side slots; non-pack entries are NonContainer.
    let saved = player();
    let mut entities = Vec::new();
    let mut places = Vec::new();
    for (index, (weenie_type, pack_slot)) in [(21, true), (1, true), (1, false), (21, false)]
        .into_iter()
        .enumerate()
    {
        let mut entity = saved.player.entity.clone();
        entity.object_id = 0x80000001 + index as u32;
        entity.state.weenie_type = weenie_type;
        entities.push(entity);
        places.push(ItemPlacementV2::Contained {
            container: saved.player.entity.object_id,
            slot: index as u32,
            pack_slot,
            equipped: 0,
        });
    }
    let items: Vec<_> = entities
        .iter()
        .zip(&places)
        .map(|(entity, placement)| EntryInventoryItem { entity, placement })
        .collect();
    let result = prepare_player_description(&saved, &items, &[], false).unwrap();
    let hex = include_str!("../../tests/fixtures/entry_inventory.csv")
        .lines()
        .find(|line| !line.starts_with('#'))
        .unwrap();
    let expected: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let mut bytes = (result.inventory.len() as u32).to_le_bytes().to_vec();
    for item in result.inventory {
        bytes.extend(item.object_id.to_le_bytes());
        bytes.extend(item.container_type.to_le_bytes());
    }
    assert_eq!(bytes, expected);
}
