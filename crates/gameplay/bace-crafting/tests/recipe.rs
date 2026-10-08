use bace_crafting::*;
use bace_random::RandomRoot;
use std::collections::BTreeMap;

fn key(id: u32) -> PropertyKey {
    PropertyKey {
        kind: PropertyKind::Int,
        id,
    }
}
fn item(id: u32) -> CraftItem {
    CraftItem {
        id,
        owner: 1,
        revision: 1,
        stack: 1,
        equipped: false,
        in_trade: false,
        reserved: false,
        times_tinkered: 0,
        tinker_log: vec![],
        properties: BTreeMap::from([(key(28), PropertyValue::Int(10))]),
    }
}
fn context() -> CraftContext {
    CraftContext {
        actor: 1,
        actor_revision: 1,
        character_random_id: [1; 16],
        operation_id: [2; 16],
        busy: false,
        peace_mode: true,
        chance: ChanceInput {
            skill: 1000,
            trained: true,
            lum_craft: 0,
            tool_workmanship: 10.0,
            target_workmanship: 10.0,
            material: 0x3d,
            times_tinkered: 0,
            imbue: false,
            imbue_augmentation: false,
            foolproof: true,
        },
        properties: BTreeMap::new(),
    }
}
fn recipe() -> PreparedRecipe {
    PreparedRecipe {
        id: 1,
        revision: 1,
        requirements: vec![Requirement {
            participant: Participant::Target,
            key: key(28),
            comparison: RequirementComparison::AtLeast,
            value: PropertyValue::Int(10),
            absent_is_default: true,
        }],
        success: RecipeBranch {
            consume_source: 1,
            consume_target: 0,
            destroy_source_chance: 1.0,
            destroy_target_chance: 0.0,
            mutations: vec![Mutation {
                participant: Participant::Target,
                key: key(28),
                kind: MutationKind::Add(PropertyValue::Int(1)),
            }],
        },
        failure: RecipeBranch {
            consume_source: 1,
            consume_target: 1,
            destroy_source_chance: 1.0,
            destroy_target_chance: 1.0,
            mutations: vec![],
        },
        increment_tinker_count: true,
        proficiency: None,
    }
}

#[test]
fn proposal_does_not_mutate_source_and_retry_cannot_reroll() {
    let c = context();
    let source = item(2);
    let target = item(3);
    let r = recipe();
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let quote = quote_craft(&c, &source, &target, &r, 100, 100).unwrap();
    let p = propose_craft(&c, &source, &target, &r, &quote, 100, &root).unwrap();
    assert!(p.success);
    assert!(p.source_after.is_none());
    let after = p.target_after.as_ref().unwrap();
    assert_eq!(
        after.properties.get(&key(28)),
        Some(&PropertyValue::Int(11))
    );
    assert_eq!(after.times_tinkered, 1);
    assert_eq!(after.tinker_log, [0x3d]);
    assert_eq!(after.revision, 2);
    assert_eq!(source, item(2));
    assert_eq!(target, item(3));
    assert_eq!(
        p,
        propose_craft(&c, &source, &target, &r, &quote, 101, &root).unwrap()
    );
    assert_eq!(
        roll_tinker(c.chance, &root, c.character_random_id, c.operation_id).unwrap(),
        (p.chance, p.success)
    );
}

#[test]
fn confirmation_rechecks_every_authoritative_input() {
    let c = context();
    let source = item(2);
    let target = item(3);
    let r = recipe();
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let quote = quote_craft(&c, &source, &target, &r, 100, 100).unwrap();
    assert_eq!(
        propose_craft(&c, &source, &target, &r, &quote, 200, &root),
        Err(CraftError::StaleConfirmation)
    );
    let mut modified = target.clone();
    modified.revision += 1;
    assert_eq!(
        propose_craft(&c, &source, &modified, &r, &quote, 100, &root),
        Err(CraftError::StaleConfirmation)
    );
    let mut modified = c.clone();
    modified.chance.skill = 999;
    assert_eq!(
        propose_craft(&modified, &source, &target, &r, &quote, 100, &root),
        Err(CraftError::StaleConfirmation)
    );
    let mut modified = r.clone();
    modified.revision += 1;
    assert_eq!(
        propose_craft(&c, &source, &target, &modified, &quote, 100, &root),
        Err(CraftError::StaleConfirmation)
    );
}

