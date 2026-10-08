use bace_content::{Property, SecondaryAttribute, WeenieV1};
use bace_entity::VitalPool;
use bace_geometry::Vec3;
use bace_runtime::world_saves::freeze_world;
use bace_simulation::PlayerWorldSnapshot;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::CellId;

fn saved() -> PlayerSaveV6 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "human".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.secondary_attributes = [1, 3, 5]
        .map(|id| Property {
            id,
            value: SecondaryAttribute {
                init_level: 10,
                level_from_cp: 4,
                cp_spent: 32,
                current_level: 100,
            },
        })
        .to_vec();
    state.properties.ints.push(Property {
        id: 9999,
        value: 73,
    });
    PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 1,
            mutation_revision: 4,
            state,
        },
        account_id: 1,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap()
}
fn world() -> PlayerWorldSnapshot {
    PlayerWorldSnapshot {
        cell: CellId(0xa260000a),
        position: Vec3::new(32.0, 47.0, 19.0),
        heading: std::f32::consts::FRAC_PI_2,
        vitals: [70, 80, 90].map(|current| {
            Some(VitalPool {
                current,
                maximum: 100,
            })
        }),
    }
}
#[test]
fn accepted_pose_and_current_vitals_survive_binary_reload_without_losing_definitions() {
    let mut saved = saved();
    let before = saved.clone();
    freeze_world(&mut saved, world()).unwrap();
    let reloaded = PlayerSaveV6::decode_or_migrate(&saved.encode().unwrap()).unwrap();
    assert_eq!(reloaded, saved);
    let props = &reloaded.player.entity.state.properties;
    let location = &props.positions.iter().find(|p| p.id == 1).unwrap().value;
    assert_eq!(location.obj_cell_id, 0xa260000a);
    assert_eq!(location.position_y, 47.0);
    assert!((location.rotation_z - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    for (p, current) in props.secondary_attributes.iter().zip([70, 80, 90]) {
        assert_eq!(p.value.current_level, current);
        assert_eq!(p.value.init_level, 10);
        assert_eq!(p.value.level_from_cp, 4);
        assert_eq!(p.value.cp_spent, 32);
    }
    assert_eq!(props.ints, before.player.entity.state.properties.ints);
    assert_eq!(reloaded.player.metadata, before.player.metadata);
    assert_eq!(reloaded.player.entity.mutation_revision, 4);
}
#[test]
fn invalid_or_missing_vital_definitions_leave_frozen_player_unchanged() {
    let mut saved = saved();
    saved
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .retain(|p| p.id != 5);
    let original = saved.clone();
    assert!(freeze_world(&mut saved, world()).is_err());
    assert_eq!(saved, original);
    let mut bad = world();
    bad.position.x = f32::NAN;
    assert!(freeze_world(&mut saved, bad).is_err());
    assert_eq!(saved, original);
    let mut bad = world();
    bad.vitals[0] = Some(VitalPool {
        current: 101,
        maximum: 100,
    });
    assert!(freeze_world(&mut saved, bad).is_err());
    assert_eq!(saved, original);
}
