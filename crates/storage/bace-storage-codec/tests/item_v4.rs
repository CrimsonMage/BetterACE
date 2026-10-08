use bace_storage_codec::*;
fn legacy() -> ItemSaveV3 {
    ItemSaveV3::migrate_v2(ItemSaveV2 {
        entity: EntitySaveV1 {
            object_id: 500,
            template_revision: 2,
            mutation_revision: 3,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 100,
                class_name: "constructed_fixture".into(),
                weenie_type: 10,
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
fn complete() -> ItemSaveV4 {
    ItemSaveV4 {
        previous: legacy(),
        construction: Some(FrozenCreatureConstructionV1 {
            weenie_type: 10,
            origin: FrozenGeneratorConstructionOriginV1 {
                generator: 9,
                incarnation: 2,
                content_revision: 3,
                profile: 0,
                occurrence: 0,
                random_identity: [5; 16],
                random_key_version: 1,
            },
            equipment_order: vec![501],
            death_roster: vec![
                FrozenConstructedChildV1 {
                    entity: 502,
                    parent: None,
                },
                FrozenConstructedChildV1 {
                    entity: 503,
                    parent: Some(502),
                },
            ],
        }),
    }
}
#[test]
fn legacy_migration_preserves_bytes_and_unknown_construction() {
    let old = legacy();
    let v3 = old.encode().unwrap();
    let migrated = ItemSaveV4::decode_or_migrate(&v3, None).unwrap();
    assert_eq!(migrated.previous.encode().unwrap(), v3);
    assert!(migrated.construction.is_none());
    assert_eq!(
        ItemSaveV4::decode_or_migrate(&old.previous.encode().unwrap(), None).unwrap(),
        migrated
    );
    let v1 = old.entity.encode_item().unwrap();
    assert!(ItemSaveV4::decode_or_migrate(&v1, None).is_err());
    assert_eq!(
        ItemSaveV4::decode_or_migrate(&v1, Some(old.placement.clone())).unwrap(),
        migrated
    );
}
#[test]
fn subtype_origin_and_topological_roster_are_checked_before_admission() {
    let good = complete();
    let bytes = good.encode().unwrap();
    assert_eq!(ItemSaveV4::decode(&bytes).unwrap(), good);
    let mut cases = Vec::new();
    for weenie_type in [0, 12, 61, 69, 71] {
        let mut bad = good.clone();
        bad.construction.as_mut().unwrap().weenie_type = weenie_type;
        cases.push(bad);
    }
    for (entity, parent) in [(500, None), (0, None), (502, Some(503)), (502, Some(502))] {
        let mut bad = good.clone();
        bad.construction.as_mut().unwrap().death_roster[0] =
            FrozenConstructedChildV1 { entity, parent };
        cases.push(bad);
    }
    let mut bad = good.clone();
    bad.construction.as_mut().unwrap().equipment_order = vec![501, 501];
    cases.push(bad);
    let mut bad = good.clone();
    bad.construction.as_mut().unwrap().death_roster[1].entity = 502;
    cases.push(bad);
    let mut bad = good.clone();
    bad.construction.as_mut().unwrap().origin.random_key_version = 0;
    cases.push(bad);
    let mut bad = good.clone();
    bad.construction.as_mut().unwrap().equipment_order = (1..=1025).collect();
    cases.push(bad);
    for bad in cases {
        assert!(bad.encode().is_err());
        let unchecked = encode(101, 4, &bad, CodecLimits::default()).unwrap();
        assert!(ItemSaveV4::decode(&unchecked).is_err());
    }
    for end in 0..bytes.len() {
        assert!(ItemSaveV4::decode(&bytes[..end]).is_err());
    }
    let mut corrupt = bytes;
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(ItemSaveV4::decode(&corrupt).is_err());
}
#[test]
fn ordinary_saves_cannot_erase_or_reassign_construction_origin() {
    let before = complete();
    let legacy = ItemSaveV4::migrate_v3(before.previous.clone()).unwrap();
    validate_item_construction_transition(&legacy, &before).unwrap();
    assert!(validate_item_construction_transition(&before, &legacy).is_err());
    let mut changed = before.clone();
    changed.construction.as_mut().unwrap().origin.occurrence += 1;
    assert!(validate_item_construction_transition(&before, &changed).is_err());
    let mut changed = before.clone();
    changed.construction.as_mut().unwrap().death_roster.pop();
    validate_item_construction_transition(&before, &changed).unwrap(); // graph delta checked by placement adapter
}
