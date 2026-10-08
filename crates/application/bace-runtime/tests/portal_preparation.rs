use bace_content::{Position, Property, WeenieV1};
use bace_runtime::portal_preparation::{freeze_portal_links, prepare_portal_links};
fn position(x: f32) -> Position {
    Position {
        obj_cell_id: 1,
        position_x: x,
        position_y: 0.0,
        position_z: 0.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}
fn state() -> WeenieV1 {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "fixture".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.positions = vec![
        Property {
            id: 16,
            value: position(16.0),
        },
        Property {
            id: 1,
            value: position(1.0),
        },
        Property {
            id: 4,
            value: position(4.0),
        },
        Property {
            id: 15,
            value: position(15.0),
        },
    ];
    state.properties.data_ids = vec![
        Property { id: 48, value: 0 },
        Property { id: 1, value: 123 },
        Property { id: 31, value: 0 },
        Property { id: 47, value: 0 },
    ];
    state.properties.bools = vec![
        Property {
            id: 9002,
            value: false,
        },
        Property { id: 1, value: true },
        Property {
            id: 9001,
            value: true,
        },
    ];
    state
}
#[test]
fn unchanged_portal_owner_preserves_unsorted_sparse_vectors_and_present_zero() {
    let before = state();
    let links = prepare_portal_links(&before, 42).unwrap();
    for slot in [31, 47, 48] {
        assert_eq!(links.template(slot), Some(0));
    }
    assert_eq!(freeze_portal_links(&before, &links).unwrap(), before);
}
#[test]
fn changed_sanctuary_updates_in_place_and_explicit_removal_preserves_other_order() {
    let before = state();
    let mut links = prepare_portal_links(&before, 42).unwrap();
    let mut changed = links.position(4).unwrap();
    changed.origin[0] = 12.0;
    links.synchronize_sanctuary(Some(changed)).unwrap();
    let after = freeze_portal_links(&before, &links).unwrap();
    let mut expected = before.clone();
    expected.properties.positions[2].value.position_x = 12.0;
    assert_eq!(after, expected);
    links.synchronize_sanctuary(None).unwrap();
    expected.properties.positions.remove(2);
    assert_eq!(freeze_portal_links(&after, &links).unwrap(), expected);
    links.synchronize_sanctuary(Some(changed)).unwrap();
    let without_sanctuary = expected.clone();
    expected.properties.positions.push(Property {
        id: 4,
        value: position(12.0),
    });
    assert_eq!(
        freeze_portal_links(&without_sanctuary, &links).unwrap(),
        expected
    );
}

#[test]
fn committed_link_can_replace_present_zero_without_reordering_other_fields() {
    let before = state();
    let destination = prepare_portal_links(&before, 42)
        .unwrap()
        .position(15)
        .unwrap();
    let mutation = bace_interactions::PortalLinkMutation {
        before_revision: 42,
        after_revision: 43,
        position_slot: 8,
        before: None,
        after: destination,
        data_id: Some((31, Some(0), 88)),
        tied_summoned: false,
    };
    let after = bace_runtime::portal_preparation::freeze_portal_link(&before, &mutation).unwrap();
    let mut expected = before.clone();
    expected.properties.positions.push(Property {
        id: 8,
        value: position(15.0),
    });
    expected.properties.data_ids[2].value = 88;
    expected.properties.bools[2].value = false;
    assert_eq!(after, expected);
}
