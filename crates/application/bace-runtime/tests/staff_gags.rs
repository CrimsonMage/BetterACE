use bace_content::{Property, WeenieV1};
use bace_runtime::staff_gags::{freeze_offline_gag, overlay_gag, restore_gag};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
fn saved() -> PlayerSaveV6 {
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 3,
            mutation_revision: 7,
            state: WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "gag_freeze".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: 1,
        name: "Gag Freeze".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
#[test]
fn gag_save_changes_only_source_properties_and_revision_then_reconstruction_resets_notice() {
    let mut before = saved();
    before.player.entity.state.properties.ints.push(Property {
        id: 125,
        value: 987,
    });
    before
        .player
        .entity
        .state
        .properties
        .floats
        .push(Property { id: 1, value: 0.25 });
    let after = freeze_offline_gag(&before, true, 1234.).unwrap();
    assert_eq!(after.player.entity.mutation_revision, 8);
    let mut expected = before.clone();
    expected.player.entity.mutation_revision = 8;
    expected
        .player
        .entity
        .state
        .properties
        .bools
        .push(Property {
            id: 111,
            value: true,
        });
    expected.player.entity.state.properties.floats.extend([
        Property {
            id: 112,
            value: 1234.,
        },
        Property {
            id: 161,
            value: 300.,
        },
    ]);
    assert_eq!(after, expected);
    let mut recovered = restore_gag(&after.player.entity.state).unwrap();
    assert!(!recovered.state.noticed);
    recovered.state.noticed = true;
    recovered.state.remaining = 295.;
    let mut source = after.player.entity.state.clone();
    overlay_gag(&mut source, recovered.state, false).unwrap();
    let cold = restore_gag(&source).unwrap();
    assert_eq!(cold.state.remaining, 295.);
    assert!(!cold.state.noticed);
    assert!(cold.pending_interval.is_none());
    let removed = freeze_offline_gag(&after, false, 9999.).unwrap();
    assert!(
        !removed
            .player
            .entity
            .state
            .properties
            .bools
            .iter()
            .any(|p| p.id == 111)
    );
    assert!(
        !removed
            .player
            .entity
            .state
            .properties
            .floats
            .iter()
            .any(|p| [112, 161].contains(&p.id))
    );
}
#[test]
fn invalid_gag_snapshot_inputs_fail_without_changing_the_before_image() {
    let before = saved();
    assert!(freeze_offline_gag(&before, true, f64::NAN).is_err());
    assert_eq!(before.player.entity.mutation_revision, 7);
    let mut maximum = before.clone();
    maximum.player.entity.mutation_revision = u64::MAX;
    assert!(freeze_offline_gag(&maximum, true, 1.).is_err());
}
