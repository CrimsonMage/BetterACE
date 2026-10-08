use bace_fellowship::{FellowRewardMember, Fellowship};
use bace_types::EntityId;
#[test]
fn compiled_original_ace_reward_vectors() {
    for line in include_str!("fixtures/ace_rewards.csv").lines().skip(1) {
        let (input, expected) = line.split_once(';').unwrap();
        let c: Vec<_> = input.split(',').collect();
        let n = |i: usize| c[i].parse::<u64>().unwrap();
        let count = n(2) as usize;
        let mut group = Fellowship::new(1, "Oracle".into(), EntityId(1), true).unwrap();
        for id in 2..=count as u32 {
            group.join(EntityId(id)).unwrap();
        }
        let members: Vec<_> = (0..count)
            .map(|i| {
                let j = 3 + i * 5;
                FellowRewardMember {
                    actor: EntityId(i as u32 + 1),
                    level: n(j) as u32,
                    xp_to_next_level: n(j + 1),
                    distance_2d: c[j + 2].parse().unwrap(),
                    indoor: n(j + 3) != 0,
                    landblock: n(j + 4) as u16,
                }
            })
            .collect();
        let e: Vec<u64> = expected.split(',').map(|v| v.parse().unwrap()).collect();
        let sharing = group.sharing(&members, 50).unwrap();
        assert_eq!(
            (sharing.share, sharing.even),
            (e[0] != 0, e[1] != 0),
            "{line}"
        );
        let split = group
            .split_experience(n(0), EntityId(1), n(1) != 0, false, 50, &members)
            .unwrap();
        for member in &members {
            assert_eq!(
                split
                    .iter()
                    .find(|(id, _)| *id == member.actor)
                    .map_or(0, |(_, xp)| *xp),
                e[member.actor.0 as usize + 1],
                "{line}"
            );
        }
    }
}
