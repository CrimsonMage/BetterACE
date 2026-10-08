use bace_storage_codec::*;
fn old() -> PlayerSaveV3 {
    PlayerSaveV3::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "schema_fixture".into(),
                weenie_type: 24,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: 1,
        name: "Migration Fixture".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
fn recovery() -> CombatRecoverySaveV1 {
    CombatRecoverySaveV1 {
        captured_unix_millis: 10_000,
        state: FrozenCombatRecoveryV1 {
            schema_version: 1,
            revision: 7,
            minimum_remaining: 1.5,
            streak_remaining: 2.0,
            last_success_school: 1,
            last_success_age: 0.0,
        },
    }
}
#[test]
fn old_player_bytes_are_preserved_and_migration_invents_no_lock_or_contract() {
    let old = old();
    let bytes = old.encode().unwrap();
    let new = PlayerSaveV4::decode_or_migrate(&bytes).unwrap();
    assert_eq!(new.previous.encode().unwrap(), bytes);
    assert!(new.combat_recovery.is_none());
    assert!(new.contracts.is_empty());
    let future = encode(100, 5, &new, CodecLimits::default()).unwrap();
    assert!(PlayerSaveV4::decode_or_migrate(&future).is_err());
}
#[test]
fn cooldown_history_cannot_be_erased_rewound_or_cleared_without_new_revision() {
    let before = recovery();
    assert!(validate_recovery_transition(Some(before), None).is_err());
    let mut after = before;
    after.state.revision -= 1;
    assert!(validate_recovery_transition(Some(before), Some(after)).is_err());
    after = before;
    after.state.minimum_remaining = 0.0;
    after.state.streak_remaining = 0.0;
    assert!(validate_recovery_transition(Some(before), Some(after)).is_err());
    after = before;
    after.captured_unix_millis += 1000;
    after.state.minimum_remaining -= 1.0;
    after.state.streak_remaining -= 1.0;
    after.state.last_success_age = 1.0;
    validate_recovery_transition(Some(before), Some(after)).unwrap();
    after.state.last_success_school = 5;
    assert!(validate_recovery_transition(Some(before), Some(after)).is_err());
    after.state.revision += 1;
    validate_recovery_transition(Some(before), Some(after)).unwrap();
    assert_eq!(before.elapsed_seconds(9_000).unwrap(), 0.0);
}
#[test]
fn frozen_recovery_and_contract_boundaries_reject_invalid_payloads() {
    let mut value = PlayerSaveV4::migrate_v3(old()).unwrap();
    value.combat_recovery = Some(recovery());
    value.contracts = vec![ContractSaveV1 {
        id: 1,
        display: true,
    }];
    let encoded = value.encode().unwrap();
    assert_eq!(PlayerSaveV4::decode(&encoded).unwrap(), value);
    value.contracts.push(ContractSaveV1 {
        id: 1,
        display: false,
    });
    assert!(value.encode().is_err());
    value.contracts.clear();
    value
        .combat_recovery
        .as_mut()
        .unwrap()
        .state
        .streak_remaining = f64::NAN;
    let invalid = encode(100, 4, &value, CodecLimits::default()).unwrap();
    assert!(PlayerSaveV4::decode(&invalid).is_err());
}

#[test]
fn stalled_simulation_can_save_conservatively_aged_recovery_without_resetting_it() {
    let before = recovery();
    let mut after = before;
    after.captured_unix_millis += 1000;
    after.state.minimum_remaining -= 0.75;
    after.state.streak_remaining -= 0.75;
    after.state.last_success_age = 0.75;
    validate_recovery_transition(Some(before), Some(after)).unwrap();
    let mut reset = after;
    reset.state.minimum_remaining = before.state.minimum_remaining + 0.01;
    assert!(validate_recovery_transition(Some(before), Some(reset)).is_err());
    let mut early = after;
    early.state.streak_remaining = 0.0;
    assert!(validate_recovery_transition(Some(before), Some(early)).is_err());
    let mut early = after;
    early.state.last_success_age = 1.1;
    assert!(validate_recovery_transition(Some(before), Some(early)).is_err());
    let mut rewind = after;
    rewind.captured_unix_millis += 100;
    rewind.state.last_success_age = 0.5;
    assert!(validate_recovery_transition(Some(after), Some(rewind)).is_err());
}
