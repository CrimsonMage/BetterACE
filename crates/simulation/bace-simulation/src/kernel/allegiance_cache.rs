//! Pinned GDLE WeenieObject.cpp GiveSkillXP/GiveSkillPoints update the cached
//! allegiance inputs only for Leadership/Loyalty and a positive award. Neither
//! login, buffs, attribute raises nor GiveSkillAdvancementClass call this path.
use super::Kernel;
use crate::allegiances::CachedAllegianceSkills;
use bace_gameplay_api::social::SocialError;
use bace_types::EntityId;
impl Kernel {
    pub fn can_note_allegiance_skill_award(
        &self,
        actor: EntityId,
        skill: u32,
        amount: u32,
    ) -> Result<(), SocialError> {
        if amount == 0
            || !matches!(skill, 35 | 36)
            || self.allegiances.registry.node(actor).is_none()
        {
            return Ok(());
        }
        if self.allegiances.cached_skills.len() >= self.allegiances.capacity
            && !self.allegiances.cached_skills.contains_key(&actor)
        {
            return Err(SocialError::Capacity);
        }
        let profile = self.combat.skills.get(&actor).ok_or(SocialError::Missing)?;
        if !profile.values.contains_key(&35)
            || !profile.values.contains_key(&36)
            || self.characters.native_services(actor).is_none()
        {
            return Err(SocialError::Missing);
        }
        Ok(())
    }
    /// Invoke after accepted GiveSkillXP/Points state and derived skill refresh.
    /// Valuable NPC rewards must invoke this after their exact durable receipt.
    pub fn note_allegiance_skill_award(
        &mut self,
        actor: EntityId,
        skill: u32,
        amount: u32,
    ) -> Result<(), SocialError> {
        self.can_note_allegiance_skill_award(actor, skill, amount)?;
        if amount == 0
            || !matches!(skill, 35 | 36)
            || self.allegiances.registry.node(actor).is_none()
        {
            return Ok(());
        }
        let profile = self.combat.skills.get(&actor).ok_or(SocialError::Missing)?;
        let level = self
            .characters
            .native_services(actor)
            .ok_or(SocialError::Missing)?
            .level;
        let value = CachedAllegianceSkills {
            level,
            leadership: profile.values[&35].current,
            loyalty: profile.values[&36].current,
        };
        self.allegiances.cached_skills.insert(actor, value);
        Ok(())
    }
    pub(in crate::kernel) fn step_allegiance_cached_skills(&mut self) -> Result<(), SocialError> {
        if self.allegiances.pending.is_some() || self.allegiances.cached_skills.is_empty() {
            return Ok(());
        }
        let mut patch = self.allegiances.registry.patch()?;
        // Process at most one persistence batch. Frozen values are not recomputed
        // later when enchantments/attributes may have changed without a source trigger.
        let selected: Vec<_> = self
            .allegiances
            .cached_skills
            .iter()
            .take(256)
            .map(|(&id, &v)| (id, v))
            .collect();
        for (actor, value) in selected {
            let Some(before) = self.allegiances.registry.node(actor) else {
                self.allegiances.cached_skills.remove(&actor);
                continue;
            };
            if before.level == value.level
                && before.leadership == value.leadership
                && before.loyalty == value.loyalty
            {
                self.allegiances.cached_skills.remove(&actor);
                continue;
            }
            let mut after = before.clone();
            after.level = value.level;
            after.leadership = value.leadership;
            after.loyalty = value.loyalty;
            patch.nodes.push((Some(before.clone()), Some(after)));
        }
        if let Some(actor) = patch
            .nodes
            .first()
            .and_then(|(_, n)| n.as_ref())
            .map(|n| n.character)
        {
            let ticket = self.allegiances.propose(actor, patch, Vec::new())?;
            self.allegiances.cache_operation = Some(ticket.operation);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
