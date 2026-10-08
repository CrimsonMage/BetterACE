use bace_content::{Property, TreasureWieldedRowV1, WeenieV1};
use bace_loot::{TreasureError, TreasureRandom, WieldedTreasure};
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq)]
struct Draws {
    floats: Vec<f64>,
    ints: Vec<i32>,
    f: usize,
    i: usize,
}
impl TreasureRandom for Draws {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        let n = *self.floats.get(self.f).ok_or(TreasureError::Bounds)?;
        self.f += 1;
        Ok(n)
    }
    fn inclusive(&mut self, lo: i32, hi: i32) -> Result<i32, TreasureError> {
        let n = *self.ints.get(self.i).ok_or(TreasureError::Bounds)?;
        self.i += 1;
        if !(lo..=hi).contains(&n) {
            return Err(TreasureError::Bounds);
        }
        Ok(n)
    }
}
fn template(id: u32) -> Option<Arc<WeenieV1>> {
    let mut value = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("template{id}"),
        weenie_type: if id == 100 { 51 } else { 1 },
        last_modified: None,
        properties: Default::default(),
    };
    value.properties.ints = vec![(5, 6), (12, 3), (13, 2), (15, 7), (19, 21)]
        .into_iter()
        .map(|(id, value)| Property { id, value })
        .collect();
    Some(Arc::new(value))
}
fn row(text: &str, index: u32) -> TreasureWieldedRowV1 {
    let f: Vec<_> = text.split(':').collect();
    TreasureWieldedRowV1 {
        id: index,
        treasure_type: 1,
        weenie_class_id: f[0].parse().unwrap(),
        palette_id: f[7].parse().unwrap(),
        shade: f[8].parse().unwrap(),
        stack_size: f[5].parse().unwrap(),
        stack_size_variance: f[6].parse().unwrap(),
        probability: f[1].parse().unwrap(),
        set_start: f[2] == "1",
        has_sub_set: f[3] == "1",
        continues_previous_set: f[4] == "1",
        unknown_1: 0,
        unknown_3: 0,
        unknown_4: 0,
        unknown_5: 0,
        unknown_9: 0,
        unknown_10: 0,
        unknown_11: 0,
        unknown_12: 0,
        last_modified: String::new(),
    }
}
#[test]
fn original_ace_wielded_selection_mutations_and_draw_order() {
    for line in include_str!("fixtures/wielded.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let f: Vec<_> = line.split('|').collect();
        let rows = f[1]
            .split(';')
            .filter(|s| !s.is_empty())
            .enumerate()
            .map(|(i, s)| row(s, i as u32))
            .collect();
        let table = WieldedTreasure::prepare(rows, template).unwrap();
        let mut draws = Draws {
            floats: f[2]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect(),
            ints: f[3]
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect(),
            f: 0,
            i: 0,
        };
        let items = table.generate(&mut draws).unwrap();
        assert_eq!(draws.f, f[4].parse::<usize>().unwrap(), "{}", f[0]);
        assert_eq!(draws.i, f[5].parse::<usize>().unwrap(), "{}", f[0]);
        let expected: Vec<_> = f[6].split(';').filter(|s| !s.is_empty()).collect();
        assert_eq!(items.len(), expected.len(), "{}", f[0]);
        for (item, expected) in items.iter().zip(expected) {
            let e: Vec<_> = expected.split(':').collect();
            let int = |id| {
                item.properties
                    .ints
                    .iter()
                    .find(|p| p.id == id)
                    .map_or(0, |p| p.value)
            };
            assert_eq!(item.weenie_id, e[0].parse::<u32>().unwrap());
            for (id, value) in [(12, e[1]), (3, e[2]), (5, e[4]), (19, e[5])] {
                assert_eq!(int(id), value.parse::<i32>().unwrap());
            }
            assert_eq!(
                item.properties
                    .floats
                    .iter()
                    .find(|p| p.id == 12)
                    .map_or(0.0, |p| p.value),
                e[3].parse::<f64>().unwrap()
            );
        }
    }
}
#[test]
fn failed_materialization_preserves_random_cursor_and_missing_assets_fail_prepare() {
    let source = row("100:1:1:0:0:5:0.5:0:0", 1);
    assert!(matches!(
        WieldedTreasure::prepare(vec![source.clone()], |_| None),
        Err(TreasureError::MissingTemplate(100))
    ));
    let table = WieldedTreasure::prepare(vec![source], template).unwrap();
    let mut draws = Draws {
        floats: vec![0.0, 0.0],
        ints: vec![2],
        f: 0,
        i: 0,
    };
    let prior = draws.clone();
    assert_eq!(table.generate(&mut draws), Err(TreasureError::Bounds));
    assert_eq!(draws, prior);
}
