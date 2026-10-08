//! Receipt-gated replacement of derived equipment profiles. Existing attack and
//! projectile owners are never cloned or replaced by an inventory operation.
use super::*;
use crate::PreparedEquipmentPhysical;
impl Combat {
    /// Preflight the post-receipt ACE stance shuffle against the rebuilt DAT
    /// closure. The physical driver will retain capacity pressure and retry it.
    pub(crate) fn validate_equipment_mode_replacement(
        &self,
        world: &World,
        prepared: &PreparedEquipmentPhysical,
        mode: u32,
    ) -> Result<(), SkillRefreshError> {
        let style = match mode {
            1 => 0x8000_003d,
            2 | 4 => prepared.source.profile.style,
            8 => 0x8000_0049,
            _ => return Err(SkillRefreshError::InvalidInput),
        };
        if !self.physical_drivers.contains_key(&prepared.actor)
            || world
                .combatant(prepared.actor)
                .is_none_or(|state| state.health() == 0)
        {
            return Err(SkillRefreshError::Busy);
        }
        let before = world
            .source_motion_state(prepared.actor)
            .ok_or(SkillRefreshError::InvalidInput)?;
        if before.style == style {
            return Ok(());
        }
        if !prepared.motions.iter().any(|(motion, speed, chain)| {
            *motion == style
                && *speed == 1.
                && chain.source_transition().is_some_and(|source| {
                    source.before.style == before.style
                        && source.before.substate == before.substate
                        && source.before.speed.is_sign_negative() == before.speed.is_sign_negative()
                        && source.after.style == style
                })
        }) {
            return Err(SkillRefreshError::InvalidInput);
        }
        Ok(())
    }
    /// The exact receipt already replaced the profile and motion cache. Queue
    /// the preflighted style in the existing bounded physical driver; pressure
    /// delays its motion packet without losing the durable equipment result.
    pub(crate) fn adopt_equipment_mode(&mut self, world: &mut World, actor: EntityId, mode: u32) {
        let style = match mode {
            1 => 0x8000_003d,
            2 | 4 => {
                self.physical
                    .get(&actor)
                    .expect("preflighted equipment profile")
                    .style
            }
            8 => 0x8000_0049,
            _ => unreachable!("preflighted equipment mode"),
        };
        let current = world
            .source_motion_state(actor)
            .expect("preflighted equipment source style");
        world
            .combatant_mut(actor)
            .expect("preflighted equipment combatant")
            .set_mode(mode);
        if current.style != style {
            self.physical_drivers
                .get_mut(&actor)
                .expect("preflighted equipment driver")
                .mode_style = Some(style);
        }
    }
    pub(crate) fn validate_equipment_replacement(
        &self,
        prepared: &PreparedEquipmentPhysical,
    ) -> Result<(), SkillRefreshError> {
        let actor = prepared.actor;
        if self.active(actor)
            || !self.physical.contains_key(&actor)
            || !self.physical_sources.contains_key(&actor)
        {
            return Err(SkillRefreshError::Busy);
        }
        self.can_set_skills(actor)?;
        self.validate_physical_refresh_source(actor, &prepared.source)?;
        let profile = &prepared.source.profile;
        bace_combat::physical::validate_physical_profile(profile)
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        if !profile.player
            || profile.equipment.len() > 64
            || prepared.motions.is_empty()
            || prepared.motions.len() > 64
        {
            return Err(SkillRefreshError::InvalidInput);
        }
        let existing = self
            .physical_motions
            .keys()
            .filter(|k| k.0 == actor)
            .count();
        if self.physical_motions.len() - existing + prepared.motions.len()
            > self.capacity.saturating_mul(64)
        {
            return Err(SkillRefreshError::Capacity);
        }
        let mut keys = std::collections::BTreeSet::new();
        for (motion, speed, chain) in &prepared.motions {
            let key = super::motion::key(actor, chain).ok_or(SkillRefreshError::InvalidInput)?;
            if *motion != chain.motion
                || *speed != chain.speed
                || !speed.is_finite()
                || *speed <= 0.
                || chain.stop_chain().is_none()
                || chain.nominal_duration_seconds() > 150.
                || !keys.insert(key)
            {
                return Err(SkillRefreshError::InvalidInput);
            }
        }
        Ok(())
    }
    pub(crate) fn adopt_equipment_replacement(
        &mut self,
        prepared: &PreparedEquipmentPhysical,
        values: &[bace_character::SkillValues],
        after_revision: u64,
        registry_revision: u64,
    ) {
        let actor = prepared.actor;
        self.physical_motions.retain(|key, _| key.0 != actor);
        for (_, _, chain) in &prepared.motions {
            self.physical_motions.insert(
                super::motion::key(actor, chain).expect("validated equipment motion"),
                chain.clone(),
            );
        }
        self.physical_ratings
            .insert(actor, prepared.source.profile.ratings);
        self.physical.insert(actor, prepared.source.profile.clone());
        self.physical_sources.insert(actor, prepared.source.clone());
        self.set_skills(actor, prepared.skills.clone(), values, after_revision)
            .expect("preflighted equipment skills");
        self.skills
            .get_mut(&actor)
            .expect("equipment skill owner")
            .registry_revision = Some(registry_revision);
    }
}
