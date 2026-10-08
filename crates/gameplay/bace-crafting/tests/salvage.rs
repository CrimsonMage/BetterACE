use bace_crafting::*;
use std::collections::BTreeMap;

fn item(id: u32) -> SalvageInput {
    SalvageInput {
        id,
        owner: 1,
        revision: 1,
        stack: 1,
        material: 0x3d,
        raw_workmanship: 10,
        value: 100,
        retained: false,
        equipped: false,
        in_trade: false,
        reserved: false,
        is_salvage: false,
        structure: 0,
        num_items: 0,
    }
}
fn request<'a>(items: &'a [SalvageInput], templates: &'a BTreeMap<u32, u32>) -> SalvageRequest<'a> {
    SalvageRequest {
        actor: 1,
        actor_revision: 1,
        operation_id: [1; 16],
        tool: SalvageTool {
            id: 2,
            owner: 1,
            revision: 1,
            is_ust: true,
            reserved: false,
        },
        skills: SalvageSkills {
            salvaging: 195,
            armor: 0,
            weapon: 0,
            magic_item: 0,
            item: 0,
            augmentations: 0,
        },
        items,
        bag_templates: templates,
        free_slots: 300,
    }
}

#[test]
fn verbatim_gdle_scalar_vectors() {
    let mut count = 0;
    for line in include_str!("fixtures/salvage.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<u32> = line.split(',').map(|s| s.parse().unwrap()).collect();
        assert_eq!(
            salvage_amount(v[0], v[1], v[2] as i32).unwrap(),
            v[3],
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 135);
    assert_eq!(salvage_amount(195, 10, 100), salvage_amount(195, 10, 4));
    assert_eq!(salvage_amount(195, 10, -1), salvage_amount(195, 10, 0));
}

#[test]
fn splitting_preserves_all_units_value_and_workmanship() {
    let templates = BTreeMap::from([(0x3d, 20983)]);
    let items = [
        SalvageInput {
            raw_workmanship: 100,
            ..item(3)
        },
        item(4),
    ];
    let result = propose_salvage(request(&items, &templates)).unwrap();
    assert_eq!(result.bags.len(), 2);
    assert_eq!(result.bags.iter().map(|b| b.units).sum::<u32>(), 112);
    assert_eq!(result.bags.iter().map(|b| b.value).sum::<u32>(), 100);
    assert!(
        result.bags.iter().all(
            |b| b.units <= 100 && f64::from(b.raw_workmanship) / f64::from(b.num_items) == 55.0
        )
    );
    assert_eq!(result.consumed, [(3, 1), (4, 1)]);
    assert_eq!(
        result
            .results
            .iter()
            .map(|result| result.units)
            .collect::<Vec<_>>(),
        [100, 12]
    );
    let mut insufficient = request(&items, &templates);
    insufficient.free_slots = 1;
    assert_eq!(propose_salvage(insufficient), Err(CraftError::Capacity));
}

#[test]
fn unsuitable_only_and_stacked_inputs_remain_unconsumed() {
    let templates = BTreeMap::from([(0x3d, 20983)]);
    let items = [
        SalvageInput {
            stack: 2,
            ..item(3)
        },
        SalvageInput {
            retained: true,
            ..item(4)
        },
        SalvageInput {
            owner: 5,
            ..item(5)
        },
        SalvageInput {
            material: 0,
            ..item(6)
        },
        SalvageInput {
            in_trade: true,
            ..item(7)
        },
    ];
    let result = propose_salvage(request(&items, &templates)).unwrap();
    assert!(result.consumed.is_empty());
    assert!(result.results.is_empty());
    assert!(result.bags.is_empty());
    assert_eq!(result.unsuitable.len(), 5);
    assert_eq!(result.unsuitable[0], (3, UnsupportedSalvage::Stacked));
    let empty = propose_salvage(request(&[], &templates)).unwrap();
    assert!(empty.results.is_empty());
}

#[test]
fn duplicates_unknown_templates_and_output_failures_cannot_destroy_inputs() {
    let templates = BTreeMap::from([(0x3d, 20983)]);
    let duplicate = [item(3), item(3)];
    assert_eq!(
        propose_salvage(request(&duplicate, &templates)),
        Err(CraftError::Duplicate)
    );
    let items = [item(3)];
    let missing = BTreeMap::new();
    let result = propose_salvage(request(&items, &missing)).unwrap();
    assert!(result.consumed.is_empty());
    let invalid = BTreeMap::from([(0x3d, 0)]);
    assert_eq!(
        propose_salvage(request(&items, &invalid)),
        Err(CraftError::InvalidState)
    );
    let mut req = request(&items, &templates);
    req.tool.is_ust = false;
    assert_eq!(propose_salvage(req), Err(CraftError::InvalidState));
}

#[test]
fn existing_bags_keep_structure_and_augmentation_does_not_create_extra_units() {
    let templates = BTreeMap::from([(0x3d, 20983)]);
    let items = [
        SalvageInput {
            is_salvage: true,
            structure: 80,
            raw_workmanship: 40,
            num_items: 5,
            ..item(3)
        },
        SalvageInput {
            is_salvage: true,
            structure: 70,
            raw_workmanship: 20,
            num_items: 4,
            ..item(4)
        },
    ];
    let mut req = request(&items, &templates);
    req.skills.augmentations = 4;
    let result = propose_salvage(req).unwrap();
    assert_eq!(result.bags.iter().map(|b| b.units).sum::<u32>(), 150);
    assert_eq!(result.bags.iter().map(|b| b.value).sum::<u32>(), 200);
    assert_eq!(result.results[0].workmanship, 60.0 / 9.0);
}

#[test]
fn gdle_reporting_skill_is_request_wide_and_each_split_bag_has_one_result() {
    let templates = BTreeMap::from([(61, 10)]);
    let items = [
        SalvageInput {
            raw_workmanship: 100,
            ..item(3)
        },
        SalvageInput {
            is_salvage: true,
            structure: 20,
            num_items: 2,
            raw_workmanship: 10,
            ..item(4)
        },
    ];
    let mut input = request(&items, &templates);
    input.skills.salvaging = 1;
    input.skills.armor = 1000;
    input.skills.augmentations = 4;
    let proposal = propose_salvage(input).unwrap();
    assert_eq!(proposal.reporting_skill, 29);
    assert_eq!(proposal.augmentation_bonus, 0);
    assert_eq!(proposal.bags.len(), 2);
    assert_eq!(proposal.results.len(), proposal.bags.len());
    for (bag, result) in proposal.bags.iter().zip(&proposal.results) {
        assert_eq!(result.units, bag.units);
        assert_eq!(result.skill, 29);
        assert_eq!(result.augmentation_bonus, 0);
    }
}
