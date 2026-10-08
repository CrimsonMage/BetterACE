use bace_content::*;
use bace_loot::{LootDrop, LootGraph, LootScratch};
use bace_random::RandomRoot;
fn leaf(id: &str, template: u32) -> LootNodeV1 {
    LootNodeV1 {
        mutation: None,
        id: id.into(),
        selection: LootSelectionV1::Item,
        item: Some(LootItemV1 {
            template,
            minimum_stack: 1,
            maximum_stack: 1,
        }),
        branches: vec![],
    }
}
fn branch(id: &str, target: &str) -> LootBranchV1 {
    LootBranchV1 {
        id: id.into(),
        target: target.into(),
        minimum_rolls: 1,
        maximum_rolls: 1,
        weight: None,
        probability: None,
    }
}
fn data() -> LootGraphV1 {
    let mut a = branch("a", "first");
    a.weight = Some(3);
    let mut b = branch("b", "second");
    b.weight = Some(1);
    LootGraphV1 {
        schema_version: 1,
        id: 1,
        root: "root".into(),
        nodes: vec![
            LootNodeV1 {
                mutation: None,
                id: "root".into(),
                selection: LootSelectionV1::All,
                item: None,
                branches: vec![branch("nested", "table")],
            },
            LootNodeV1 {
                mutation: None,
                id: "table".into(),
                selection: LootSelectionV1::Weighted,
                item: None,
                branches: vec![a, b],
            },
            leaf("first", 1),
            leaf("second", 2),
        ],
    }
}
#[test]
fn nested_weighted_tables_replay_and_match_the_configured_distribution() {
    let graph = LootGraph::prepare(data()).unwrap();
    let root = RandomRoot::new([17; 32], 1).unwrap();
    let mut scratch = LootScratch::default();
    let mut output = Vec::with_capacity(256);
    let mut first = 0;
    for event in 1u128..=10000 {
        output.clear();
        graph
            .generate(&root, event.to_le_bytes(), &mut output, &mut scratch)
            .unwrap();
        assert_eq!(output.len(), 1);
        if output[0].template == 1 {
            first += 1;
        }
    }
    // Predetermined binomial acceptance bound around the explicit 3:1 table,
    // not a statement about any historical retail probability.
    assert!((7300..=7700).contains(&first));
    output.clear();
    graph
        .generate(&root, 7u128.to_le_bytes(), &mut output, &mut scratch)
        .unwrap();
    let expected = output.clone();
    output.clear();
    graph
        .generate(&root, 7u128.to_le_bytes(), &mut output, &mut scratch)
        .unwrap();
    assert_eq!(output, expected);
}
#[test]
fn invalid_graphs_and_expansion_limits_never_partially_publish_drops() {
    let mut invalid = data();
    invalid.nodes[1].branches[0].target = "root".into();
    assert!(LootGraph::prepare(invalid).is_err());
    let mut invalid = data();
    invalid.nodes[1].branches[0].target = "absent".into();
    assert!(LootGraph::prepare(invalid).is_err());
    let mut huge = data();
    huge.nodes[0].branches[0].minimum_rolls = 256;
    huge.nodes[0].branches[0].maximum_rolls = 256;
    huge.nodes[1].branches[0].minimum_rolls = 256;
    huge.nodes[1].branches[0].maximum_rolls = 256;
    let graph = LootGraph::prepare(huge).unwrap();
    let root = RandomRoot::new([8; 32], 1).unwrap();
    let mut output = Vec::with_capacity(256);
    output.push(LootDrop {
        template: 99,
        stack: 1,
        node: "existing".into(),
        mutations: Vec::new(),
    });
    let before = output.clone();
    assert!(
        graph
            .generate(&root, [1; 16], &mut output, &mut LootScratch::default())
            .is_err()
    );
    assert_eq!(output, before);
}
#[test]
fn cumulative_residual_no_drop_is_not_normalized_away() {
    let mut d = data();
    d.nodes[1].selection = LootSelectionV1::OrderedCumulative;
    for b in &mut d.nodes[1].branches {
        b.weight = None;
        b.probability = Some(ProbabilityV1 {
            numerator: 0,
            denominator: 10,
        });
    }
    let graph = LootGraph::prepare(d).unwrap();
    let mut output = Vec::with_capacity(8);
    graph
        .generate(
            &RandomRoot::new([1; 32], 1).unwrap(),
            [1; 16],
            &mut output,
            &mut LootScratch::default(),
        )
        .unwrap();
    assert!(output.is_empty());
}

#[test]
fn item_subtables_apply_bounded_material_stat_and_spell_mutations() {
    let mut graph = data();
    graph.nodes[0].branches[0].target = "first".into();
    graph.nodes[2].branches = vec![branch("stats", "stats")];
    graph.nodes.push(LootNodeV1 {
        id: "stats".into(),
        selection: LootSelectionV1::All,
        item: None,
        mutation: None,
        branches: vec![branch("damage", "damage"), branch("spell", "spell")],
    });
    graph.nodes.push(LootNodeV1 {
        id: "damage".into(),
        selection: LootSelectionV1::Mutation,
        item: None,
        mutation: Some(LootMutationV1::Int {
            property: 44,
            minimum: 12,
            maximum: 12,
        }),
        branches: vec![],
    });
    graph.nodes.push(LootNodeV1 {
        id: "spell".into(),
        selection: LootSelectionV1::Mutation,
        item: None,
        mutation: Some(LootMutationV1::Spell { spell: 100 }),
        branches: vec![],
    });
    let compiled = LootGraph::prepare(graph.clone()).unwrap();
    let mut out = Vec::with_capacity(256);
    compiled
        .generate(
            &RandomRoot::new([3; 32], 1).unwrap(),
            [1; 16],
            &mut out,
            &mut LootScratch::default(),
        )
        .unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].mutations.len(), 2);
    let template = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "synthetic-item".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    let item = bace_loot::materialize_drop(&out[0], &template).unwrap();
    assert_eq!(item.properties.ints[0].value, 12);
    assert_eq!(item.properties.spell_book[0].value, 2.0);
    assert!(template.properties.ints.is_empty());
    graph.root = "damage".into();
    assert!(LootGraph::prepare(graph).is_err());
}

#[test]
fn ordinary_loot_cannot_claim_the_reserved_rare_award_tag() {
    let graph = LootGraphV1 {
        schema_version: 1,
        id: 1,
        root: "$rare".into(),
        nodes: vec![leaf("$rare", 1)],
    };
    assert!(LootGraph::prepare(graph).is_err());
}
