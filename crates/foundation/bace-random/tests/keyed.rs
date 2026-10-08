use bace_random::{Domain, RandomRoot};

#[test]
fn independent_python_hmac_vectors_freeze_algorithm_and_framing() {
    let key = std::array::from_fn(|i| i as u8);
    let root = RandomRoot::new(key, 9).unwrap();
    for line in include_str!("fixtures/v1.csv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let v: Vec<u64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        let domain = match v[1] {
            1 => Domain::OrdinaryLoot,
            2 => Domain::RareOccurrence,
            4 => Domain::RareTier,
            6 => Domain::RareTimer,
            7 => Domain::Magic,
            11 => Domain::Crafting,
            _ => panic!("fixture domain"),
        };
        let mut stream = if v[0] == 2 {
            root.crafting_stream([17; 16], [34; 16]).unwrap()
        } else if v[0] == 0 {
            root.event_stream([34; 16], domain).unwrap()
        } else {
            root.rare_stream([17; 16], v[2], domain).unwrap()
        };
        for expected in &v[3..6] {
            assert_eq!(stream.next_u64().unwrap(), *expected);
        }
        assert_eq!(
            stream.fork(b"weapons", 4).unwrap().next_u64().unwrap(),
            v[6]
        );
    }
}

#[test]
fn crafting_scope_binds_both_character_and_operation_without_cross_talk() {
    let root = RandomRoot::new([5; 32], 1).unwrap();
    let expected = root
        .crafting_stream([1; 16], [2; 16])
        .unwrap()
        .next_u64()
        .unwrap();
    let other_character = root
        .crafting_stream([3; 16], [2; 16])
        .unwrap()
        .next_u64()
        .unwrap();
    let other_operation = root
        .crafting_stream([1; 16], [3; 16])
        .unwrap()
        .next_u64()
        .unwrap();
    assert_ne!(expected, other_character);
    assert_ne!(expected, other_operation);
    assert_eq!(
        expected,
        root.crafting_stream([1; 16], [2; 16])
            .unwrap()
            .next_u64()
            .unwrap()
    );
    assert!(root.crafting_stream([0; 16], [2; 16]).is_err());
    assert!(root.crafting_stream([1; 16], [0; 16]).is_err());
}

#[test]
fn unrelated_players_loot_and_sibling_nodes_cannot_advance_personal_rares() {
    let root = RandomRoot::new([9; 32], 1).unwrap();
    let mut expected = root
        .rare_stream([1; 16], 24, Domain::RareOccurrence)
        .unwrap();
    let mut actual = root
        .rare_stream([1; 16], 24, Domain::RareOccurrence)
        .unwrap();
    for i in 1..=1000 {
        let mut other = root
            .rare_stream([2; 16], i, Domain::RareOccurrence)
            .unwrap();
        other.chance(1, 2500).unwrap();
        let mut loot = root
            .event_stream((i as u128).to_le_bytes(), Domain::OrdinaryLoot)
            .unwrap();
        for _ in 0..10 {
            loot.below(177).unwrap();
        }
        assert_eq!(actual.next_u64().unwrap(), expected.next_u64().unwrap());
    }
    let parent = root.event_stream([3; 16], Domain::OrdinaryLoot).unwrap();
    let mut first = parent.fork(b"first", 0).unwrap();
    let mut second = parent.fork(b"second", 0).unwrap();
    let expected = second.next_u64().unwrap();
    for _ in 0..100 {
        first.next_u64().unwrap();
    }
    assert_eq!(
        parent.fork(b"second", 0).unwrap().next_u64().unwrap(),
        expected
    );
    assert!(root.event_stream([1; 16], Domain::RareTier).is_err());
    assert!(root.rare_stream([1; 16], 0, Domain::OrdinaryLoot).is_err());
}

#[test]
fn invalid_bounds_and_probability_endpoints_do_not_consume_draws() {
    let root = RandomRoot::new([3; 32], 1).unwrap();
    let mut stream = root.event_stream([1; 16], Domain::Combat).unwrap();
    let mut reference = stream.clone();
    assert!(stream.below(0).is_err());
    assert!(stream.chance(2, 1).is_err());
    assert!(stream.chance(1, 0).is_err());
    assert!(!stream.chance(0, 100).unwrap());
    assert!(stream.chance(100, 100).unwrap());
    assert_eq!(stream.next_u64().unwrap(), reference.next_u64().unwrap());
    for upper in [1, 2, 3, 257, u64::MAX / 2 + 1, u64::MAX] {
        for _ in 0..100 {
            assert!(stream.below(upper).unwrap() < upper);
        }
    }
}
