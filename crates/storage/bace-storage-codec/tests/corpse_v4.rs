use bace_storage_codec::*;
fn legacy() -> CorpseSaveV3 {
    CorpseSaveV3::migrate_v2(CorpseSaveV2 {
        corpse: CorpseSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x80000001,
                template_revision: 1,
                mutation_revision: 1,
                state: bace_content::WeenieV1 {
                    schema_version: 1,
                    weenie_id: 100,
                    class_name: "corpse_fixture".into(),
                    weenie_type: 11,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            owner: Some(0x50000001),
            death_operation: "native-death-opaque-event-id".into(),
            expires_at: 123456,
        },
        placement: ItemPlacementV2::Removed,
    })
    .unwrap()
}
#[test]
fn migration_preserves_prior_bytes_without_fabricating_identity() {
    let old = legacy();
    let v3 = old.encode().unwrap();
    let migrated = CorpseSaveV4::decode_or_migrate(&v3, None).unwrap();
    assert_eq!(migrated.previous.encode().unwrap(), v3);
    assert_eq!((migrated.source, migrated.operation), (None, None));
    let v2 = old.previous.encode().unwrap();
    assert_eq!(
        CorpseSaveV4::decode_or_migrate(&v2, None).unwrap(),
        migrated
    );
    let v1 = old.corpse.encode().unwrap();
    assert!(CorpseSaveV4::decode_or_migrate(&v1, None).is_err());
    assert_eq!(
        CorpseSaveV4::decode_or_migrate(&v1, Some(ItemPlacementV2::Removed)).unwrap(),
        migrated
    );
}
#[test]
fn identity_pair_is_atomic_bounded_and_envelope_checked() {
    let good = CorpseSaveV4 {
        previous: legacy(),
        source: Some(0x50000001),
        operation: Some(u64::MAX),
    };
    let bytes = good.encode().unwrap();
    assert_eq!(CorpseSaveV4::decode(&bytes).unwrap(), good);
    for (source, operation) in [
        (None, Some(1)),
        (Some(1), None),
        (Some(0), Some(1)),
        (Some(1), Some(0)),
    ] {
        let mut invalid = good.clone();
        invalid.source = source;
        invalid.operation = operation;
        assert!(invalid.encode().is_err());
        let unchecked = encode(102, 4, &invalid, CodecLimits::default()).unwrap();
        assert!(CorpseSaveV4::decode(&unchecked).is_err());
    }
    for length in 0..bytes.len() {
        assert!(CorpseSaveV4::decode(&bytes[..length]).is_err());
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(CorpseSaveV4::decode(&corrupt).is_err());
    for (kind, schema) in [(101, 4), (102, 5)] {
        let future = encode(kind, schema, &good, CodecLimits::default()).unwrap();
        assert!(CorpseSaveV4::decode_or_migrate(&future, None).is_err());
    }
}
#[test]
fn durable_identity_and_deadline_cannot_be_erased_or_changed() {
    let legacy = CorpseSaveV4::migrate_v3(legacy()).unwrap();
    let mut before = legacy.clone();
    before.source = Some(7);
    before.operation = Some(9);
    validate_corpse_transition(&legacy, &before).unwrap();
    assert!(validate_corpse_transition(&before, &legacy).is_err());
    for index in 0..5 {
        let mut after = before.clone();
        match index {
            0 => after.corpse.expires_at += 1,
            1 => after.source = Some(8),
            2 => after.operation = Some(10),
            3 => after.corpse.owner = None,
            _ => after.corpse.death_operation = "other".into(),
        }
        assert!(validate_corpse_transition(&before, &after).is_err());
    }
    let mut after = before.clone();
    after.corpse.entity.mutation_revision += 1;
    validate_corpse_transition(&before, &after).unwrap();
}

#[test]
fn final_item_pickup_can_only_shorten_expiry_at_a_newer_revision() {
    let before = CorpseSaveV4 {
        previous: legacy(),
        source: Some(7),
        operation: Some(9),
    };
    let mut after = before.clone();
    after.corpse.expires_at -= 15;
    assert!(validate_corpse_transition(&before, &after).is_err());
    after.corpse.entity.mutation_revision += 1;
    validate_corpse_transition(&before, &after).unwrap();
    let mut changed = after.clone();
    changed.corpse.expires_at = before.corpse.expires_at + 1;
    assert!(validate_corpse_transition(&before, &changed).is_err());
    let mut changed = after.clone();
    changed.operation = Some(10);
    assert!(validate_corpse_transition(&before, &changed).is_err());
    assert_eq!(after.corpse.owner, before.corpse.owner);
    assert_eq!(after.corpse.death_operation, before.corpse.death_operation);
}