#[test]
fn invalid_owner_trade_busy_requirement_and_overflow_are_atomic() {
    let c = context();
    let source = item(2);
    let target = item(3);
    let r = recipe();
    let mut invalid = source.clone();
    invalid.owner = 9;
    assert_eq!(
        quote_craft(&c, &invalid, &target, &r, 0, 20),
        Err(CraftError::Ownership)
    );
    invalid = source.clone();
    invalid.in_trade = true;
    assert_eq!(
        quote_craft(&c, &invalid, &target, &r, 0, 20),
        Err(CraftError::Busy)
    );
    assert_eq!(
        quote_craft(&c, &target, &target, &r, 0, 20),
        Err(CraftError::Duplicate)
    );
    invalid = target.clone();
    invalid.properties.insert(key(28), PropertyValue::Int(9));
    assert_eq!(
        quote_craft(&c, &source, &invalid, &r, 0, 20),
        Err(CraftError::Requirement)
    );
    invalid = target.clone();
    invalid
        .properties
        .insert(key(28), PropertyValue::Int(i32::MAX));
    let q = quote_craft(&c, &source, &invalid, &r, 0, 20).unwrap();
    let root = RandomRoot::new([7; 32], 1).unwrap();
    assert_eq!(
        propose_craft(&c, &source, &invalid, &r, &q, 0, &root),
        Err(CraftError::Overflow)
    );
    assert_eq!(invalid.properties[&key(28)], PropertyValue::Int(i32::MAX));
}

#[test]
fn definite_failure_proposes_both_consumptions_without_live_mutation() {
    let mut c = context();
    c.chance.foolproof = false;
    c.chance.skill = 0;
    c.chance.times_tinkered = 9;
    c.chance.target_workmanship = 1000.0;
    let source = item(2);
    let mut target = item(3);
    target.times_tinkered = 9;
    let r = recipe();
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let q = quote_craft(&c, &source, &target, &r, 0, 20).unwrap();
    let p = propose_craft(&c, &source, &target, &r, &q, 0, &root).unwrap();
    assert!(!p.success);
    assert!(p.source_after.is_none());
    assert!(p.target_after.is_none());
    assert_eq!(target.times_tinkered, 9);
}

#[test]
fn property_operations_are_ordered_and_type_checked() {
    let c = context();
    let source = item(2);
    let target = item(3);
    let mut r = recipe();
    let root = RandomRoot::new([5; 32], 1).unwrap();
    r.success.mutations = vec![
        Mutation {
            participant: Participant::Target,
            key: key(28),
            kind: MutationKind::SetBitsOn(16),
        },
        Mutation {
            participant: Participant::Target,
            key: key(28),
            kind: MutationKind::SetBitsOff(2),
        },
        Mutation {
            participant: Participant::Actor,
            key: key(5),
            kind: MutationKind::Copy {
                participant: Participant::Target,
                key: key(28),
            },
        },
    ];
    let q = quote_craft(&c, &source, &target, &r, 0, 20).unwrap();
    let p = propose_craft(&c, &source, &target, &r, &q, 0, &root).unwrap();
    assert_eq!(p.actor_properties[&key(5)], PropertyValue::Int(24));
    assert_eq!(p.actor_revision, 2);
    r.success.mutations[0].kind = MutationKind::Set(PropertyValue::Bool(true));
    let q = quote_craft(&c, &source, &target, &r, 0, 20).unwrap();
    assert_eq!(
        propose_craft(&c, &source, &target, &r, &q, 0, &root),
        Err(CraftError::InvalidState)
    );
}

#[test]
fn retained_recipe_payload_is_bounded_even_on_unused_branch() {
    let mut recipe = recipe();
    recipe.failure.mutations = (0..17)
        .map(|id| Mutation {
            participant: Participant::Target,
            key: PropertyKey {
                kind: PropertyKind::String,
                id: id + 1,
            },
            kind: MutationKind::Set(PropertyValue::String("x".repeat(4096))),
        })
        .collect();
    assert_eq!(recipe.validate_bounds(), Err(CraftError::Capacity));
    assert_eq!(
        quote_craft(&context(), &item(2), &item(3), &recipe, 0, 10),
        Err(CraftError::Capacity)
    );
}

