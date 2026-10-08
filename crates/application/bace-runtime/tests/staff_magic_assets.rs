use bace_content::SpellRowV1;
use bace_dat::{SpellBase, SpellTable};
use bace_runtime::staff_magic_assets::*;
use std::collections::BTreeMap;
mod treasure_table_support;
fn base(school: u32, category: u32) -> SpellBase {
    SpellBase {
        name: "equipped spell".into(),
        description: String::new(),
        school,
        icon: 1,
        category,
        flags: 4,
        base_mana: 1,
        range_constant: 0.0,
        range_modifier: 0.0,
        power: 100,
        economy_modifier: 1.0,
        formula_version: 1,
        component_loss: 0.0,
        meta_type: 1,
        meta_id: 1,
        enchantment: Some((30.0, 0.25, 0.5)),
        portal_lifetime: None,
        formula: vec![],
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_modifier: 0,
    }
}
fn row() -> SpellRowV1 {
    serde_json::from_value(serde_json::json!({"id":1,"name":"equipped spell","last_modified":"fixture","stat_mod_type":1,"stat_mod_key":1,"stat_mod_val":10.0})).unwrap()
}

#[test]
fn original_csharp_buff_list_and_enum_ids_cover_all_levels_and_fallback() {
    treasure_table_support::install_tables();
    let vectors: Vec<_> = include_str!("fixtures/staff_buffs.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(',').collect::<Vec<_>>())
        .collect();
    let mut table = SpellTable {
        spells: BTreeMap::new(),
        sets: BTreeMap::new(),
    };
    let mut rows = BTreeMap::new();
    for vector in &vectors {
        let id: u32 = vector[3].parse().unwrap();
        table.spells.insert(id, base(3, 1));
        let mut r = row();
        r.id = id;
        rows.insert(id, r);
    }
    let set_member = *table.spells.keys().next().unwrap();
    table
        .sets
        .insert(77, BTreeMap::from([(2, vec![set_member])]));
    let assets = prepare_staff_magic_assets(&table, &rows).unwrap();
    let expected_auras: std::collections::BTreeSet<u32> = vectors
        .iter()
        .filter(|r| {
            r[0] == "8"
                && r[1] == "Self"
                && matches!(
                    r[2],
                    "BloodDrinker"
                        | "Defender"
                        | "HeartSeeker"
                        | "SpiritDrinker"
                        | "SwiftKiller"
                        | "HermeticLink"
                )
        })
        .map(|r| r[3].parse().unwrap())
        .collect();
    assert_eq!(expected_auras.len(), 6);
    assert_eq!(
        assets
            .enchantments
            .iter()
            .filter(|e| e.is_level8_aura)
            .map(|e| e.spell)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_auras
    );
    assert!(
        assets
            .enchantments
            .iter()
            .find(|e| e.spell == set_member)
            .unwrap()
            .is_set_spell
    );
    assert!(assets.enchantments.iter().all(|e| e.caster == 0
        && e.start_time == 0.
        && e.metadata.degrade_modifier == 0.25
        && e.metadata.degrade_limit == 0.5));

    assert_eq!(assets.plans.len(), 8);
    for plan in &assets.plans {
        let level = plan.level.to_string();
        for (branch, actual) in [("Self", &plan.self_spells), ("Other", &plan.other_spells)] {
            let expected: Vec<u32> = vectors
                .iter()
                .filter(|r| r[0] == level && r[1] == branch && !r[2].starts_with('@'))
                .map(|r| r[3].parse().unwrap())
                .collect();
            assert_eq!(*actual, expected);
        }
        let expected: Vec<u32> = vectors
            .iter()
            .filter(|r| r[0] == level && r[1] == "Self" && r[2].starts_with('@'))
            .map(|r| r[3].parse().unwrap())
            .collect();
        assert_eq!(plan.banes, expected);
        assert!(plan.missing.is_empty());
    }
    let armor = bace_loot::ace_tables::enum_value("SpellId", "ArmorOther8").unwrap() as u32;
    rows.remove(&armor);
    let fallback = prepare_staff_magic_assets(&table, &rows).unwrap();
    assert_eq!(fallback.plans[7].self_spells, fallback.plans[6].self_spells);
    let id = assets.definitions[0].spell;
    let name = &assets.definitions[0].enum_name;
    assert_eq!(
        resolve_staff_spell_name(&name.to_lowercase(), &assets.definitions, true),
        Some(id)
    );
    assert_eq!(
        resolve_staff_spell_name("999999", &assets.definitions, true),
        None
    );
}
