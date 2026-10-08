use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_loot::{TreasureError, TreasureRandom, generate_player_no_corpse_create_list};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
struct Draw {
    consumed: usize,
}
impl TreasureRandom for Draw {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.consumed += 1;
        Ok(0.0)
    }
    fn inclusive(&mut self, low: i32, _: i32) -> Result<i32, TreasureError> {
        Ok(low)
    }
}
fn item(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("item{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: Default::default(),
    }
}
fn row(id: u32, destination_type: i32) -> CreateListEntry {
    CreateListEntry {
        database_record_id: id,
        destination_type,
        weenie_class_id: id,
        stack_size: 1,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }
}

#[test]
fn pinned_no_corpse_create_list_filter_keeps_source_order_and_placeholder() {
    // The pinned original-source fixture records ACE's exact GenerateTreasure
    // filter/call order. Its CreateListSelect helper is a documented stub; the
    // selector's independent C# vectors live in create-list.csv.
    let trace = include_str!("../../../application/bace-runtime/tests/fixtures/no_corpse.trace");
    let source = trace
        .lines()
        .find(|line| line.starts_with("1|0|0|1|0|"))
        .unwrap();
    assert!(source.contains("select:30,31,34,35;create:30;create:31;create:34;create:35"));

    let mut player = item(500, 1);
    player.properties.create_list = vec![
        row(30, 1),
        row(31, 8),
        row(32, 2),
        row(33, 10),
        row(34, 9),
        CreateListEntry {
            weenie_class_id: 0,
            ..row(35, 1)
        },
    ];
    let templates: BTreeMap<_, _> = [30, 31, 34]
        .into_iter()
        .map(|id| (id, Arc::new(item(id, 1))))
        .collect();
    let mut random = Draw { consumed: 0 };
    let forest = generate_player_no_corpse_create_list(&player, &templates, &mut random).unwrap();
    assert_eq!(random.consumed, 1);
    assert_eq!(
        forest
            .iter()
            .map(|row| row.source.weenie_id)
            .collect::<Vec<_>>(),
        [30, 31, 34]
    );
    assert_eq!(
        forest
            .iter()
            .map(|row| row.source_destination)
            .collect::<Vec<_>>(),
        [Some(1), Some(8), Some(9)]
    );
    assert!(forest.iter().all(|row| row.parent_index.is_none()));
}

#[test]
fn fresh_nested_contain_child_keeps_parent_index_and_failed_roll_keeps_cursor() {
    let mut player = item(500, 1);
    player.properties.create_list = vec![row(30, 1), row(34, 9)];
    let mut bag = item(34, 21);
    bag.properties.ints = vec![Property { id: 6, value: 2 }];
    bag.properties.create_list = vec![row(36, 1)];
    let templates: BTreeMap<_, _> = [(30, item(30, 1)), (34, bag), (36, item(36, 1))]
        .into_iter()
        .map(|(id, row)| (id, Arc::new(row)))
        .collect();
    let mut random = Draw { consumed: 0 };
    let forest = generate_player_no_corpse_create_list(&player, &templates, &mut random).unwrap();
    assert_eq!(
        forest
            .iter()
            .map(|row| row.source.weenie_id)
            .collect::<Vec<_>>(),
        [30, 34, 36]
    );
    assert_eq!(forest[2].parent_index, Some(1));
    assert_eq!(forest[2].source_destination, Some(1));
    assert_eq!(random.consumed, 1);

    let mut missing = templates;
    missing.remove(&34);
    let before = random.consumed;
    assert!(generate_player_no_corpse_create_list(&player, &missing, &mut random).is_err());
    assert_eq!(random.consumed, before);
}
