mod common;
use bace_loot::RareEvaluator;
use bace_random::{Domain, RandomRoot};
use common::*;

#[test]
fn eligibility_precedes_all_rolls_and_normal_loot_is_not_an_input() {
    let root = RandomRoot::new([7; 32], 1).unwrap();
    let evaluator = RareEvaluator::prepare(profile(1, 1, 1)).unwrap();
    let initial = state(0x50000001);
    for (player, creature, eligible) in [
        (99, 99, false),
        (100, 99, false),
        (98, 99, true),
        (999, 100, true),
    ] {
        let mut event = kill(initial.character, 100);
        event.player_level = player;
        event.creature_level = creature;
        let decision = evaluator.evaluate(&root, initial, event).unwrap();
        assert_eq!(decision.eligible, eligible);
        assert_eq!(decision.award.is_some(), eligible);
        if !eligible {
            assert_eq!(decision.next, initial);
        }
    }
    let mut event = kill(initial.character, 100);
    event.lootable_monster_death = false;
    assert_eq!(
        evaluator.evaluate(&root, initial, event).unwrap().next,
        initial
    );
    assert!(
        evaluator
            .evaluate(&root, initial, kill(0x50000002, 100))
            .is_err()
    );
}

#[test]
fn due_bonus_is_separate_normal_rares_have_no_cooldown_and_at_most_one_award() {
    let root = RandomRoot::new([7; 32], 1).unwrap();
    let evaluator = RareEvaluator::prepare(profile(1, 1, 1)).unwrap();
    let first = evaluator
        .evaluate(&root, state(0x50000001), kill(0x50000001, 100))
        .unwrap();
    assert!(first.standard_success);
    assert!(!first.realtime_success);
    assert_eq!(first.next.next_realtime_at, Some(110));
    let second = evaluator
        .evaluate(&root, first.next, kill(0x50000001, 101))
        .unwrap();
    assert!(second.award.is_some());
    assert_eq!(second.next.next_realtime_at, Some(110));
    let due = evaluator
        .evaluate(&root, second.next, kill(0x50000001, 1000))
        .unwrap();
    assert!(due.standard_success && due.realtime_success);
    assert!(due.award.is_some());
    assert_eq!(due.next.next_realtime_at, Some(1010));
    // Same frozen input replays exactly. Evaluation never mutates stored state.
    assert_eq!(
        due,
        evaluator
            .evaluate(&root, second.next, kill(0x50000001, 1000))
            .unwrap()
    );
    let failure = RareEvaluator::prepare(profile(0, 1, 0)).unwrap();
    let missed = failure
        .evaluate(&root, second.next, kill(0x50000001, 1000))
        .unwrap();
    assert!(missed.award.is_none());
    assert_eq!(missed.next.attempt_ordinal, 3);
    assert_eq!(missed.next.next_realtime_at, Some(110));
}

#[test]
fn other_characters_and_normal_loot_cannot_change_rare_decisions() {
    let root = RandomRoot::new([12; 32], 1).unwrap();
    let evaluator = RareEvaluator::prepare(profile(1, 5, 0)).unwrap();
    let mut a = state(0x50000001);
    let mut reference = a;
    let mut b = state(0x50000002);
    for now in 1..=200 {
        for _ in 0..7 {
            b = evaluator
                .evaluate(&root, b, kill(b.character, now))
                .unwrap()
                .next;
        }
        let mut loot = root
            .event_stream((now as u128).to_le_bytes(), Domain::OrdinaryLoot)
            .unwrap();
        for _ in 0..20 {
            loot.below(999).unwrap();
        }
        let expected = evaluator
            .evaluate(&root, reference, kill(reference.character, now))
            .unwrap();
        let actual = evaluator
            .evaluate(&root, a, kill(a.character, now))
            .unwrap();
        assert_eq!(actual, expected);
        a = actual.next;
        reference = expected.next;
    }
}

#[test]
fn tier_rebalancing_does_not_change_occurrence_and_missing_values_never_default() {
    let root = RandomRoot::new([1; 32], 1).unwrap();
    let original = profile(1, 5, 0);
    let mut changed = original.clone();
    for tier in &mut changed.tiers {
        tier.weight = u64::from(tier.tier == 6);
    }
    let a = RareEvaluator::prepare(original.clone()).unwrap();
    let b = RareEvaluator::prepare(changed).unwrap();
    let mut state = state(0x50000001);
    for now in 1..=100 {
        let x = a
            .evaluate(&root, state, kill(state.character, now))
            .unwrap();
        let y = b
            .evaluate(&root, state, kill(state.character, now))
            .unwrap();
        assert_eq!(x.standard_success, y.standard_success);
        assert_eq!(x.realtime_success, y.realtime_success);
        assert_eq!(x.next, y.next);
        if let Some(award) = y.award {
            assert_eq!(award.tier, 6);
        }
        state = x.next;
    }
    let mut missing = original;
    missing.standard = None;
    assert!(RareEvaluator::prepare(missing.clone()).is_err());
    missing.enabled = false;
    missing.validate().unwrap();
    assert!(RareEvaluator::prepare(missing).is_err());
}
