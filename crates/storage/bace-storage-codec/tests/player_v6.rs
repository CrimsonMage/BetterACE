use bace_storage_codec::*;
fn old() -> PlayerSaveV5 {
    PlayerSaveV5::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "recovery_fixture".into(),
                weenie_type: 24,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: 1,
        name: "Recovery Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
#[test]
fn migration_preserves_old_payload_and_does_not_invent_active_attack() {
    let old = old();
    let bytes = old.encode().unwrap();
    let new = PlayerSaveV6::decode_or_migrate(&bytes).unwrap();
    assert_eq!(new.previous.encode().unwrap(), bytes);
    assert!(new.physical_recovery.is_none());
    assert!(PlayerSaveV5::decode_or_migrate(&new.encode().unwrap()).is_err());
    let future = encode(100, 7, &new, CodecLimits::default()).unwrap();
    assert!(PlayerSaveV6::decode_or_migrate(&future).is_err());
}
#[test]
fn trusted_offline_elapsed_ages_lock_but_backwards_clock_does_not() {
    let recovery = PhysicalRecoverySaveV1 {
        captured_unix_millis: 10_000,
        remaining_seconds: 2.0,
    };
    assert_eq!(recovery.remaining_at(9_000).unwrap(), 2.0);
    assert_eq!(recovery.remaining_at(10_500).unwrap(), 1.5);
    assert_eq!(recovery.remaining_at(15_000).unwrap(), 0.0);
    for remaining_seconds in [f64::NAN, f64::INFINITY, -0.1, 180.01] {
        assert!(
            PhysicalRecoverySaveV1 {
                remaining_seconds,
                ..recovery
            }
            .validate()
            .is_err()
        );
    }
    assert!(recovery.remaining_at(u64::MAX).is_err());
}
#[test]
fn recovery_decode_rejects_invalid_payload_after_integrity_verification() {
    let mut save = PlayerSaveV6::migrate_v5(old()).unwrap();
    save.physical_recovery = Some(PhysicalRecoverySaveV1 {
        captured_unix_millis: 100,
        remaining_seconds: -1.0,
    });
    let bytes = encode(100, 6, &save, CodecLimits::default()).unwrap();
    assert!(PlayerSaveV6::decode(&bytes).is_err());
}
