use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5,
    validate_item_source_destination_transition,
};

fn legacy() -> ItemSaveV4 {
    ItemSaveV4::migrate_v2(ItemSaveV2 {
        entity: EntitySaveV1 {
            object_id: 500,
            template_revision: 2,
            mutation_revision: 3,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 100,
                class_name: "source_origin_fixture".into(),
                weenie_type: 2,
                last_modified: None,
                properties: Default::default(),
            },
        },
        placement: ItemPlacementV2::Contained {
            container: 700,
            slot: 0,
            pack_slot: false,
            equipped: 0,
        },
    })
    .unwrap()
}

#[test]
fn legacy_origin_is_unknown_and_authorized_flags_round_trip() {
    let old = legacy();
    let migrated = ItemSaveV5::decode_or_migrate(&old.encode().unwrap(), None).unwrap();
    assert_eq!(migrated.previous, old);
    assert_eq!(migrated.source_destination, None);
    for flags in [0, 1, 2, 4, 8, 16, 32, 7, 9, 10, 12, 63] {
        let mut current = migrated.clone();
        current.source_destination = Some(flags);
        assert_eq!(
            ItemSaveV5::decode(&current.encode().unwrap()).unwrap(),
            current
        );
    }
    let mut invalid = migrated;
    invalid.source_destination = Some(64);
    assert!(invalid.encode().is_err());
}

#[test]
fn known_origin_cannot_be_rewritten_or_erased() {
    let before = ItemSaveV5 {
        previous: legacy(),
        source_destination: Some(9),
    };
    let mut same = before.clone();
    same.entity.mutation_revision += 1;
    assert!(validate_item_source_destination_transition(&before, &same).is_ok());
    same.source_destination = None;
    assert!(validate_item_source_destination_transition(&before, &same).is_err());
    same.source_destination = Some(10);
    assert!(validate_item_source_destination_transition(&before, &same).is_err());
}