#[test]
fn confirmation_rebase_preserves_draw_expiry_and_all_recipe_reads() {
    let mut c = context();
    c.properties.insert(key(125), PropertyValue::Int(1));
    c.properties.insert(key(10), PropertyValue::Int(4));
    let source = item(2);
    let target = item(3);
    let mut r = recipe();
    r.requirements.push(Requirement {
        participant: Participant::Actor,
        key: key(10),
        comparison: RequirementComparison::AtLeast,
        value: PropertyValue::Int(3),
        absent_is_default: true,
    });
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let q = quote_craft(&c, &source, &target, &r, 100, 100).unwrap();
    let original = propose_craft(&c, &source, &target, &r, &q, 100, &root).unwrap();
    let mut newer = c.clone();
    newer.actor_revision = 7;
    newer.properties.insert(key(125), PropertyValue::Int(30));
    let rebased = propose_confirmed_craft(&newer, &source, &target, &r, &q, 199, &root).unwrap();
    assert_eq!(original.success, rebased.success);
    assert_eq!(original.source_after, rebased.source_after);
    assert_eq!(original.target_after, rebased.target_after);
    assert_eq!(rebased.expected_actor_revision, 7);
    assert_eq!(
        rebased.actor_before_properties[&key(125)],
        PropertyValue::Int(30)
    );
    assert_eq!(
        propose_confirmed_craft(&newer, &source, &target, &r, &q, 200, &root),
        Err(CraftError::StaleConfirmation)
    );
    newer.properties.insert(key(10), PropertyValue::Int(5));
    assert_eq!(
        propose_confirmed_craft(&newer, &source, &target, &r, &q, 199, &root),
        Err(CraftError::StaleConfirmation)
    );
    newer = c.clone();
    newer.chance.skill += 1;
    assert_eq!(
        propose_confirmed_craft(&newer, &source, &target, &r, &q, 199, &root),
        Err(CraftError::StaleConfirmation)
    );
    newer = c.clone();
    newer.operation_id = [8; 16];
    assert_eq!(
        propose_confirmed_craft(&newer, &source, &target, &r, &q, 199, &root),
        Err(CraftError::StaleConfirmation)
    );
}

#[test]
fn original_source_dialog_rounding_and_tinker_broadcast_vectors() {
    for line in include_str!("fixtures/tinker_messages.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] == "C" {
            let chance: f64 = fields[1].parse().unwrap();
            assert_eq!(
                tinker_confirmation_text(chance, false).unwrap(),
                format!(
                    "You determine that you have a {} percent chance to succeed.",
                    fields[2]
                )
            );
            continue;
        }
        let n: usize = fields[1].parse().unwrap();
        let mut ctx = context();
        ctx.chance.tool_workmanship = [1., 2.675, 5., 9.99][n % 4];
        ctx.properties.insert(
            PropertyKey {
                kind: PropertyKind::String,
                id: 1,
            },
            PropertyValue::String("Alice".into()),
        );
        let mut source = item(2);
        let mut target = item(3);
        source.properties.insert(
            PropertyKey {
                kind: PropertyKind::String,
                id: 1,
            },
            PropertyValue::String(
                if n.is_multiple_of(2) {
                    "Salvage (100)"
                } else {
                    "Steel Salvaged"
                }
                .into(),
            ),
        );
        source.properties.insert(key(131), PropertyValue::Int(64));
        target.properties.insert(
            PropertyKey {
                kind: PropertyKind::String,
                id: 1,
            },
            PropertyValue::String(
                if n.is_multiple_of(2) {
                    "Gold Sword"
                } else {
                    "Sword"
                }
                .into(),
            ),
        );
        if n.is_multiple_of(2) {
            target.properties.insert(key(131), PropertyValue::Int(2));
        }
        if n.is_multiple_of(3) {
            target.properties.insert(
                PropertyKey {
                    kind: PropertyKind::String,
                    id: 7,
                },
                PropertyValue::String("inscribed".into()),
            );
        }
        target.properties.insert(
            PropertyKey {
                kind: PropertyKind::String,
                id: 8,
            },
            PropertyValue::String("Bob".into()),
        );
        assert_eq!(
            tinker_result_text(
                &ctx,
                &source,
                &target,
                &BTreeMap::from([(64, "Steel".into()), (2, "Gold".into())]),
                n < 4
            )
            .unwrap(),
            fields[2],
            "case {n}"
        );
    }
}

#[test]
fn explicit_mutation_journal_retains_intermediate_values_but_not_script_packets() {
    let mut r = recipe();
    r.success.mutations.push(Mutation {
        participant: Participant::Target,
        key: key(28),
        kind: MutationKind::Add(PropertyValue::Int(2)),
    });
    let c = context();
    let s = item(2);
    let t = item(3);
    let q = quote_craft(&c, &s, &t, &r, 0, 20).unwrap();
    let proposal =
        propose_craft(&c, &s, &t, &r, &q, 1, &RandomRoot::new([1; 32], 1).unwrap()).unwrap();
    assert_eq!(
        proposal
            .property_updates
            .iter()
            .map(|u| u.value.clone())
            .collect::<Vec<_>>(),
        [PropertyValue::Int(11), PropertyValue::Int(13)]
    );
}
