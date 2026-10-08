//! GDLE 353cbab52ef7da2b7063bc3e3f008461d8531693 AllegianceManager.cpp
//! HandleAllegiancePassup and checkpoint timing; WeenieObject::TryToUnloadAllegianceXP.
use crate::{AllegianceNode, AllegiancePatch, AllegianceRegistry};
use bace_gameplay_api::social::SocialError;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllegianceCredit {
    pub actor: EntityId,
    pub amount: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AllegianceXpProposal {
    pub patch: AllegiancePatch,
    pub credits: Vec<AllegianceCredit>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassupInputs {
    pub direct: bool,
    pub loyalty: u32,
    pub leadership: u32,
    pub real_days: f64,
    pub game_hours: f64,
    pub average_real_days: f64,
    pub average_game_hours: f64,
    pub vassals: usize,
}
/// Original double arithmetic and truncation; malformed time/overflow is refused.
pub fn passup_amount(amount: u64, input: PassupInputs) -> Result<u64, SocialError> {
    if amount > i64::MAX as u64
        || input.vassals == 0
        || input.vassals > 11
        || [
            input.real_days,
            input.game_hours,
            input.average_real_days,
            input.average_game_hours,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(SocialError::Invalid);
    }
    let vassal_factor = (0.25 * input.vassals as f64).clamp(0.0, 1.0);
    let real = input.real_days.min(730.0) / 730.0;
    let game = input.game_hours.min(720.0) / 720.0;
    let avg_real = input.average_real_days.min(730.0) / 730.0;
    let avg_game = input.average_game_hours.min(720.0) / 720.0;
    let loyalty = f64::from(input.loyalty.min(291)) / 291.0;
    let leadership = f64::from(input.leadership.min(291)) / 291.0;
    let (a, b) = if input.direct {
        (50.0, 22.5)
    } else {
        (16.0, 8.0)
    };
    let generated = 0.01 * (a + b * loyalty * (1.0 + real * game));
    let received = 0.01 * (a + b * leadership * (1.0 + vassal_factor * avg_real * avg_game));
    let passup = generated * received;
    Ok((amount as f64 * passup) as u64)
}
impl AllegianceRegistry {
    pub fn propose_passup(
        &self,
        source: EntityId,
        amount: u64,
        now: u64,
        online: &BTreeSet<EntityId>,
        projected_level: impl FnMut(EntityId, u64) -> Result<u32, SocialError>,
    ) -> Result<AllegianceXpProposal, SocialError> {
        self.propose_passup_batch(&[(source, amount, None)], now, online, projected_level)
    }
    pub fn propose_passup_batch(
        &self,
        sources: &[(EntityId, u64, Option<u32>)],
        now: u64,
        online: &BTreeSet<EntityId>,
        mut projected_level: impl FnMut(EntityId, u64) -> Result<u32, SocialError>,
    ) -> Result<AllegianceXpProposal, SocialError> {
        self.passup_batch(sources, now, online, false, |actor, amount, _| {
            projected_level(actor, amount)
        })
    }
    /// Applies each original reward immediately before its pass-up walk. This preserves
    /// cached-level eligibility when fellowship recipients are also patrons.
    pub fn propose_passup_awards(
        &self,
        sources: &[(EntityId, u64)],
        now: u64,
        online: &BTreeSet<EntityId>,
        credit: impl FnMut(EntityId, u64, bool) -> Result<u32, SocialError>,
    ) -> Result<AllegianceXpProposal, SocialError> {
        let sources: Vec<_> = sources
            .iter()
            .map(|(actor, amount)| (*actor, *amount, None))
            .collect();
        self.passup_batch(&sources, now, online, true, credit)
    }
    fn passup_batch(
        &self,
        sources: &[(EntityId, u64, Option<u32>)],
        now: u64,
        online: &BTreeSet<EntityId>,
        award_sources: bool,
        mut projected_level: impl FnMut(EntityId, u64, bool) -> Result<u32, SocialError>,
    ) -> Result<AllegianceXpProposal, SocialError> {
        if sources.len() > 1024
            || sources
                .iter()
                .any(|(_, amount, _)| *amount > i64::MAX as u64)
        {
            return Err(SocialError::Invalid);
        }
        let mut patch = self.patch()?;
        let mut staged = BTreeMap::<EntityId, AllegianceNode>::new();
        let mut credits = Vec::new();
        for &(initial_source, initial_amount, mut level) in sources {
            if award_sources {
                level = Some(projected_level(initial_source, initial_amount, true)?);
            }
            if let Some(level) = level
                && let Some(before) = staged
                    .get(&initial_source)
                    .or_else(|| self.node(initial_source))
            {
                let mut node = before.clone();
                node.level = level;
                staged.insert(initial_source, node);
            }
            let mut seen = BTreeSet::new();
            let mut source = initial_source;
            let mut amount = initial_amount;
            let mut direct = true;
            while let Some(mut node) = staged.get(&source).or_else(|| self.node(source)).cloned() {
                if !seen.insert(source) {
                    return Err(SocialError::Invalid);
                }
                if seen.len() > self.capacity {
                    return Err(SocialError::Capacity);
                }
                let Some(patron_id) = node.patron else {
                    break;
                };
                if !online.contains(&source) {
                    break;
                }
                let mut patron = staged
                    .get(&patron_id)
                    .or_else(|| self.node(patron_id))
                    .cloned()
                    .ok_or(SocialError::Invalid)?;
                if !node.may_pass_up {
                    if node.sworn_at < 1_534_765_700 || patron.level >= node.level {
                        node.may_pass_up = true;
                    } else {
                        break;
                    }
                }
                let mut real = 0.0;
                let mut game = 0.0;
                let mut average_real = 0.0;
                let mut average_game = 0.0;
                for id in &patron.vassals {
                    let vassal = staged
                        .get(id)
                        .or_else(|| self.node(*id))
                        .ok_or(SocialError::Invalid)?;
                    let days = now
                        .checked_sub(vassal.sworn_at)
                        .ok_or(SocialError::Invalid)? as f64
                        / 86400.0;
                    let hours = vassal.online_seconds as f64 / 3600.0;
                    if *id == source {
                        real = days;
                        game = hours;
                    }
                    average_real += days;
                    average_game += hours;
                }
                if !patron.vassals.contains(&source) {
                    return Err(SocialError::Invalid);
                }
                average_real /= patron.vassals.len().max(1) as f64;
                average_game /= patron.vassals.len().max(1) as f64;
                let passed = passup_amount(
                    amount,
                    PassupInputs {
                        direct,
                        loyalty: node.loyalty,
                        leadership: patron.leadership,
                        real_days: real,
                        game_hours: game,
                        average_real_days: average_real,
                        average_game_hours: average_game,
                        vassals: patron.vassals.len(),
                    },
                )?;
                if passed == 0 {
                    staged.insert(source, node);
                    break;
                }
                node.tithed_total = node
                    .tithed_total
                    .checked_add(passed)
                    .ok_or(SocialError::Overflow)?;
                patron.received_total = patron
                    .received_total
                    .checked_add(passed)
                    .ok_or(SocialError::Overflow)?;
                patron.unclaimed = patron
                    .unclaimed
                    .checked_add(passed)
                    .ok_or(SocialError::Overflow)?
                    .min(u64::from(u32::MAX));
                if online.contains(&patron_id) {
                    let credit = patron.unclaimed;
                    patron.level = projected_level(patron_id, credit, false)?;
                    credits.push(AllegianceCredit {
                        actor: patron_id,
                        amount: credit,
                    });
                    patron.unclaimed = 0;
                }
                staged.insert(source, node);
                staged.insert(patron_id, patron);
                source = patron_id;
                amount = passed;
                direct = false;
            }
        }
        patch.nodes = staged
            .into_iter()
            .filter_map(|(id, after)| {
                let before = self.node(id).cloned();
                (before.as_ref() != Some(&after)).then_some((before, Some(after)))
            })
            .collect();
        Ok(AllegianceXpProposal { patch, credits })
    }
    pub fn propose_redemption(
        &self,
        actor: EntityId,
        projected_level: u32,
    ) -> Result<AllegianceXpProposal, SocialError> {
        let before = self.node(actor).ok_or(SocialError::Missing)?;
        let mut after = before.clone();
        let amount = after.unclaimed;
        after.unclaimed = 0;
        after.level = projected_level;
        let mut patch = self.patch()?;
        patch.nodes.push((Some(before.clone()), Some(after)));
        Ok(AllegianceXpProposal {
            patch,
            credits: if amount == 0 {
                vec![]
            } else {
                vec![AllegianceCredit { actor, amount }]
            },
        })
    }
    pub fn propose_online_checkpoint(
        &self,
        elapsed_seconds: u64,
        online: &BTreeSet<EntityId>,
    ) -> Result<AllegiancePatch, SocialError> {
        let mut patch = self.patch()?;
        for actor in online {
            if let Some(before) = self.node(*actor).filter(|n| n.patron.is_some()) {
                let mut after = before.clone();
                after.online_seconds = after
                    .online_seconds
                    .checked_add(elapsed_seconds)
                    .ok_or(SocialError::Overflow)?;
                patch.nodes.push((Some(before.clone()), Some(after)));
            }
        }
        Ok(patch)
    }
    pub fn propose_cached_skills(
        &self,
        actor: EntityId,
        level: u32,
        leadership: u32,
        loyalty: u32,
    ) -> Result<AllegiancePatch, SocialError> {
        let before = self.node(actor).ok_or(SocialError::Missing)?;
        let mut after = before.clone();
        after.level = level;
        after.leadership = leadership;
        after.loyalty = loyalty;
        let mut patch = self.patch()?;
        patch.nodes.push((Some(before.clone()), Some(after)));
        Ok(patch)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct AllegianceClock {
    last_checkpoint: f64,
}
impl AllegianceClock {
    pub fn new(now: f64) -> Result<Self, SocialError> {
        if !now.is_finite() || now < 0.0 {
            return Err(SocialError::Invalid);
        }
        Ok(Self {
            last_checkpoint: now,
        })
    }
    pub fn due(&self, now: f64) -> Result<Option<u64>, SocialError> {
        if !now.is_finite() || now < self.last_checkpoint {
            return Err(SocialError::Invalid);
        }
        let elapsed = now - self.last_checkpoint;
        if elapsed < 300.0 {
            return Ok(None);
        }
        if elapsed.round() >= u64::MAX as f64 {
            return Err(SocialError::Overflow);
        }
        Ok(Some(elapsed.round() as u64))
    }
    pub fn adopt(&mut self, now: f64) -> Result<(), SocialError> {
        self.due(now)?.ok_or(SocialError::Invalid)?;
        self.last_checkpoint = now;
        Ok(())
    }
}
