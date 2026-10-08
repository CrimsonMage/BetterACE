//! Frozen operator-configured rare policy. No emulator rate is retail evidence.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertainEvidence {
    CommunityEstimate,
    HistoricalSourceUnrecovered,
    OperatorAssumption,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareEvidence {
    pub classification: UncertainEvidence,
    pub note: String,
    pub reference: Option<String>,
}
impl RareEvidence {
    pub fn validate(&self) -> Result<(), String> {
        if self.note.trim().is_empty()
            || self.note.len() > 1024
            || self.reference.as_ref().is_some_and(|r| r.len() > 2048)
        {
            return Err("rare assumptions require bounded, nonempty evidence notes".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbabilityV1 {
    pub numerator: u64,
    pub denominator: u64,
}
impl ProbabilityV1 {
    pub fn validate(self) -> Result<(), String> {
        if self.denominator == 0 || self.numerator > self.denominator {
            return Err("invalid probability ratio".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareProbabilityV1 {
    pub value: ProbabilityV1,
    pub evidence: RareEvidence,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RareTimerInitialization {
    FirstEligibleKill,
    FirstLogin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RareTimerReset {
    RealtimeSuccess,
    AnyRareSuccess,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RareIntervalDistribution {
    UniformSeconds,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RealtimeRarePolicyV1 {
    pub minimum_seconds: u64,
    pub maximum_seconds: u64,
    pub distribution: RareIntervalDistribution,
    pub initialization: RareTimerInitialization,
    pub reset: RareTimerReset,
    pub bonus_chance: RareProbabilityV1,
    /// Exact seconds, sampling, initialization and reset are explicit assumptions.
    pub timing_evidence: RareEvidence,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedRareItemV1 {
    pub template: u32,
    pub weight: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareTierV1 {
    pub tier: u8,
    pub weight: u64,
    pub evidence: RareEvidence,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub items: Vec<WeightedRareItemV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RareProfileV1 {
    pub schema_version: u16,
    pub id: u32,
    pub enabled: bool,
    pub standard: Option<RareProbabilityV1>,
    pub realtime: Option<RealtimeRarePolicyV1>,
    #[serde(deserialize_with = "crate::bounded::vec")]
    pub tiers: Vec<RareTierV1>,
}
impl RareProfileV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.id == 0 || self.tiers.len() > 6 {
            return Err("rare schema, identity or tier count".into());
        }
        if let Some(standard) = &self.standard {
            standard.value.validate()?;
            standard.evidence.validate()?;
        }
        if let Some(realtime) = &self.realtime {
            // One year is a resource/overflow bound, not a claimed retail interval.
            if realtime.minimum_seconds == 0
                || realtime.minimum_seconds > realtime.maximum_seconds
                || realtime.maximum_seconds > 366 * 86400
            {
                return Err("rare real-time interval bounds".into());
            }
            realtime.bonus_chance.value.validate()?;
            realtime.bonus_chance.evidence.validate()?;
            realtime.timing_evidence.validate()?;
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut total = 0_u64;
        for tier in &self.tiers {
            if !(1..=6).contains(&tier.tier) || !ids.insert(tier.tier) || tier.items.len() > 4096 {
                return Err("rare tier identity or item count".into());
            }
            tier.evidence.validate()?;
            total = total
                .checked_add(tier.weight)
                .ok_or("rare tier weights overflow")?;
            let mut items = std::collections::BTreeSet::new();
            let mut item_total = 0_u64;
            for item in &tier.items {
                if item.template == 0 || !items.insert(item.template) {
                    return Err("rare item identity".into());
                }
                item_total = item_total
                    .checked_add(item.weight)
                    .ok_or("rare item weights overflow")?;
            }
            if self.enabled && tier.weight != 0 && item_total == 0 {
                return Err("active rare tier has no selectable items".into());
            }
        }
        if self.enabled
            && (self.standard.is_none() || self.realtime.is_none() || ids.len() != 6 || total == 0)
        {
            return Err("rares unconfigured: select explicit occurrence, timing, six-tier and item settings".into());
        }
        Ok(())
    }
}
