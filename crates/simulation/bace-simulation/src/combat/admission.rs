use super::*;
impl Combat {
    pub(crate) fn validate_player_admission(
        &self,
        actor: EntityId,
        profile: &bace_gameplay_api::weapon_combat::PhysicalCombatProfile,
    ) -> Result<(), CombatRejection> {
        bace_combat::physical::validate_physical_profile(profile)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        if self.physical.contains_key(&actor) || self.active(actor) {
            return Err(CombatRejection::Busy);
        }
        if self.physical.len() >= self.capacity
            || (!self.physical_deadlines.contains_key(&actor)
                && self.physical_deadlines.len() >= self.capacity)
            || profile
                .main
                .as_ref()
                .is_some_and(|w| w.cleave_targets as usize > self.capacity)
            || profile
                .offhand
                .as_ref()
                .is_some_and(|w| w.cleave_targets as usize > self.capacity)
        {
            return Err(CombatRejection::Capacity);
        }
        self.can_set_skills(actor)
            .map_err(|_| CombatRejection::Capacity)?;
        Ok(())
    }
}
