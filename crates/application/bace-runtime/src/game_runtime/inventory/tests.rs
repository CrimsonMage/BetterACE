use super::*;
use crate::{
    game_inventory::FrozenInventoryItem,
    player_entry::{PreparedEntryAppearanceAssets, prepare_entry_object, prepare_item_model},
};
use bace_content::{Property, WeenieV1};
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4};
fn item(id: u32, container: u32, equipped: u32) -> FrozenInventoryItem {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 500,
        class_name: "split_output_fixture".into(),
        weenie_type: 51,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.strings.push(Property {
        id: 1,
        value: "Fresh template name".into(),
    });
    state.properties.ints.extend([
        Property { id: 1, value: 128 },
        Property { id: 11, value: 100 },
        Property { id: 12, value: 2 },
    ]);
    state.properties.data_ids.push(Property {
        id: 8,
        value: 0x6000001,
    });
    state.properties.instance_ids.push(Property {
        id: 2,
        value: container,
    });
    FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: None,
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 7,
            mutation_revision: 1,
            state,
        },
        placement: Some(ItemPlacementV2::Contained {
            container,
            slot: 2,
            pack_slot: false,
            equipped,
        }),
        persisted_version: 1,
        enchantments: vec![],
    }
}
#[test]
fn adapter_rejects_external_equipped_and_cyclic_ancestry_before_cold_allocation() {
    let actor = EntityId(0x50000001);
    let id = EntityId(0x80000001);
    let bag = 0x80000002;
    let request = InventoryRequest::SplitToContainer {
        item: id,
        container: actor,
        placement: 0,
        amount: 2,
    };
    let mut rows = vec![item(id.0, bag, 0), item(bag, actor.0, 0)];
    assert_eq!(
        cold::owned_source(actor, request, |id| rows
            .iter()
            .find(|r| r.entity.object_id == id)
            .and_then(|r| r.placement.as_ref().map(|p| (&r.entity.state, p))))
        .unwrap()
        .weenie_id,
        500
    );
    rows[1].placement = Some(ItemPlacementV2::Contained {
        container: id.0,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    });
    assert!(
        cold::owned_source(actor, request, |id| rows
            .iter()
            .find(|r| r.entity.object_id == id)
            .and_then(|r| r.placement.as_ref().map(|p| (&r.entity.state, p))))
        .is_err()
    );
    rows[1].placement = Some(ItemPlacementV2::Contained {
        container: actor.0,
        slot: 0,
        pack_slot: false,
        equipped: 1,
    });
    assert!(
        cold::owned_source(actor, request, |id| rows
            .iter()
            .find(|r| r.entity.object_id == id)
            .and_then(|r| r.placement.as_ref().map(|p| (&r.entity.state, p))))
        .is_err()
    );
    rows.pop();
    assert!(
        cold::owned_source(actor, request, |id| rows
            .iter()
            .find(|r| r.entity.object_id == id)
            .and_then(|r| r.placement.as_ref().map(|p| (&r.entity.state, p))))
        .is_err()
    );
}
#[test]
fn fresh_description_uses_full_committed_qualities_and_canonical_object_counters() {
    use bace_replication::{SequenceKind, Sequences};
    if bace_loot::ace_tables::active_id().is_none() {
        let source =
            include_str!("../../../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
        let tables = bace_content_tools::parse_treasure_table_set(source).unwrap();
        if let Err(error) = bace_loot::ace_tables::install(tables) {
            assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
        }
    }
    assert_eq!(bace_loot::ace_tables::active_id(), Some(1));
    let id = 0x80000001;
    let root = 0x50000001;
    let row = item(id, root, 0);
    let saved = ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: row.entity,
        placement: row.placement.unwrap(),
    })
    .unwrap();
    let saved = ItemSaveV4::decode(&saved.encode().unwrap()).unwrap();
    let chargen = bace_dat::CharGen {
        reserved: 0,
        starter_areas: vec![],
        heritage_marker: 0,
        heritage_groups: BTreeMap::new(),
    };
    let appearance = PreparedEntryAppearanceAssets::default();
    let mut sequences = Sequences::with_instance(256, 12).unwrap();
    sequences.advance(SequenceKind::ObjectPosition, 0).unwrap();
    let object = prepare_entry_object(
        id,
        &saved.entity.state,
        prepare_item_model(&saved.entity.state, &appearance.borrowed(&chargen)).unwrap(),
        output::contained_state(&saved.entity.state, &sequences),
    )
    .unwrap();
    assert_eq!(object.object_id, id);
    assert_eq!(object.game.name, "Fresh template name");
    assert_eq!(object.game.class_id, 500);
    assert_eq!(object.game.icon_id, 0x6000001);
    assert_eq!(object.game.options.container, Some(root));
    assert_eq!(object.game.options.stack_size, Some(2));
    assert_eq!(object.game.options.max_stack_size, Some(100));
    assert_eq!(object.physics.sequences.instance, 12);
    assert_eq!(object.physics.sequences.position, 1);
    assert!(object.physics.options.position.is_none());
    assert!(object.physics.options.parent.is_none());
}
