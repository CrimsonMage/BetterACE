use bace_content::TreasureDeathRowV1;
use bace_loot::{TreasureCategory, TreasureError, TreasureRandom, select_treasure};
mod table_support;
#[derive(Clone)]
struct Draw {
    value: f64,
    count: u32,
}
impl TreasureRandom for Draw {
    fn unit(&mut self) -> Result<f64, TreasureError> {
        self.count += 1;
        Ok(self.value)
    }
    fn inclusive(&mut self, min: i32, max: i32) -> Result<i32, TreasureError> {
        self.count += 1;
        Ok(min + (self.value * f64::from(max - min + 1)) as i32)
    }
}
#[test]
fn official_ace_selection_routes_and_draw_counts() {
    table_support::install_tables();
    for line in include_str!("fixtures/selection.csv")
        .lines()
        .filter(|v| !v.starts_with('#'))
    {
        let v: Vec<_> = line.split('|').collect();
        let n = |i: usize| v[i].parse::<i32>().unwrap();
        let p = TreasureDeathRowV1 {
            id: 1,
            treasure_type: 1,
            tier: n(0),
            loot_quality_mod: 0.0,
            unknown_chances: n(1),
            item_chance: 100,
            item_min_amount: 1,
            item_max_amount: 1,
            item_treasure_type_selection_chances: n(3),
            magic_item_chance: 100,
            magic_item_min_amount: 1,
            magic_item_max_amount: 1,
            magic_item_treasure_type_selection_chances: n(3),
            mundane_item_chance: 100,
            mundane_item_min_amount: 1,
            mundane_item_max_amount: 1,
            mundane_item_type_selection_chances: n(3),
            last_modified: String::new(),
        };
        let mut random = Draw {
            value: v[4].parse().unwrap(),
            count: 0,
        };
        let category = match n(2) {
            0 => TreasureCategory::Item,
            1 => TreasureCategory::Magic,
            _ => TreasureCategory::Mundane,
        };
        let result =
            select_treasure(&p, category, &mut random).unwrap_or_else(|e| panic!("{line}: {e:?}"));
        assert_eq!(
            (
                result.item_type,
                result.armor_type,
                result.weapon_type,
                result.wcid,
                random.count
            ),
            (n(5), n(6), n(7), n(8) as u32, n(9) as u32),
            "{line}"
        );
    }
}
