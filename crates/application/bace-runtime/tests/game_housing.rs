use bace_persistence::{CharacterLease, OwnershipState};
use bace_runtime::game_housing::*;
use bace_storage_codec::*;
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "housing_fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
#[test]
fn offline_eviction_proposal_preserves_player_rare_ui_and_house_contents_without_physics() {
    let id = 0x50000001;
    let mut player = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: entity(id),
            account_id: 1,
            name: "Housing Owner".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    player.rares = Some(RareStateV1 {
        character: id,
        random_identity: [7; 16],
        key_version: 1,
        attempt_ordinal: 5,
        timer_ordinal: 2,
        next_realtime_at: Some(1000),
        last_effective_time: 10,
    });
    player.ui.gameplay_options = vec![1, 2, 3];
    let original = player.clone();
    let mut house = HouseSaveV3::migrate_v2(
        HouseSaveV2::migrate_v1(HouseSaveV1 {
            entity: entity(0x70000001),
            house_id: 99,
            owner_id: id,
            purchased_at: 1,
            rent_period_start: 1,
            rent_due_at: 2,
            access: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        prepare_housing_state(&house, 30 * 86400, None),
        Err(HousingFreezeError::MissingRent)
    ));
    house.rent_complete = true;
    house.rent = vec![HousePaymentV2 {
        template: 273,
        required: 10,
        paid: 0,
    }];
    let operation = prepare_offline_rent(OfflineRentInput {
        house: house.clone(),
        house_version: 4,
        player,
        player_version: 8,
        lease: CharacterLease {
            character_id: id,
            epoch: 3,
            state: OwnershipState::Offline,
        },
        interval_seconds: 30 * 86400,
        now: 3,
        rent_enabled: true,
        requirements_met: true,
        apartment: false,
        rent_supplement: None,
    })
    .unwrap()
    .unwrap();
    assert_eq!(operation.ownership.owner, None);
    assert_eq!(operation.ownership.generation, 2);
    assert!(
        operation.inventory.changes.is_empty(),
        "eviction retains items"
    );
    let saved = PlayerSaveV6::decode(
        &operation
            .inventory
            .snapshots
            .iter()
            .find(|s| s.object_id == id)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(saved.rares, original.rares);
    assert_eq!(saved.ui, original.ui);
    assert!(
        saved
            .player
            .entity
            .state
            .properties
            .bools
            .iter()
            .any(|p| p.id == 9003 && p.value)
    );
    let saved_house = HouseSaveV3::decode(
        &operation
            .inventory
            .snapshots
            .iter()
            .find(|s| s.object_id == house.entity.object_id)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert!(saved_house.owner_id.is_none());
    assert_eq!(saved_house.access_generation, 2);
}
