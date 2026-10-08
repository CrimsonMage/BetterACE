//! Pinned ACE Fellowship.CalculateXPSharing/SplitXp/GetDistanceScalar.
use crate::{Fellowship, FellowshipError as E};
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FellowRewardMember {
    pub actor: EntityId,
    pub level: u32,
    pub xp_to_next_level: u64,
    pub indoor: bool,
    pub landblock: u16,
    pub distance_2d: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FellowshipSharing {
    pub share: bool,
    pub even: bool,
}
impl Fellowship {
    pub fn sharing(
        &self,
        members: &[FellowRewardMember],
        even_share_level: u32,
    ) -> Result<FellowshipSharing, E> {
        self.validate_reward_members(members)?;
        let leader = members
            .iter()
            .find(|m| m.actor == self.leader)
            .ok_or(E::NotLeader)?;
        if members.iter().all(|m| m.level >= even_share_level) {
            return Ok(FellowshipSharing {
                share: self.share_xp,
                even: true,
            });
        }
        let difference = members
            .iter()
            .map(|m| m.level.abs_diff(leader.level))
            .max()
            .unwrap_or(0);
        Ok(FellowshipSharing {
            share: self.share_xp && difference <= 10,
            even: difference <= 5,
        })
    }
    pub fn split_experience(
        &self,
        amount: u64,
        source: EntityId,
        quest: bool,
        quest_bonus: bool,
        even_share_level: u32,
        members: &[FellowRewardMember],
    ) -> Result<Vec<(EntityId, u64)>, E> {
        if amount > i64::MAX as u64 {
            return Err(E::Overflow);
        }
        let sharing = self.sharing(members, even_share_level)?;
        let earner = members
            .iter()
            .find(|m| m.actor == source)
            .ok_or(E::NotMember)?;
        if !sharing.share {
            return Ok(vec![(source, amount)]);
        }
        if quest && !quest_bonus {
            return Ok(members
                .iter()
                .map(|m| (m.actor, amount / members.len() as u64))
                .collect());
        }
        let sum = members
            .iter()
            .try_fold(0u64, |sum, m| sum.checked_add(m.xp_to_next_level))
            .ok_or(E::Overflow)?;
        if !sharing.even && sum == 0 {
            return Err(E::Invalid);
        }
        let percent = [1.0, 0.75, 0.6, 0.55, 0.5, 0.45, 0.4, 0.35, 0.3][members.len() - 1];
        let total = (amount as f64 * percent).round_ties_even();
        members
            .iter()
            .map(|m| {
                let distance = distance_scalar(earner, m, quest)?;
                let amount = if sharing.even {
                    (total * distance).round_ties_even()
                } else {
                    (amount as f64 * (m.xp_to_next_level as f64 / sum as f64) * distance)
                        .round_ties_even()
                };
                if amount >= i64::MAX as f64 {
                    return Err(E::Overflow);
                }
                Ok((m.actor, amount as u64))
            })
            .collect()
    }
    fn validate_reward_members(&self, members: &[FellowRewardMember]) -> Result<(), E> {
        if members.is_empty()
            || members.len() != self.members.len()
            || members
                .iter()
                .any(|m| m.level == 0 || !self.members.contains(&m.actor))
            || members
                .iter()
                .enumerate()
                .any(|(i, m)| members[..i].iter().any(|p| p.actor == m.actor))
        {
            return Err(E::Invalid);
        }
        Ok(())
    }
}
pub fn distance_scalar(
    earner: &FellowRewardMember,
    fellow: &FellowRewardMember,
    quest: bool,
) -> Result<f64, E> {
    if !fellow.distance_2d.is_finite() || fellow.distance_2d < 0.0 {
        return Err(E::Invalid);
    }
    if quest {
        return Ok(1.0);
    }
    if earner.indoor != fellow.indoor || (earner.indoor && earner.landblock != fellow.landblock) {
        return Ok(0.0);
    }
    let distance = fellow.distance_2d;
    if distance >= 1200.0 {
        return Ok(0.0);
    }
    if distance <= 600.0 {
        return Ok(1.0);
    }
    Ok(f64::from((1.0 - (distance - 600.0) / 600.0).max(0.0)))
}
