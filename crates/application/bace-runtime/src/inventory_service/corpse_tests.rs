use super::*;
use bace_persistence::SaveSnapshot;
use bace_storage_codec::*;
use bace_types::EntityId;
fn fixture() -> (
    PlacementOperation,
    Vec<FrozenInventoryItem>,
    CorpseDecayChange,
) {
    let mut state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 11,
        class_name: "corpse".into(),
        weenie_type: 11,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.bools.push(bace_content::Property {
        id: 123,
        value: true,
    });
    state.properties.strings.push(bace_content::Property {
        id: 1,
        value: "retained corpse".into(),
    });
    let entity = EntitySaveV1 {
        object_id: 500,
        template_revision: 1,
        mutation_revision: 1,
        state,
    };
    let placement = ItemPlacementV2::World(bace_content::Position {
        obj_cell_id: 0x12340001,
        position_x: 1.,
        position_y: 2.,
        position_z: 3.,
        rotation_w: 1.,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.,
    });
    let corpse = CorpseSaveV4 {
        previous: CorpseSaveV3 {
            previous: CorpseSaveV2 {
                corpse: CorpseSaveV1 {
                    entity: entity.clone(),
                    owner: Some(300),
                    death_operation: "immutable-death".into(),
                    expires_at: 1200,
                },
                placement: placement.clone(),
            },
            enchantments: vec![],
        },
        source: Some(300),
        operation: Some(5),
    };
    let sources = vec![FrozenInventoryItem {
        entity: entity.clone(),
        source_destination: None,
        placement: Some(placement.clone()),
        persisted_version: 3,
        enchantments: vec![],
        construction: None,
        corpse: Some(Box::new(CorpseSaveV5::migrate_v4(corpse).unwrap())),
    }];
    let mut after = entity;
    after.mutation_revision = 2;
    let item = ItemSaveV5::migrate_v4(
        ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity: after,
            placement,
        })
        .unwrap(),
    )
    .unwrap();
    let operation = PlacementOperation {
        operation_id: "final-pickup".into(),
        snapshots: vec![SaveSnapshot {
            object_id: 500,
            mutation_revision: 2,
            expected_version: 3,
            bytes: item.encode().unwrap(),
        }],
        participants: vec![500],
        leases: vec![],
        changes: vec![],
        storage_views: vec![],
    };
    let change = CorpseDecayChange {
        corpse: EntityId(500),
        death_operation: 5,
        before_expires_tick: 6100,
        after_expires_tick: 550,
        prepared_tick: 100,
    };
    (operation, sources, change)
}
#[test]
fn exact_pickup_row_keeps_corpse_identity_and_clamps_deadline_once() {
    let (mut operation, sources, change) = fixture();
    freeze(&mut operation, &sources, &[change], 100, 1_000_000).unwrap();
    let after = CorpseSaveV5::decode(&operation.snapshots[0].bytes).unwrap();
    let before = sources[0].corpse.as_ref().unwrap();
    assert_eq!(after.corpse.expires_at, 1015);
    assert_eq!(after.corpse.owner, before.corpse.owner);
    assert_eq!(after.source, before.source);
    assert_eq!(after.operation, before.operation);
    assert_eq!(after.corpse.death_operation, before.corpse.death_operation);
    assert_eq!(
        after.corpse.entity.state.properties.bools,
        before.corpse.entity.state.properties.bools
    );
    assert_eq!(
        after.corpse.entity.state.properties.strings,
        before.corpse.entity.state.properties.strings
    );
    assert_eq!(
        after
            .corpse
            .entity
            .state
            .properties
            .floats
            .iter()
            .find(|p| p.id == 44)
            .unwrap()
            .value,
        15.
    );
    assert_eq!(operation.snapshots[0].expected_version, 3);
    assert_eq!(after.corpse.entity.mutation_revision, 2);
    assert_eq!(after.access, before.access);
    validate_corpse_transition_v5(before, &after).unwrap();
}
#[test]
fn expired_or_already_shorter_deadline_is_never_extended_and_bad_identity_rejects() {
    let (mut operation, mut sources, mut change) = fixture();
    sources[0].corpse.as_mut().unwrap().corpse.expires_at = 1003;
    freeze(&mut operation, &sources, &[change], 100, 1_000_000).unwrap();
    assert_eq!(
        CorpseSaveV5::decode(&operation.snapshots[0].bytes)
            .unwrap()
            .corpse
            .expires_at,
        1003
    );
    let (mut operation, sources, _) = fixture();
    change.death_operation = 6;
    assert!(freeze(&mut operation, &sources, &[change], 100, 1_000_000).is_err());
    let (mut operation, sources, change) = fixture();
    freeze(&mut operation, &sources, &[change], 600, 1_016_000).unwrap();
    assert_eq!(
        CorpseSaveV5::decode(&operation.snapshots[0].bytes)
            .unwrap()
            .corpse
            .expires_at,
        1016
    );
}
