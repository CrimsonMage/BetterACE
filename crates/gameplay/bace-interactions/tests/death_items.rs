use bace_interactions::{DeathItemError, DeathPossession, select_death_items};
use bace_types::EntityId;
#[test]
fn death_item_order_matches_unchanged_compiled_ace_for_categories_stack_values_and_variance() {
    for row in include_str!("fixtures/death_items.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let (input, expected) = row.split_once(',').unwrap();
        let items: Vec<_> = input
            .split(';')
            .map(|row| {
                let v: Vec<i32> = row.split(':').map(|n| n.parse().unwrap()).collect();
                DeathPossession {
                    id: EntityId(v[0] as u32),
                    template: 100,
                    item_type: v[1] as u32,
                    value: v[2],
                    stack: v[3] as u32,
                    wielded: false,
                    bonded: 0,
                }
            })
            .collect();
        let mut n = 0;
        let plan = select_death_items(&items, 275, 14, 0, false, || {
            let variance = [-0.1, 0.0, 0.1][n % 3];
            n += 1;
            Ok(variance)
        })
        .unwrap();
        let expected: Vec<_> = expected
            .split(';')
            .map(|value| value.split_once(':').unwrap().0.parse::<u32>().unwrap())
            .collect();
        assert_eq!(
            plan.drops.iter().map(|d| d.item.0).collect::<Vec<_>>(),
            expected,
            "{row}"
        );
        for drop in plan.drops {
            assert_eq!(drop.amount, 1);
            assert_eq!(
                drop.fresh_template,
                items.iter().find(|p| p.id == drop.item).unwrap().stack > 1
            );
        }
    }
}
#[test]
fn no_drop_exempts_everything_while_slippery_and_destroy_ignore_level_and_item_allowance() {
    let base = DeathPossession {
        id: EntityId(1),
        template: 100,
        item_type: 1,
        value: 100,
        stack: 5,
        wielded: false,
        bonded: 0,
    };
    let items = [
        base,
        DeathPossession {
            id: EntityId(2),
            bonded: -1,
            wielded: true,
            ..base
        },
        DeathPossession {
            id: EntityId(3),
            bonded: -2,
            ..base
        },
        DeathPossession {
            id: EntityId(4),
            bonded: 1,
            ..base
        },
        DeathPossession {
            id: EntityId(5),
            template: 273,
            ..base
        },
    ];
    let suppressed = select_death_items(&items, 275, 14, 999, true, || {
        panic!("suppressed death draws")
    })
    .unwrap();
    assert!(suppressed.drops.is_empty() && suppressed.destroyed.is_empty());
    assert_eq!(suppressed.coins, 0);
    let plan = select_death_items(&items, 5, 0, 999, false, || Ok(0.0)).unwrap();
    assert_eq!(plan.drops.len(), 1);
    assert_eq!(plan.drops[0].item, EntityId(2));
    assert_eq!(plan.drops[0].amount, 5);
    assert!(!plan.drops[0].fresh_template);
    assert_eq!(plan.destroyed, vec![EntityId(3)]);
    assert_eq!(plan.coins, 0);
    let plan = select_death_items(&items, 34, 1, 999, false, || Ok(0.0)).unwrap();
    assert_eq!(plan.coins, 499);
    assert_eq!(plan.drops[0].item, EntityId(1));
    assert_eq!(plan.drops[0].amount, 1);
    assert!(plan.drops[0].fresh_template);
    assert_eq!(items[0].stack, 5);
    assert_eq!(
        select_death_items(&items, 100, 1, 0, false, || Ok(f32::NAN)),
        Err(DeathItemError::Random)
    );
}
