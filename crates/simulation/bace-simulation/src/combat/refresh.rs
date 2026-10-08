//! Rebuild complete physical derived values from accepted raw source ownership.
use super::*;
use bace_combat::preparation::{
    PhysicalQualityProjection, PreparedPhysicalRefreshSource, refresh_physical_from_source,
};
use std::sync::Arc;
impl Combat {
    pub(super) fn adopt_physical_ammunition_source(
        &mut self,
        actor: EntityId,
        receipt: bace_gameplay_api::weapon_combat::PhysicalLaunchReceipt,
        profile: Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    ) {
        let Some(source) = self.physical_sources.get_mut(&actor) else {
            return;
        };
        let mut updated = (**source).clone();
        if receipt.remaining == 0 {
            updated.equipment.retain(|e| e.entity != receipt.ammunition);
            updated.qualities.retain(|q| q.entity != receipt.ammunition);
        } else if let Some(e) = updated
            .equipment
            .iter_mut()
            .find(|e| e.entity == receipt.ammunition)
        {
            e.revision = receipt.after_revision;
            let mut raw = (*e.weenie).clone();
            let count = i32::try_from(receipt.remaining).expect("receipt source count preflight");
            if let Some(p) = raw.properties.ints.iter_mut().find(|p| p.id == 12) {
                p.value = count;
            } else {
                raw.properties.ints.push(bace_content::Property {
                    id: 12,
                    value: count,
                });
            }
            e.weenie = Arc::new(raw);
        }
        updated.profile = profile;
        *source = Arc::new(updated);
    }
    pub(crate) fn rebind_physical_registry_revision(
        &mut self,
        item: EntityId,
        before: u64,
        after: u64,
    ) -> Result<(), SkillRefreshError> {
        if before.checked_add(1) != Some(after) {
            return Err(SkillRefreshError::InvalidInput);
        }
        for profile in self.physical.values() {
            if profile
                .equipment
                .iter()
                .any(|e| e.entity == item.0 && e.revision != before)
            {
                return Err(SkillRefreshError::Busy);
            }
        }
        for source in self.physical_sources.values() {
            if source
                .equipment
                .iter()
                .any(|e| e.entity == item.0 && e.revision != before)
            {
                return Err(SkillRefreshError::Busy);
            }
        }
        for (&actor, profile) in &mut self.physical {
            if !profile.equipment.iter().any(|e| e.entity == item.0) {
                continue;
            }
            let mut updated = (**profile).clone();
            rebind(&mut updated, item, after);
            *profile = Arc::new(updated);
            if let Some(attack) = self.physical_attacks.get_mut(&actor) {
                attack.profile = profile.clone();
            }
            for missile in self
                .missiles
                .values_mut()
                .filter(|m| m.proposal.actor == actor.0 && !m.submitted)
            {
                missile.profile = profile.clone();
            }
        }
        for source in self.physical_sources.values_mut() {
            if !source.equipment.iter().any(|e| e.entity == item.0) {
                continue;
            }
            let mut updated = (**source).clone();
            for equipment in &mut updated.equipment {
                if equipment.entity == item.0 {
                    equipment.revision = after;
                }
            }
            let mut profile = (*updated.profile).clone();
            rebind(&mut profile, item, after);
            updated.profile = Arc::new(profile);
            *source = Arc::new(updated);
        }
        Ok(())
    }
    pub(crate) fn validate_physical_refresh_source(
        &self,
        actor: EntityId,
        source: &PreparedPhysicalRefreshSource,
    ) -> Result<(), SkillRefreshError> {
        if source.actor != actor.0 || source.qualities.len() > 32768 {
            return Err(SkillRefreshError::InvalidInput);
        }
        refresh_physical_from_source(source, &source.profile, &source.qualities)
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        if !self.physical_sources.contains_key(&actor)
            && self.physical_sources.len() >= self.capacity
        {
            return Err(SkillRefreshError::Capacity);
        }
        Ok(())
    }
    pub(crate) fn register_physical_refresh_source(
        &mut self,
        actor: EntityId,
        source: Arc<PreparedPhysicalRefreshSource>,
    ) -> Result<(), SkillRefreshError> {
        self.validate_physical_refresh_source(actor, &source)?;
        if self
            .physical
            .get(&actor)
            .is_none_or(|p| p.equipment != source.profile.equipment)
        {
            return Err(SkillRefreshError::InvalidInput);
        }
        self.physical_sources.insert(actor, source);
        Ok(())
    }
    pub(crate) fn physical_refresh_source(
        &self,
        actor: EntityId,
    ) -> Option<&Arc<PreparedPhysicalRefreshSource>> {
        self.physical_sources.get(&actor)
    }
    pub(crate) fn refresh_physical_qualities(
        &mut self,
        actor: EntityId,
        qualities: &[PhysicalQualityProjection],
        inventory: &crate::inventory::Inventory,
    ) -> Result<(), SkillRefreshError> {
        let Some(source) = self.physical_sources.get(&actor) else {
            return Ok(());
        };
        let current = self
            .physical
            .get(&actor)
            .ok_or(SkillRefreshError::MissingActor)?;
        if !super::equipment::equipment_current(current, actor, inventory) {
            return Err(SkillRefreshError::Busy);
        }
        let profile = refresh_physical_from_source(source, current, qualities)
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        let mut ratings = profile.ratings;
        if let Some(old) = self.physical_ratings.get(&actor) {
            ratings.reckless = old.reckless;
            ratings.sneak = old.sneak;
        }
        let selected = self
            .physical_attacks
            .get(&actor)
            .map(|a| {
                bace_combat::physical::select_melee(
                    &profile,
                    a.height,
                    a.power,
                    a.selected.hand == bace_gameplay_api::weapon_combat::PhysicalHand::Offhand,
                )
            })
            .transpose()
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        let profile = Arc::new(profile);
        self.physical_ratings.insert(actor, ratings);
        self.physical.insert(actor, profile.clone());
        if let Some(attack) = self.physical_attacks.get_mut(&actor) {
            attack.profile = profile;
            if let Some(selected) = selected {
                attack.selected.skill = selected.skill;
                attack.selected.skill_level = selected.skill_level;
            }
        }
        Ok(())
    }
}
fn rebind(
    profile: &mut bace_gameplay_api::weapon_combat::PhysicalCombatProfile,
    item: EntityId,
    revision: u64,
) {
    for stamp in &mut profile.equipment {
        if stamp.entity == item.0 {
            stamp.revision = revision;
        }
    }
    for weapon in [
        &mut profile.main,
        &mut profile.offhand,
        &mut profile.launcher,
        &mut profile.ammunition,
        &mut profile.gloves,
        &mut profile.boots,
    ]
    .into_iter()
    .flatten()
    {
        if weapon.entity == item.0 {
            weapon.revision = revision;
        }
    }
}
