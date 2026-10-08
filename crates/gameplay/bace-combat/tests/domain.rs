use bace_combat::{
    AttackImpact, DamageShare, MeleeAttack, RewardError, Strike, kill_rewards, skill_chance,
};

#[test]
fn official_skill_chance_vectors() {
    for line in include_str!("fixtures/skill.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let parts: Vec<_> = line.split(',').collect();
        let actual = skill_chance(parts[0].parse().unwrap(), parts[1].parse().unwrap()).unwrap();
        let expected: f64 = parts[2].parse().unwrap();
        assert!((actual - expected).abs() < 1e-14, "{line}: {actual}");
    }
    assert!(skill_chance(i32::MAX, i32::MIN).is_err());
}
#[test]
fn attack_hooks_do_not_repeat_and_geometry_cancels_pending_strikes() {
    let mut attack = MeleeAttack::new(
        20,
        3.0,
        1.0,
        &[
            Strike {
                offset_seconds: 0.25,
            },
            Strike {
                offset_seconds: 0.75,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        attack.poll(3.1, true, true, true, true).unwrap(),
        AttackImpact::Waiting
    );
    assert_eq!(
        attack.poll(3.25, true, true, true, true).unwrap(),
        AttackImpact::Strike { index: 0 }
    );
    assert_eq!(
        attack.poll(3.25, true, true, true, true).unwrap(),
        AttackImpact::Waiting
    );
    assert_eq!(
        attack.poll(3.75, true, true, true, false).unwrap(),
        AttackImpact::Cancelled
    );
    assert_eq!(
        attack.poll(4.0, true, true, true, true).unwrap(),
        AttackImpact::Cancelled
    );
}
#[test]
fn death_rewards_include_ineligible_damage_and_bankers_rounding() {
    let shares = [
        DamageShare {
            actor: 2,
            damage: 1.0,
            eligible: true,
        },
        DamageShare {
            actor: 3,
            damage: 3.0,
            eligible: true,
        },
    ];
    let mut out = Vec::with_capacity(4);
    kill_rewards(&shares, Some(10), &mut out).unwrap();
    assert_eq!(out, [(2, 2), (3, 8)]);
    let mut shares = shares;
    shares[1].eligible = false;
    out.clear();
    kill_rewards(&shares, Some(10), &mut out).unwrap();
    assert_eq!(out, [(2, 2)]);
    shares[1].damage = f32::NAN;
    assert_eq!(
        kill_rewards(&shares, Some(10), &mut out),
        Err(RewardError::InvalidDamage)
    );
    assert_eq!(out, [(2, 2)]);
}
#[test]
fn rewards_retain_output_on_pressure() {
    let mut out = Vec::new();
    assert_eq!(
        kill_rewards(
            &[DamageShare {
                actor: 1,
                damage: 1.0,
                eligible: true
            }],
            Some(10),
            &mut out
        ),
        Err(RewardError::OutputCapacity)
    );
    assert!(out.is_empty());
}
#[test]
fn official_death_reward_vectors() {
    for line in include_str!("fixtures/rewards.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let shares: Vec<_> = p[1]
            .split(';')
            .enumerate()
            .map(|(index, damage)| DamageShare {
                actor: index as u32,
                damage: damage.parse().unwrap(),
                eligible: p[2].as_bytes()[index] == b'1',
            })
            .collect();
        let mut output = Vec::with_capacity(shares.len());
        kill_rewards(&shares, Some(p[0].parse().unwrap()), &mut output).unwrap();
        let expected: Vec<_> = p[3]
            .split(';')
            .enumerate()
            .filter(|(_, xp)| *xp != "-")
            .map(|(index, xp)| (index as u32, xp.parse::<i64>().unwrap()))
            .collect();
        assert_eq!(output, expected, "{line}");
    }
}
