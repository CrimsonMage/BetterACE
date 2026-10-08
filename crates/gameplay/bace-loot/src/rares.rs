//! Retail-evidence structure, with explicitly uncertain operator probabilities.
//! A decision is a proposal: the owner must commit both failures and successes
//! with the death operation before adopting its next state or reporting an award.
use bace_content::{RareProfileV1, RareTimerInitialization, RareTimerReset};
use bace_gameplay_api::{CharacterRareState, RareAward, RareDecision, RareKillContext};
use bace_random::{Domain, RandomError, RandomRoot};

#[derive(Debug, PartialEq, Eq)]
pub enum RareError {
    InvalidProfile(String),
    Unconfigured,
    State,
    Overflow,
    Random(RandomError),
}
impl From<RandomError> for RareError {
    fn from(error: RandomError) -> Self {
        Self::Random(error)
    }
}

struct Tier {
    id: u8,
    end: u64,
    items: Vec<(u64, u32)>,
    total: u64,
}
pub struct RareEvaluator {
    profile: RareProfileV1,
    tiers: Vec<Tier>,
    total: u64,
}
impl RareEvaluator {
    pub fn prepare(mut profile: RareProfileV1) -> Result<Self, RareError> {
        profile.validate().map_err(RareError::InvalidProfile)?;
        if !profile.enabled {
            return Err(RareError::Unconfigured);
        }
        profile.tiers.sort_by_key(|tier| tier.tier);
        let mut tiers = Vec::with_capacity(6);
        let mut total = 0;
        for tier in &mut profile.tiers {
            tier.items.sort_by_key(|item| item.template);
            total += tier.weight;
            let mut sum = 0;
            let items = tier
                .items
                .iter()
                .map(|item| {
                    sum += item.weight;
                    (sum, item.template)
                })
                .collect();
            tiers.push(Tier {
                id: tier.tier,
                end: total,
                items,
                total: sum,
            });
        }
        Ok(Self {
            profile,
            tiers,
            total,
        })
    }
    pub fn profile(&self) -> &RareProfileV1 {
        &self.profile
    }

    pub fn initialize_on_login(
        &self,
        root: &RandomRoot,
        state: CharacterRareState,
        now: u64,
    ) -> Result<CharacterRareState, RareError> {
        check_state(root, state)?;
        if now > i64::MAX as u64 {
            return Err(RareError::State);
        }
        if self
            .profile
            .realtime
            .as_ref()
            .ok_or(RareError::Unconfigured)?
            .initialization
            != RareTimerInitialization::FirstLogin
            || state.next_realtime_at.is_some()
        {
            return Ok(state);
        }
        let mut next = state;
        next.last_effective_time = now.max(state.last_effective_time);
        self.schedule(root, &mut next)?;
        Ok(next)
    }

    pub fn evaluate(
        &self,
        root: &RandomRoot,
        state: CharacterRareState,
        kill: RareKillContext,
    ) -> Result<RareDecision, RareError> {
        check_state(root, state)?;
        if state.character != kill.character
            || kill.player_level == 0
            || kill.now_unix_seconds > i64::MAX as u64
        {
            return Err(RareError::State);
        }
        let mut decision = RareDecision {
            character: kill.character,
            eligible: false,
            previous: state,
            next: state,
            profile_id: self.profile.id,
            standard_success: false,
            realtime_success: false,
            award: None,
        };
        if !kill.lootable_monster_death
            || !(kill.creature_level > kill.player_level || kill.creature_level >= 100)
        {
            return Ok(decision);
        }
        decision.eligible = true;
        let next = &mut decision.next;
        next.last_effective_time = kill.now_unix_seconds.max(state.last_effective_time);
        let realtime = self
            .profile
            .realtime
            .as_ref()
            .ok_or(RareError::Unconfigured)?;
        if next.next_realtime_at.is_none() {
            if realtime.initialization == RareTimerInitialization::FirstLogin {
                return Err(RareError::State);
            }
            self.schedule(root, next)?;
        }
        next.attempt_ordinal = state
            .attempt_ordinal
            .checked_add(1)
            .ok_or(RareError::Overflow)?;
        let chance = self
            .profile
            .standard
            .as_ref()
            .ok_or(RareError::Unconfigured)?
            .value;
        decision.standard_success = root
            .rare_stream(
                state.random_identity,
                state.attempt_ordinal,
                Domain::RareOccurrence,
            )?
            .chance(chance.numerator, chance.denominator)?;
        if next
            .next_realtime_at
            .is_some_and(|due| next.last_effective_time >= due)
        {
            let chance = realtime.bonus_chance.value;
            decision.realtime_success = root
                .rare_stream(
                    state.random_identity,
                    state.attempt_ordinal,
                    Domain::RareRealtime,
                )?
                .chance(chance.numerator, chance.denominator)?;
        }
        if decision.standard_success || decision.realtime_success {
            let tier_draw = root
                .rare_stream(
                    state.random_identity,
                    state.attempt_ordinal,
                    Domain::RareTier,
                )?
                .below(self.total)?;
            let tier = self
                .tiers
                .iter()
                .find(|t| tier_draw < t.end)
                .ok_or(RareError::State)?;
            let item_draw = root
                .rare_stream(
                    state.random_identity,
                    state.attempt_ordinal,
                    Domain::RareItem,
                )?
                .below(tier.total)?;
            let template = tier
                .items
                .iter()
                .find(|(end, _)| item_draw < *end)
                .ok_or(RareError::State)?
                .1;
            decision.award = Some(RareAward {
                tier: tier.id,
                template,
            });
            if realtime.reset == RareTimerReset::AnyRareSuccess || decision.realtime_success {
                self.schedule(root, next)?;
            }
        }
        Ok(decision)
    }

    fn schedule(&self, root: &RandomRoot, state: &mut CharacterRareState) -> Result<(), RareError> {
        let policy = self
            .profile
            .realtime
            .as_ref()
            .ok_or(RareError::Unconfigured)?;
        let width = policy.maximum_seconds - policy.minimum_seconds + 1;
        let delay = root
            .rare_stream(
                state.random_identity,
                state.timer_ordinal,
                Domain::RareTimer,
            )?
            .below(width)?
            + policy.minimum_seconds;
        let ordinal = state
            .timer_ordinal
            .checked_add(1)
            .ok_or(RareError::Overflow)?;
        let deadline = state
            .last_effective_time
            .checked_add(delay)
            .ok_or(RareError::Overflow)?;
        if deadline > i64::MAX as u64 {
            return Err(RareError::Overflow);
        }
        state.next_realtime_at = Some(deadline);
        state.timer_ordinal = ordinal;
        Ok(())
    }
}
fn check_state(root: &RandomRoot, state: CharacterRareState) -> Result<(), RareError> {
    if !(0x5000_0001..=0x5fff_ffff).contains(&state.character)
        || state.random_identity == [0; 16]
        || state.key_version != root.key_version()
        || state.last_effective_time > i64::MAX as u64
        || state.next_realtime_at.is_some_and(|t| t > i64::MAX as u64)
    {
        return Err(RareError::State);
    }
    Ok(())
}
