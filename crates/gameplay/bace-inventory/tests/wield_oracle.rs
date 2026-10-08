use bace_inventory::{WieldCriterion, WieldPolicy, WieldValues, check_wield_requirements};
struct Values;
impl WieldValues for Values {
    fn skill(&self, key: i32) -> Option<(u32, u32, u32)> {
        (key == 1).then_some((50, 100, 2))
    }
    fn attribute(&self, key: i32) -> Option<(u32, u32)> {
        (key == 1).then_some((50, 100))
    }
    fn vital(&self, key: i32) -> Option<(u32, u32)> {
        (key == 1).then_some((50, 100))
    }
    fn level(&self) -> i32 {
        50
    }
    fn int_property(&self, _: i32) -> i32 {
        -1
    }
    fn bool_property(&self, _: i32) -> bool {
        true
    }
    fn creature_type(&self) -> i32 {
        31
    }
}
fn policy() -> WieldPolicy {
    WieldPolicy {
        enabled: true,
        actor: 123,
        heritage: 3,
        allowed_wielder: None,
        heritage_specific_armor: None,
        valid_locations: 0,
        criteria: [WieldCriterion::default(); 4],
    }
}
#[test]
fn original_ace_wield_requirements_all_kinds_slots_heritages_and_bypass() {
    let mut count = 0;
    for row in include_str!("fixtures/wield.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let words: Vec<_> = row.split_whitespace().collect();
        let n: Vec<i64> = words[1..].iter().map(|s| s.parse().unwrap()).collect();
        let mut p = policy();
        let expected = match words[0] {
            "criterion" => {
                p.criteria[n[2] as usize] = WieldCriterion {
                    kind: n[0] as u32,
                    key: 1,
                    difficulty: n[1] as i32,
                };
                n[3]
            }
            "policy" => {
                p.enabled = n[0] != 0;
                p.heritage = n[1] as u32;
                p.heritage_specific_armor = (n[2] != -1).then_some(n[2] as i32);
                p.valid_locations = n[3] as u32;
                p.allowed_wielder = (n[4] != 0).then_some(n[4] as u32);
                n[5]
            }
            _ => panic!("unexpected fixture"),
        };
        let actual = check_wield_requirements(p, &Values)
            .err()
            .map_or(0, |e| e.weenie_error().unwrap());
        assert_eq!(i64::from(actual), expected, "{row}");
        count += 1;
    }
    assert_eq!(count, 1272);
}
#[test]
fn missing_owned_stat_fails_closed_without_panicking() {
    let mut p = policy();
    p.criteria[0] = WieldCriterion {
        kind: 3,
        key: 99,
        difficulty: 1,
    };
    assert_eq!(
        check_wield_requirements(p, &Values),
        Err(bace_inventory::WieldFailure::MissingValue)
    );
}
