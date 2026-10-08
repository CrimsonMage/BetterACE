use bace_content::*;
use bace_gameplay_api::{CharacterRareState, RareKillContext};
pub fn evidence() -> RareEvidence {
    RareEvidence {
        classification: UncertainEvidence::OperatorAssumption,
        note: "SYNTHETIC TEST PROFILE, not retail values".into(),
        reference: None,
    }
}
pub fn probability(n: u64, d: u64) -> RareProbabilityV1 {
    RareProbabilityV1 {
        value: ProbabilityV1 {
            numerator: n,
            denominator: d,
        },
        evidence: evidence(),
    }
}
pub fn profile(n: u64, d: u64, bonus: u64) -> RareProfileV1 {
    RareProfileV1 {
        schema_version: 1,
        id: 1,
        enabled: true,
        standard: Some(probability(n, d)),
        realtime: Some(RealtimeRarePolicyV1 {
            minimum_seconds: 10,
            maximum_seconds: 10,
            distribution: RareIntervalDistribution::UniformSeconds,
            initialization: RareTimerInitialization::FirstEligibleKill,
            reset: RareTimerReset::RealtimeSuccess,
            bonus_chance: probability(bonus, 1),
            timing_evidence: evidence(),
        }),
        tiers: (1..=6)
            .map(|tier| RareTierV1 {
                tier,
                weight: 1,
                evidence: evidence(),
                items: vec![WeightedRareItemV1 {
                    template: 100 + u32::from(tier),
                    weight: 1,
                }],
            })
            .collect(),
    }
}
pub fn state(character: u32) -> CharacterRareState {
    CharacterRareState {
        character,
        random_identity: ((character as u128) + 1).to_le_bytes(),
        key_version: 1,
        attempt_ordinal: 0,
        timer_ordinal: 0,
        next_realtime_at: None,
        last_effective_time: 0,
    }
}
pub fn kill(character: u32, now: u64) -> RareKillContext {
    RareKillContext {
        character,
        player_level: 200,
        creature_level: 100,
        lootable_monster_death: true,
        now_unix_seconds: now,
    }
}
