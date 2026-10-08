use bace_magic::{MagicProcItem, MagicSchool, magic_cloak_projectile_skill, magic_item_skill};
#[test]
fn original_item_spellcraft_school_order_and_cloak_vectors() {
    for row in include_str!("fixtures/proc_skill.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut parts = row.split(',');
        let kind = parts.next().unwrap();
        let p: Vec<i32> = parts.map(|v| v.parse().unwrap()).collect();
        let mut item = MagicProcItem {
            item: 1,
            spellcraft: None,
            skills: [111, 222, 333, 444, 555],
            cloak: false,
            wield_difficulty: None,
        };
        if kind == "skill" {
            item.spellcraft = (p[0] >= 0).then_some(p[0] as u32);
            let school = match p[1] {
                34 => Some(MagicSchool::War),
                33 => Some(MagicSchool::Life),
                31 => Some(MagicSchool::Creature),
                32 => Some(MagicSchool::Item),
                43 => Some(MagicSchool::Void),
                _ => None,
            };
            assert_eq!(magic_item_skill(&item, school, 0), p[2] as u32, "{row}");
        } else {
            item.cloak = p[2] != 0;
            item.wield_difficulty = (p[1] >= 0).then_some(p[1] as u32);
            assert_eq!(
                magic_cloak_projectile_skill(&item, p[0] as u32, 77).unwrap(),
                p[3] as u32,
                "{row}"
            );
        }
    }
}
