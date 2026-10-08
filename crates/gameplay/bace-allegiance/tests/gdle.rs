use bace_allegiance::*;
use bace_types::{AccountId, EntityId};
use std::collections::BTreeSet;
fn node(id: u32, parent: Option<u32>, children: Vec<u32>, monarch: u32) -> AllegianceNode {
    AllegianceNode {
        character: EntityId(id),
        account: AccountId(u64::from(id)),
        name: format!("Actor{id}"),
        gender: 1,
        heritage: 1,
        patron: parent.map(EntityId),
        monarch: EntityId(monarch),
        vassals: children.into_iter().map(EntityId).collect(),
        rank: 1,
        followers: 0,
        level: 100,
        leadership: 291,
        loyalty: 291,
        sworn_at: 1_600_000_000,
        online_seconds: 720 * 3600,
        may_pass_up: true,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    }
}
fn tree() -> AllegianceRegistry {
    let mut a = node(1, Some(2), vec![], 3);
    a.rank = 1;
    let mut b = node(2, Some(3), vec![1], 3);
    b.followers = 1;
    let mut c = node(3, None, vec![2], 3);
    c.followers = 2;
    AllegianceRegistry::restore(
        vec![a, b, c],
        vec![AllegianceMetadata::new(EntityId(3), 0x80000001)],
        0,
        100,
    )
    .unwrap()
}
#[test]
fn original_compiled_cpp_passup_vectors() {
    for line in include_str!("fixtures/gdle_passup.csv").lines().skip(2) {
        let c: Vec<_> = line.split(',').collect();
        let n = |i: usize| c[i].parse::<u64>().unwrap();
        let f = |i: usize| c[i].parse::<f64>().unwrap();
        assert_eq!(
            passup_amount(
                n(0),
                PassupInputs {
                    direct: n(1) != 0,
                    loyalty: n(2) as u32,
                    leadership: n(3) as u32,
                    real_days: f(4),
                    game_hours: f(5),
                    average_real_days: f(6),
                    average_game_hours: f(7),
                    vassals: n(8) as usize
                }
            )
            .unwrap(),
            n(9),
            "{line}"
        );
    }
}
#[test]
fn offline_patron_retains_pool_and_halts_chain_without_login_repass() {
    let mut owner = tree();
    let online = BTreeSet::from([EntityId(1), EntityId(3)]);
    let proposal = owner
        .propose_passup(EntityId(1), 10000, 1_700_000_000, &online, |_, _| {
            panic!("offline patron must not redeem")
        })
        .unwrap();
    assert!(proposal.credits.is_empty());
    owner.adopt(proposal.patch).unwrap();
    let pool = owner.node(EntityId(2)).unwrap().unclaimed;
    assert!(pool > 0);
    assert_eq!(owner.node(EntityId(3)).unwrap().received_total, 0);
    let redemption = owner.propose_redemption(EntityId(2), 100).unwrap();
    assert_eq!(
        redemption.credits,
        vec![AllegianceCredit {
            actor: EntityId(2),
            amount: pool
        }]
    );
    owner.adopt(redemption.patch).unwrap();
    assert_eq!(owner.node(EntityId(2)).unwrap().unclaimed, 0);
    assert_eq!(owner.node(EntityId(2)).unwrap().received_total, pool);
    assert_eq!(owner.node(EntityId(3)).unwrap().received_total, 0);
}
#[test]
fn cached_received_counter_survives_online_redemption_and_pool_caps() {
    let mut owner = tree();
    let mut patch = owner.patch().unwrap();
    let before = owner.node(EntityId(2)).unwrap().clone();
    let mut after = before.clone();
    after.unclaimed = u64::from(u32::MAX) - 1;
    patch.nodes.push((Some(before), Some(after)));
    owner.adopt(patch).unwrap();
    let online = BTreeSet::from([EntityId(1), EntityId(2)]);
    let p = owner
        .propose_passup(EntityId(1), 10000, 1_700_000_000, &online, |id, amount| {
            assert_eq!(id, EntityId(2));
            assert_eq!(amount, u64::from(u32::MAX));
            Ok(100)
        })
        .unwrap();
    owner.adopt(p.patch).unwrap();
    assert_eq!(owner.node(EntityId(2)).unwrap().unclaimed, 0);
    assert!(owner.node(EntityId(2)).unwrap().received_total > 0);
    assert!(owner.node(EntityId(3)).unwrap().unclaimed > 0);
}
#[test]
fn checkpoint_samples_online_at_boundary_without_logout_overlap_accounting() {
    let mut clock = AllegianceClock::new(10.0).unwrap();
    assert_eq!(clock.due(309.999).unwrap(), None);
    assert_eq!(clock.due(310.6).unwrap(), Some(301));
    let mut owner = tree();
    let patch = owner
        .propose_online_checkpoint(301, &BTreeSet::from([EntityId(2), EntityId(3)]))
        .unwrap();
    owner.adopt(patch).unwrap();
    assert_eq!(owner.node(EntityId(1)).unwrap().online_seconds, 720 * 3600);
    assert_eq!(
        owner.node(EntityId(2)).unwrap().online_seconds,
        720 * 3600 + 301
    );
    assert_eq!(owner.node(EntityId(3)).unwrap().online_seconds, 720 * 3600);
    clock.adopt(310.6).unwrap();
    assert_eq!(clock.due(311.0).unwrap(), None);
}
#[test]
fn ineligible_oath_uses_cached_levels_then_becomes_eligible() {
    let mut owner = tree();
    let mut patch = owner.patch().unwrap();
    let before = owner.node(EntityId(1)).unwrap().clone();
    let mut after = before.clone();
    after.may_pass_up = false;
    after.level = 101;
    patch.nodes.push((Some(before), Some(after)));
    owner.adopt(patch).unwrap();
    let online = BTreeSet::from([EntityId(1)]);
    assert!(
        owner
            .propose_passup(
                EntityId(1),
                10000,
                1_700_000_000,
                &online,
                |_, _| unreachable!()
            )
            .unwrap()
            .patch
            .nodes
            .is_empty()
    );
    let patch = owner
        .propose_cached_skills(EntityId(2), 101, 291, 291)
        .unwrap();
    owner.adopt(patch).unwrap();
    let p = owner
        .propose_passup(
            EntityId(1),
            10000,
            1_700_000_000,
            &online,
            |_, _| unreachable!(),
        )
        .unwrap();
    owner.adopt(p.patch).unwrap();
    assert!(owner.node(EntityId(1)).unwrap().may_pass_up);
}
