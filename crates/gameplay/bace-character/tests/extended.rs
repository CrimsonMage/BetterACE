use bace_character::*;
use std::sync::Arc;
fn character(available: i64) -> CharacterProgression {
    let t = RankTable::new(&[0, 10]).unwrap();
    CharacterProgression::new(
        &[],
        Arc::new(ProgressionTables {
            attributes: t.clone(),
            vitals: t.clone(),
            trained_skills: t.clone(),
            specialized_skills: t,
        }),
        0,
        0,
    )
    .unwrap()
    .with_luminance(LuminanceState {
        available,
        maximum: 1000,
    })
    .unwrap()
}
#[test]
fn pinned_official_luminance_aetheria_rate_and_item_xp_vectors() {
    for line in include_str!("fixtures/extended.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        match p[0] {
            "lum" => {
                let mut c = character(p[1].parse().unwrap());
                let credit = c
                    .propose_luminance(
                        p[2].parse().unwrap(),
                        false,
                        LuminanceModifiers {
                            luminance: p[3].parse().unwrap(),
                            quest: 1.5,
                            enchantment: p[4].parse().unwrap(),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                assert_eq!(credit.after.available, p[5].parse::<i64>().unwrap());
                c.adopt_luminance(credit).unwrap();
                assert_eq!(c.luminance(), Some(credit.after));
            }
            "spend" => {
                let c = character(p[1].parse().unwrap());
                let result =
                    c.propose_luminance(p[2].parse().unwrap(), true, LuminanceModifiers::default());
                assert_eq!(result.is_ok(), p[3] == "True");
                if let Ok(change) = result {
                    assert_eq!(change.after.available, p[4].parse::<i64>().unwrap());
                }
            }
            "proc" => assert_eq!(
                aetheria_proc_rate(
                    p[1].parse().unwrap(),
                    p[2].parse().unwrap(),
                    p[3].parse().unwrap()
                )
                .unwrap()
                .to_bits(),
                p[4].parse::<u32>().unwrap()
            ),
            "itemxp" | "itemlevel" => {
                let style = match p[1] {
                    "1" => ItemExperienceStyle::Fixed,
                    "2" => ItemExperienceStyle::ScalesWithLevel,
                    "3" => ItemExperienceStyle::FixedPlusBase,
                    _ => continue,
                };
                let mut item = ItemExperience {
                    total: 0,
                    base: 100,
                    maximum_level: 5,
                    style,
                    revision: 0,
                };
                if p[0] == "itemxp" {
                    assert_eq!(
                        item.xp_for_level(p[2].parse().unwrap()).unwrap(),
                        p[3].parse::<u64>().unwrap()
                    );
                } else {
                    item.total = p[2].parse().unwrap();
                    if item.total <= item.xp_for_level(5).unwrap() {
                        assert_eq!(item.level().unwrap(), p[3].parse::<u32>().unwrap());
                    }
                }
            }
            _ => panic!("unexpected fixture row"),
        }
    }
}
#[test]
fn grants_spends_and_item_progression_retain_state_until_matching_adoption() {
    let mut c = character(20);
    let grant = c
        .propose_luminance(15, false, LuminanceModifiers::default())
        .unwrap();
    assert_eq!(c.luminance().unwrap().available, 20);
    assert!(
        c.propose_luminance(-1, true, LuminanceModifiers::default())
            .is_err()
    );
    assert!(
        c.propose_luminance(30, true, LuminanceModifiers::default())
            .is_err()
    );
    c.adopt_luminance(grant).unwrap();
    assert_eq!(c.adopt_luminance(grant), Err(LuminanceError::Conflict));
    let mut item = ItemExperience {
        total: 0,
        base: 100,
        maximum_level: 5,
        style: ItemExperienceStyle::ScalesWithLevel,
        revision: 0,
    };
    let change = item.propose_xp(750).unwrap();
    assert_eq!((change.before_level, change.after_level), (0, 3));
    item.adopt(change).unwrap();
    assert_eq!(item.total, 750);
    assert_eq!(item.adopt(change), Err(AetheriaError::Conflict));
}
