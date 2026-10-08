use bace_storage_codec::*;

fn corpse() -> CorpseSaveV5 {
    let entity = EntitySaveV1 {
        object_id: 0x8000_0001,
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
    };
    let old = CorpseSaveV3::migrate_v2(CorpseSaveV2 {
        corpse: CorpseSaveV1 {
            entity,
            owner: Some(0x5000_0001),
            death_operation: "exact-death".into(),
            expires_at: 123456,
        },
        placement: ItemPlacementV2::Removed,
    })
    .unwrap();
    CorpseSaveV5::migrate_v4(CorpseSaveV4 {
        previous: old,
        source: Some(0x5000_0001),
        operation: Some(17),
    })
    .unwrap()
}

#[test]
fn migration_keeps_v4_identity_and_does_not_invent_loot_history() {
    let fresh = corpse();
    let v4 = fresh.previous.encode().unwrap();
    let migrated = CorpseSaveV5::decode_or_migrate(&v4, None).unwrap();
    assert_eq!(migrated.previous.encode().unwrap(), v4);
    assert_eq!(migrated.access.victim, Some(0x5000_0001));
    assert!(!migrated.access.is_monster);
    assert!(!migrated.access.looted);
    assert!(migrated.access.permittees.is_empty());
    assert_eq!(
        CorpseSaveV5::decode(&migrated.encode().unwrap()).unwrap(),
        migrated
    );
}

#[test]
fn legacy_unknown_source_can_gain_exact_identity_once() {
    let original = corpse();
    let mut before = original.clone();
    before.previous.source = None;
    before.previous.operation = None;
    before.access.victim = None;
    before.access.is_monster = false;
    let mut enriched = original;
    enriched.corpse.entity.mutation_revision += 1;
    validate_corpse_transition_v5(&before, &enriched).unwrap();
    assert!(validate_corpse_transition_v5(&enriched, &before).is_err());
    let mut changed = enriched.clone();
    changed.previous.source = Some(0x5000_0002);
    changed.access.victim = changed.previous.source;
    changed.corpse.entity.mutation_revision += 1;
    assert!(validate_corpse_transition_v5(&enriched, &changed).is_err());
}

#[test]
fn durable_access_grows_only_at_newer_revision() {
    let before = corpse();
    let mut after = before.clone();
    after.access.looted = true;
    assert!(validate_corpse_transition_v5(&before, &after).is_err());
    after.corpse.entity.mutation_revision += 1;
    validate_corpse_transition_v5(&before, &after).unwrap();
    let mut replay = after.clone();
    replay.access.permittees.push(0x5000_0002);
    assert!(validate_corpse_transition_v5(&after, &replay).is_err());
    replay.corpse.entity.mutation_revision += 1;
    validate_corpse_transition_v5(&after, &replay).unwrap();
    assert!(validate_corpse_transition_v5(&replay, &after).is_err());
    let mut changed = replay.clone();
    changed.access.killer = Some(0x5000_0003);
    changed.corpse.entity.mutation_revision += 1;
    assert!(validate_corpse_transition_v5(&replay, &changed).is_err());
}

#[test]
fn malformed_permittee_and_victim_payloads_reject() {
    let mut value = corpse();
    value.access.permittees = vec![8, 7];
    assert!(value.encode().is_err());
    let unchecked = encode(102, 5, &value, CodecLimits::default()).unwrap();
    assert!(CorpseSaveV5::decode(&unchecked).is_err());
    value.access.permittees = vec![7, 7];
    assert!(value.encode().is_err());
    value.access.permittees.clear();
    value.access.victim = Some(0x5000_0002);
    assert!(value.encode().is_err());
}
