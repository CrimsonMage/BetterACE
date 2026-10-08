use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_loot::{TreasureError, TreasureRandom, generate_creature_equipment};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone)]
struct Draw {
    roll: f64,
    count: usize,
    floats: usize,
}
impl TreasureRandom for Draw {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.floats += 1;
        Ok(self.roll)
    }
    fn inclusive(&mut self, l: i32, h: i32) -> Result<i32, TreasureError> {
        self.count += 1;
        Ok(l + ((h - l + 1) as f64 * self.roll) as i32)
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
#[test]
fn original_monster_equipment_selection() {
    for line in include_str!("fixtures/equipment.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let columns: Vec<_> = line.split('|').collect();
        let mut source = item(10000, 10);
        source.properties.ints = vec![
            Property { id: 6, value: 1024 },
            Property {
                id: 101,
                value: 256,
            },
        ];
        source.properties.bools = vec![Property {
            id: 19,
            value: columns[2] != "3",
        }];
        let mut templates = BTreeMap::new();
        for row in columns[3].split(';') {
            let p: Vec<i32> = row.split(':').map(|s| s.parse().unwrap()).collect();
            let mut w = item(p[0] as u32, p[1] as u32);
            w.properties.ints = [
                (9, p[2]),
                (4, p[3]),
                (28, p[4]),
                (46, p[5]),
                (50, p[6]),
                (51, p[8]),
                (1, p[9]),
                (48, p[10]),
            ]
            .into_iter()
            .map(|(id, value)| Property { id, value })
            .collect();
            w.properties.bools.push(Property {
                id: 130,
                value: p[7] != 0,
            });
            if p[0] <= columns[0].parse::<i32>().unwrap() {
                source.properties.create_list.push(CreateListEntry {
                    destination_type: 10,
                    weenie_class_id: w.weenie_id,
                    stack_size: 1,
                    palette: 0,
                    shade: 0.0,
                    try_to_bond: false,
                    database_record_id: 0,
                });
            }
            templates.insert(w.weenie_id, Arc::new(w));
        }
        let mut rng = Draw {
            roll: columns[1].parse().unwrap(),
            count: 0,
            floats: 0,
        };
        let first = columns[0].parse::<u32>().unwrap();
        let wielded = if columns[2] == "4" {
            Some(
                bace_loot::WieldedTreasure::prepare(
                    (first + 1..=first + 3)
                        .map(|id| bace_content::TreasureWieldedRowV1 {
                            id,
                            treasure_type: 1,
                            weenie_class_id: id,
                            palette_id: 0,
                            shade: 0.,
                            stack_size: 0,
                            stack_size_variance: 0.,
                            probability: 1.,
                            set_start: false,
                            has_sub_set: false,
                            continues_previous_set: false,
                            unknown_1: 0,
                            unknown_3: 0,
                            unknown_4: 0,
                            unknown_5: 0,
                            unknown_9: 0,
                            unknown_10: 0,
                            unknown_11: 0,
                            unknown_12: 0,
                            last_modified: String::new(),
                        })
                        .collect(),
                    |id| templates.get(&id).cloned(),
                )
                .unwrap(),
            )
        } else {
            None
        };
        let got = generate_creature_equipment(
            &source,
            &templates,
            wielded.as_ref(),
            &[],
            false,
            &mut rng,
        )
        .unwrap();
        let text = got
            .iter()
            .map(|i| format!("{}:{}", i.source.weenie_id, i.wielded_location))
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(
            text, columns[5],
            "count={} roll={} scenario={}",
            columns[0], columns[1], columns[2]
        );
        assert_eq!(rng.count, columns[4].parse::<usize>().unwrap(), "{line}");
    }
}
fn create(id: u32, destination_type: i32) -> CreateListEntry {
    CreateListEntry {
        database_record_id: 0,
        destination_type,
        weenie_class_id: id,
        stack_size: 1,
        palette: 0,
        shade: 0.0,
        try_to_bond: false,
    }
}
#[test]
fn signed_minus_one_capacity_blocks_direct_slots_but_keeps_sidepack_fallback() {
    // The pinned complete Patches row for WCID 30997 (academyresearcher) has
    // ItemCapacity and ContainerCapacity both -1. ACE's signed comparison in
    // Container.TryAddToInventory treats each as having no free direct slot.
    let mut source = item(30997, 12);
    source.properties.ints = vec![Property { id: 6, value: -1 }, Property { id: 7, value: -1 }];
    source.properties.create_list.push(create(118, 2));
    let templates = [(118, Arc::new(item(118, 1)))].into_iter().collect();
    let mut random = Draw {
        roll: 0.5,
        count: 0,
        floats: 0,
    };
    assert!(
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random)
            .unwrap()
            .is_empty()
    );
    assert_eq!(random.floats, 1);

    source.properties.ints[1].value = 1;
    source.properties.create_list.insert(0, create(119, 2));
    let mut bag = item(119, 21);
    bag.properties.ints.push(Property { id: 6, value: 1 });
    bag.properties.bools.push(Property {
        id: 81,
        value: true,
    });
    let templates = [(118, Arc::new(item(118, 1))), (119, Arc::new(bag))]
        .into_iter()
        .collect();
    let output =
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random).unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(output[0].source.weenie_id, 119);
    assert_eq!(output[1].parent_index, Some(0));

    source.properties.ints[0].value = -2;
    assert_eq!(
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random)
            .unwrap_err(),
        TreasureError::Bounds
    );
}
#[test]
fn nested_container_contents_and_capacity_preserve_parent_placement() {
    let mut source = item(1000, 10);
    source.properties.ints = vec![Property { id: 6, value: 0 }, Property { id: 7, value: 1 }];
    source.properties.create_list = vec![create(1, 10), create(3, 10), create(4, 10)];
    let mut bag = item(1, 21);
    bag.properties.ints = vec![Property { id: 6, value: 2 }];
    bag.properties.bools = vec![Property {
        id: 81,
        value: true,
    }];
    bag.properties.create_list.push(create(2, 1));
    let templates = [bag, item(2, 1), item(3, 1), item(4, 1)]
        .into_iter()
        .map(|w| (w.weenie_id, Arc::new(w)))
        .collect();
    let mut random = Draw {
        roll: 0.3,
        count: 0,
        floats: 0,
    };
    let generated =
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random).unwrap();
    assert_eq!(
        generated
            .iter()
            .map(|i| (i.source.weenie_id, i.parent_index, i.inventory_slot))
            .collect::<Vec<_>>(),
        vec![(1, None, 0), (2, Some(0), 1), (3, Some(0), 0)]
    );
    assert!(generated[0].death_drop);
    assert!(!generated[1].death_drop);
    assert_eq!(
        generated
            .iter()
            .map(|item| item.source_destination)
            .collect::<Vec<_>>(),
        vec![Some(10), Some(1), Some(10)]
    );
}
#[test]
fn missing_container_child_follows_factory_skip_but_missing_selected_root_is_atomic() {
    let mut source = item(1000, 10);
    source.properties.ints = vec![Property { id: 6, value: 1 }];
    source.properties.create_list.push(create(1, 10));
    let mut bag = item(1, 21);
    bag.properties.ints = vec![Property { id: 6, value: 1 }];
    bag.properties.create_list.push(create(999, 1));
    let templates = [(1, Arc::new(bag))].into_iter().collect();
    let mut random = Draw {
        roll: 0.3,
        count: 0,
        floats: 0,
    };
    // Original Container.GenerateContainList continues when its factory returns
    // null; the independent container oracle covers this exact branch.
    let generated =
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random).unwrap();
    assert_eq!(generated.len(), 1);
    assert_eq!(generated[0].source.weenie_id, 1);
    source.properties.create_list[0].weenie_class_id = 999;
    random.count = 0;
    random.floats = 0;
    assert_eq!(
        generate_creature_equipment(&source, &templates, None, &[], false, &mut random)
            .unwrap_err(),
        TreasureError::MissingTemplate(999)
    );
    assert_eq!(random.count, 0);
    assert_eq!(random.floats, 0);
}
#[test]
fn original_death_drop_flags_and_destroy_bonded_filter() {
    for line in include_str!("fixtures/equipment_drop.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let parts: Vec<_> = line.split('|').collect();
        let mut source = item(1000, 10);
        source.properties.ints.push(Property { id: 6, value: 1024 });
        let mut templates = BTreeMap::new();
        let mut inventory = Vec::new();
        let mut wielded = Vec::new();
        for row in parts[1].split(';') {
            let p: Vec<i32> = row.split(':').map(|v| v.parse().unwrap()).collect();
            let id = p[0] as u32;
            let mut w = item(id, 1);
            w.properties.ints.push(Property {
                id: 33,
                value: p[2],
            });
            match p[1] {
                2 | 10 => source.properties.create_list.push(create(id, p[1])),
                8 => inventory.push(w.clone()),
                _ => wielded.push(bace_content::TreasureWieldedRowV1 {
                    id,
                    treasure_type: 1,
                    weenie_class_id: id,
                    palette_id: 0,
                    shade: 0.,
                    stack_size: 0,
                    stack_size_variance: 0.,
                    probability: 1.,
                    set_start: false,
                    has_sub_set: false,
                    continues_previous_set: false,
                    unknown_1: 0,
                    unknown_3: 0,
                    unknown_4: 0,
                    unknown_5: 0,
                    unknown_9: 0,
                    unknown_10: 0,
                    unknown_11: 0,
                    unknown_12: 0,
                    last_modified: String::new(),
                }),
            }
            templates.insert(id, Arc::new(w));
        }
        let table =
            bace_loot::WieldedTreasure::prepare(wielded, |id| templates.get(&id).cloned()).unwrap();
        let mut rng = Draw {
            roll: 0.3,
            count: 0,
            floats: 0,
        };
        let got = generate_creature_equipment(
            &source,
            &templates,
            Some(&table),
            &inventory,
            parts[0] == "1",
            &mut rng,
        )
        .unwrap();
        assert_eq!(
            got.iter()
                .filter(|i| i.death_drop)
                .map(|i| i.source.weenie_id.to_string())
                .collect::<Vec<_>>()
                .join(";"),
            parts[2]
        );
    }
}
