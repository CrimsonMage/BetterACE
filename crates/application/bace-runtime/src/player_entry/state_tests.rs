//! Compiled-original ACE physics flag vectors, including actual non-property getters.
use super::*;
use bace_content::{Property, SparseProperties};
#[test]
fn original_initial_physics_flags_and_player_bubble_overrides() {
    // Entry property names resolve through the accepted pinned enum catalog.
    // A standalone lib test has no cold region startup to install that owner.
    if bace_loot::ace_tables::active_id().is_none() {
        let source = include_str!("../../../../gameplay/bace-loot/data/ace-treasure-tables.toml");
        let tables = bace_content_tools::parse_treasure_table_set(source).unwrap();
        if let Err(error) = bace_loot::ace_tables::install(tables) {
            assert_eq!(bace_loot::ace_tables::active_id(), Some(1), "{error}");
        }
    }
    assert_eq!(bace_loot::ace_tables::active_id(), Some(1));
    let mut count = 0;
    for line in include_str!("../../tests/fixtures/entry_state.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split(',').collect();
        let mut source = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "physics_oracle".into(),
            weenie_type: 10,
            last_modified: None,
            properties: SparseProperties::default(),
        };
        if fields[1] != "none" {
            source.properties.ints.push(Property {
                id: 93,
                value: fields[1].parse().unwrap(),
            });
        }
        let value: i32 = fields[4].parse().unwrap();
        if value >= 0 {
            source.properties.bools.push(Property {
                id: fields[3].parse().unwrap(),
                value: value != 0,
            });
        }
        assert_eq!(
            initial_physics_state(&source, fields[2].parse().unwrap(), fields[0] == "1"),
            fields[5].parse::<u32>().unwrap(),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 528);
}
#[test]
fn physics_counter_projection_reads_one_owner_without_advancing_it() {
    let mut sequences = Sequences::with_instance(16, 65535).unwrap();
    for (kind, count) in [
        (K::ObjectPosition, 1),
        (K::ObjectMovement, 2),
        (K::ObjectState, 3),
        (K::ObjectVector, 4),
        (K::ObjectTeleport, 5),
        (K::ObjectServerControl, 6),
        (K::ObjectForcePosition, 7),
        (K::ObjectVisualDesc, 8),
    ] {
        for _ in 0..count {
            sequences.advance(kind, 0).unwrap();
        }
    }
    let expected = PhysicsSequences {
        position: 1,
        movement: 2,
        state: 3,
        vector: 4,
        teleport: 5,
        server_control: 6,
        force_position: 7,
        visual_description: 8,
        instance: 65535,
    };
    assert_eq!(physics_sequences(&sequences), expected);
    assert_eq!(physics_sequences(&sequences), expected);
    assert_eq!(sequences.advance(K::ObjectInstance, 0).unwrap(), 0);
    assert_eq!(physics_sequences(&sequences).instance, 0);
}
