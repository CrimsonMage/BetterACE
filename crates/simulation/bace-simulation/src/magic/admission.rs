use super::*;
impl Magic {
    pub(crate) fn validate_player_admission(
        &self,
        actor: EntityId,
        caster: &MagicCaster,
        registries: &[(EntityId, &EnchantmentRegistry)],
        recovery: bace_magic::CastRecovery,
        now: f64,
    ) -> Result<(), CastRejection> {
        if self.casters.contains_key(&actor)
            || self.recovery.contains_key(&actor)
            || !caster.player
            || caster.known_spells.len() > 8192
            || caster.school_skills.iter().any(|v| *v > i32::MAX as u32)
        {
            return Err(CastRejection::InvalidState);
        }
        if self.casters.len() >= 4096 || self.registries.len() + registries.len() > 4096 {
            return Err(CastRejection::Capacity);
        }
        if !now.is_finite() || now < self.current_time || now + 5.0 <= now {
            return Err(CastRejection::InvalidState);
        }
        let mut seen = std::collections::BTreeSet::new();
        for (id, _) in registries {
            if id.0 == 0
                || id.0 == u32::MAX
                || self.registries.contains_key(id)
                || !seen.insert(*id)
            {
                return Err(CastRejection::InvalidState);
            }
        }
        bace_magic::CastRecoveryClock::restore(recovery, now, 0.0).map_err(rejection)?;
        Ok(())
    }
}
impl Magic {
    pub(crate) fn preflight_npc_assets(
        &self,
        assets: &[(EntityId, std::sync::Arc<crate::PreparedNpcCombatAssets>)],
    ) -> Result<(), CastRejection> {
        let mut actors = std::collections::BTreeSet::new();
        let mut registries = std::collections::BTreeSet::new();
        let mut new_casters = 0;
        for (actor, input) in assets {
            if actor.0 == 0
                || actor.0 == u32::MAX
                || !actors.insert(*actor)
                || input.caster.player
                || input.damage.player
                || input.caster.known_spells.len() > 8192
                || input
                    .caster
                    .school_skills
                    .iter()
                    .chain([&input.caster.magic_defense, &input.caster.mana_conversion])
                    .any(|n| *n > i32::MAX as u32)
                || self.casters.get(actor).is_some_and(|c| c.player)
            {
                return Err(CastRejection::InvalidState);
            }
            if self.busy(*actor) || self.registry_reserved(*actor) {
                return Err(CastRejection::Busy);
            }
            bace_magic::validate_magic_damage_profile(&input.damage)
                .map_err(|_| CastRejection::InvalidState)?;
            new_casters += usize::from(!self.casters.contains_key(actor));
            if !registries.insert(*actor)
                || input.registry_items.len() > 4096
                || input
                    .equipment
                    .iter()
                    .any(|item| !input.registry_items.contains(&EntityId(item.entity)))
            {
                return Err(CastRejection::InvalidState);
            }
            for &item in &input.registry_items {
                if item == *actor || item.0 == 0 || item.0 == u32::MAX || !registries.insert(item) {
                    return Err(CastRejection::InvalidState);
                }
            }
        }
        if self.casters.len() + new_casters > 4096
            || self.registries.len()
                + registries
                    .iter()
                    .filter(|id| !self.registries.contains_key(id))
                    .count()
                > 4096
        {
            return Err(CastRejection::Capacity);
        }
        Ok(())
    }
    pub(crate) fn adopt_npc_assets(
        &mut self,
        actor: EntityId,
        input: &crate::PreparedNpcCombatAssets,
        world: &World,
        now: f64,
    ) -> Result<(), CastRejection> {
        for id in std::iter::once(actor).chain(input.registry_items.iter().copied()) {
            if !self.registries.contains_key(&id) {
                let registry =
                    EnchantmentRegistry::new(512).map_err(|_| CastRejection::Capacity)?;
                self.register_registry(id, registry, true, now)
                    .map_err(|(e, _)| e)?;
            }
        }
        if let Some(caster) = self.casters.get_mut(&actor) {
            *caster = input.caster.clone();
        } else {
            self.register_caster(actor, input.caster.clone(), world)?;
        }
        if self.damage_profiles.contains_key(&actor) {
            self.refresh_damage_profile(actor, input.damage.clone())?;
        } else {
            self.register_damage_profile(actor, input.damage.clone())?;
        }
        self.server_mana.insert(
            actor,
            input
                .source
                .properties
                .bools
                .iter()
                .find(|p| p.id == 6)
                .is_none_or(|p| p.value),
        );
        Ok(())
    }
}
