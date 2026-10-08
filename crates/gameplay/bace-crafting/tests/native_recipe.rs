mod native_recipe {
    pub mod common;
}
use bace_content::*;
use bace_crafting::*;
use bace_random::RandomRoot;
use native_recipe::common::*;
#[test]
fn original_ace_imported_modification_vectors() {
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let mut count = 0;
    for line in include_str!("fixtures/recipe_modifications.tsv")
        .lines()
        .filter(|l| l.starts_with("M\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let index = f[2].parse().unwrap();
        let source_id = f[3].parse().unwrap();
        let seed = f[4].parse::<u32>().unwrap();
        let operation = f[5].parse().unwrap();
        let stat = f[6].parse::<i32>().unwrap();
        let mut c = context();
        c.properties = properties(1, seed, stat as u32);
        let mut s = item(2);
        s.properties = properties(2, seed, stat as u32);
        let mut t = item(3);
        t.properties = properties(3, seed, stat as u32);
        let mods = [RecipeModRowV1 {
            id: 1,
            recipe_id: 1,
            executes_on_success: true,
            health: 0,
            stamina: 0,
            mana: 0,
            unknown_7: false,
            data_id: 0,
            unknown_9: 0,
            instance_id: 0,
        }];
        macro_rules! prepare {
            ($field:ident,$type:ident,$value:expr) => {{
                let values = [$type {
                    id: 1,
                    recipe_mod_id: 1,
                    index,
                    stat,
                    value: $value,
                    r#enum: operation,
                    source: source_id,
                }];
                prepare_native_recipe(
                    &source(),
                    1,
                    NativeRecipeRows {
                        mods: &mods,
                        $field: &values,
                        ..Default::default()
                    },
                )
                .unwrap()
            }};
        }
        let recipe = match f[1] {
            "Bool" => prepare!(bool_mods, RecipeModsBoolRowV1, false),
            "Int" => prepare!(int_mods, RecipeModsIntRowV1, 5),
            "Float" => prepare!(float_mods, RecipeModsFloatRowV1, 0.3),
            "String" => prepare!(
                string_mods,
                RecipeModsStringRowV1,
                if seed == 2 {
                    None
                } else {
                    Some("literal".into())
                }
            ),
            "IID" => prepare!(iid_mods, RecipeModsIIDRowV1, 50),
            "DID" => prepare!(did_mods, RecipeModsDIDRowV1, 50),
            _ => panic!("fixture"),
        };
        let quote = quote_craft(&c, &s, &t, &recipe.recipe, 0, 30).unwrap();
        let proposal = propose_craft(&c, &s, &t, &recipe.recipe, &quote, 1, &root).unwrap();
        assert_eq!(proposal.actor_properties, parse(f[7]), "{line}");
        assert_eq!(
            proposal.source_after.unwrap().properties,
            parse(f[8]),
            "{line}"
        );
        assert_eq!(
            proposal.target_after.unwrap().properties,
            parse(f[9]),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 768);
}
#[test]
fn original_ace_failure_predicates_invert_once_preserving_absence() {
    let mut count = 0;
    for line in include_str!("fixtures/recipe_modifications.tsv")
        .lines()
        .filter(|l| l.starts_with("R\t") || l.starts_with("T\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let operation = f[1].parse().unwrap();
        let mut target = item(3);
        let native = if f[0] == "R" {
            let value: f64 = f[3].parse().unwrap();
            let rows = [RecipeRequirementsFloatRowV1 {
                id: 1,
                recipe_id: 1,
                index: 0,
                stat: 123,
                value,
                r#enum: operation,
                message: Some("failure".into()),
            }];
            if f[2] != "null" {
                target.properties.insert(
                    PropertyKey {
                        kind: PropertyKind::Float,
                        id: 123,
                    },
                    PropertyValue::Float(f[2].parse().unwrap()),
                );
            }
            prepare_native_recipe(
                &source(),
                1,
                NativeRecipeRows {
                    float_requirements: &rows,
                    ..Default::default()
                },
            )
            .unwrap()
        } else {
            let rows = [RecipeRequirementsStringRowV1 {
                id: 1,
                recipe_id: 1,
                index: 0,
                stat: 123,
                value: Some(f[3].into()),
                r#enum: operation,
                message: Some("failure".into()),
            }];
            if f[2] != "null" {
                target.properties.insert(
                    PropertyKey {
                        kind: PropertyKind::String,
                        id: 123,
                    },
                    PropertyValue::String(f[2].into()),
                );
            }
            prepare_native_recipe(
                &source(),
                1,
                NativeRecipeRows {
                    string_requirements: &rows,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let result = quote_craft(&context(), &item(2), &target, &native.recipe, 0, 30);
        assert_eq!(result.is_ok(), f[4] == "1", "{line}: {result:?}");
        count += 1;
    }
    assert_eq!(count, 1149);
}
#[test]
fn script_count_and_log_adopt_exactly_once_in_proposal() {
    let mods = [RecipeModRowV1 {
        id: 1,
        recipe_id: 1,
        executes_on_success: true,
        health: 0,
        stamina: 0,
        mana: 0,
        unknown_7: false,
        data_id: 0x38000011,
        unknown_9: 0,
        instance_id: 0,
    }];
    let r = prepare_native_recipe(
        &source(),
        1,
        NativeRecipeRows {
            mods: &mods,
            ..Default::default()
        },
    )
    .unwrap();
    let c = context();
    let s = item(2);
    let t = item(3);
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let q = quote_craft(&c, &s, &t, &r.recipe, 0, 30).unwrap();
    let p = propose_craft(&c, &s, &t, &r.recipe, &q, 1, &root).unwrap();
    let after = p.target_after.unwrap();
    assert_eq!(after.times_tinkered, 1);
    assert_eq!(after.tinker_log, [61]);
    assert_eq!(
        after.properties[&PropertyKey {
            kind: PropertyKind::Int,
            id: 28
        }],
        PropertyValue::Int(20)
    );
    assert_eq!(
        after.properties[&PropertyKey {
            kind: PropertyKind::Int,
            id: 171
        }],
        PropertyValue::Int(1)
    );
    assert!(!r.recipe.increment_tinker_count);
}
#[test]
fn bounded_joins_and_unowned_side_effects_fail_before_quote() {
    let mut row = source();
    row.success_w_c_i_d = 100;
    assert_eq!(
        prepare_native_recipe(&row, 1, NativeRecipeRows::default()),
        Err(CraftError::Unsupported)
    );
    row.success_w_c_i_d = 0;
    let mut m = RecipeModRowV1 {
        id: 1,
        recipe_id: 1,
        executes_on_success: true,
        health: -1,
        stamina: 0,
        mana: 0,
        unknown_7: false,
        data_id: 0,
        unknown_9: 0,
        instance_id: 0,
    };
    assert_eq!(
        prepare_native_recipe(
            &row,
            1,
            NativeRecipeRows {
                mods: std::slice::from_ref(&m),
                ..Default::default()
            }
        ),
        Err(CraftError::Unsupported)
    );
    m.health = 0;
    m.data_id = 123;
    assert_eq!(
        prepare_native_recipe(
            &row,
            1,
            NativeRecipeRows {
                mods: &[m],
                ..Default::default()
            }
        ),
        Err(CraftError::Unsupported)
    );
    let mods = [RecipeModsIntRowV1 {
        id: 1,
        recipe_mod_id: 99,
        index: 0,
        stat: 1,
        value: 1,
        r#enum: 1,
        source: 0,
    }];
    assert_eq!(
        prepare_native_recipe(
            &row,
            1,
            NativeRecipeRows {
                int_mods: &mods,
                ..Default::default()
            }
        ),
        Err(CraftError::InvalidState)
    );
}
