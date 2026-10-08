use bace_content::{Property, SpellRowV1, WeenieV1};
use bace_dat::{SpellBase, SpellTable};
use bace_runtime::generator_enchantments::prepare_generator_item_enchantments;
use bace_types::EntityId;
use std::collections::BTreeMap;
fn template() -> WeenieV1 {
    let mut item = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "equipped_magic".into(),
        weenie_type: 6,
        last_modified: None,
        properties: Default::default(),
    };
    item.properties
        .spell_book
        .push(Property { id: 1, value: 2.0 });
    item
}
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
fn source_compiled_create_item_spell_routes_all_schools_and_legacy_aura_categories() {
    let item = template();
    let rows = BTreeMap::from([(1, row())]);
    let fixture = include_str!("fixtures/ace_generator_item_spell_routes.tsv");
    let mut count = 0;
    for line in fixture.lines() {
        let fields: Vec<u32> = line
            .split('\t')
            .map(|value| value.parse().unwrap())
            .collect();
        let table = SpellTable {
            spells: BTreeMap::from([(1, base(fields[0], fields[1]))]),
            sets: BTreeMap::new(),
        };
        let entries =
            prepare_generator_item_enchantments(EntityId(10), EntityId(20), &item, &table, &rows)
                .unwrap();
        if fields[2] == 0 {
            assert!(entries.is_empty());
        } else {
            assert_eq!(entries.len(), 1);
            assert_eq!(
                entries[0].target,
                EntityId(if fields[2] == 1 { 10 } else { 20 })
            );
            assert_eq!(entries[0].entry.caster, 20);
            assert_eq!(entries[0].entry.spec.stat_type, 0x02000001);
            assert_eq!(
                entries[0].entry.spec.duration, 30.0,
                "equipped permanence is assigned by registry add at activation"
            );
        }
        count += 1;
    }
    assert_eq!(count, 40);
}
#[test]
fn unsupported_effects_missing_assets_and_periodic_proc_cases_are_explicit_rejections() {
    let item = template();
    let rows = BTreeMap::from([(1, row())]);
    for meta_type in [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
        let mut spell = base(3, 1);
        spell.meta_type = meta_type;
        let table = SpellTable {
            spells: BTreeMap::from([(1, spell)]),
            sets: BTreeMap::new(),
        };
        assert!(
            prepare_generator_item_enchantments(EntityId(10), EntityId(20), &item, &table, &rows)
                .is_err()
        );
    }
    let mut table = SpellTable {
        spells: BTreeMap::from([(1, base(3, 1))]),
        sets: BTreeMap::new(),
    };
    assert!(
        prepare_generator_item_enchantments(
            EntityId(10),
            EntityId(20),
            &item,
            &table,
            &BTreeMap::new()
        )
        .is_err()
    );
    table.spells.get_mut(&1).unwrap().flags |= 0x10000;
    assert!(
        prepare_generator_item_enchantments(EntityId(10), EntityId(20), &item, &table, &rows)
            .is_err()
    );
    table.spells.clear();
    assert!(
        prepare_generator_item_enchantments(EntityId(10), EntityId(20), &item, &table, &rows)
            .is_err()
    );
}
#[test]
fn exact_item_set_membership_and_degradation_metadata_are_retained() {
    let mut item = template();
    item.properties.ints.push(Property { id: 265, value: 2 });
    let table = SpellTable {
        spells: BTreeMap::from([(1, base(3, 1))]),
        sets: BTreeMap::from([(2, BTreeMap::from([(7, vec![1])]))]),
    };
    let entries = prepare_generator_item_enchantments(
        EntityId(10),
        EntityId(20),
        &item,
        &table,
        &BTreeMap::from([(1, row())]),
    )
    .unwrap();
    assert!(entries[0].entry.metadata.has_spell_set_id);
    assert_eq!(entries[0].entry.metadata.spell_set_id, 2);
    assert_eq!(entries[0].entry.metadata.degrade_modifier, 0.25);
    assert_eq!(entries[0].entry.metadata.degrade_limit, 0.5);
    assert_eq!(entries[0].entry.spec.set_id, Some(2));
}

#[test]
fn source_no_op_war_and_void_spellbooks_need_no_server_effect_rows() {
    for school in [1, 5] {
        let table = SpellTable {
            spells: BTreeMap::from([(1, base(school, 1))]),
            sets: BTreeMap::new(),
        };
        let entries = prepare_generator_item_enchantments(
            EntityId(10),
            EntityId(20),
            &template(),
            &table,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(entries.is_empty());
    }
}
