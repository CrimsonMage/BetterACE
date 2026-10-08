//! Rebuild derived physical qualities only when an authoritative registry owner
//! changes. Raw accepted sources are immutable and never compounded with buffs.
use super::*;
use bace_combat::preparation::{PhysicalQualityFamily as F, PhysicalQualityProjection};
impl Kernel {
    pub fn register_physical_refresh_source(
        &mut self,
        actor: EntityId,
        source: std::sync::Arc<bace_combat::preparation::PreparedPhysicalRefreshSource>,
    ) -> Result<(), crate::SkillRefreshError> {
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
        {
            return Err(crate::SkillRefreshError::Busy);
        }
        self.combat
            .register_physical_refresh_source(actor, source)?;
        self.physical_refresh_dirty.insert(actor);
        Ok(())
    }
    pub(super) fn refresh_physical_qualities_actor(
        &mut self,
        actor: EntityId,
    ) -> Result<(), crate::SkillRefreshError> {
        let Some(source) = self.combat.physical_refresh_source(actor).cloned() else {
            return Ok(());
        };
        let mut qualities = Vec::with_capacity(source.qualities.len());
        for raw in &source.qualities {
            let registry = self
                .magic
                .registry(EntityId(raw.entity))
                .ok_or(crate::SkillRefreshError::InvalidInput)?;
            let (details, value) = match raw.family {
                F::Int => bace_magic::enchant_physical_quality(
                    registry,
                    4,
                    raw.stat,
                    raw.details.raw,
                    true,
                ),
                F::Float => bace_magic::enchant_physical_quality(
                    registry,
                    8,
                    raw.stat,
                    raw.details.raw,
                    false,
                ),
                F::BodyArmor => {
                    bace_magic::enchant_body_quality(registry, raw.stat, raw.details.raw)
                        .map(|q| (q, raw.details.raw))
                }
            }
            .map_err(|_| crate::SkillRefreshError::InvalidInput)?;
            qualities.push(PhysicalQualityProjection {
                details,
                value,
                ..*raw
            });
        }
        self.combat
            .refresh_physical_qualities(actor, &qualities, &self.inventory)?;
        let physical = self
            .combat
            .physical_profile(actor)
            .ok_or(crate::SkillRefreshError::MissingActor)?;
        let mut equipment = source.equipment.iter().collect::<Vec<_>>();
        equipment.sort_by_key(|item| {
            self.item_experience
                .items
                .get(&EntityId(item.entity))
                .map_or(u64::from(item.entity), |p| p.equipment_order)
        });
        let equipment = equipment
            .into_iter()
            .map(|item| {
                let level = self
                    .item_experience
                    .items
                    .get(&EntityId(item.entity))
                    .and_then(|p| p.experience)
                    .map(|xp| {
                        xp.level()
                            .map_err(|_| crate::SkillRefreshError::InvalidInput)
                    })
                    .transpose()?;
                Ok(bace_magic::MagicDamageEquipment {
                    entity: item.entity,
                    revision: item.revision,
                    location: item.location,
                    level,
                    weenie: &item.weenie,
                })
            })
            .collect::<Result<Vec<_>, crate::SkillRefreshError>>()?;
        let base_shield_skill = self
            .combat
            .skills
            .get(&actor)
            .and_then(|s| s.values.get(&48))
            .map_or(0, |s| s.base);
        let profile =
            bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
                player: physical.player,
                weenie: &source.weenie,
                equipment: &equipment,
                skills: &physical.skills,
                base_attributes: [physical.base_strength, physical.base_endurance, 0, 0, 0, 0],
                base_shield_skill,
            })
            .map_err(|_| crate::SkillRefreshError::InvalidInput)?;
        match self.magic.refresh_damage_profile(actor, profile) {
            Ok(()) => Ok(()),
            Err(bace_gameplay_api::CastRejection::MissingActor)
                if self
                    .world
                    .body(actor)
                    .is_ok_and(|b| b.collision_shape().is_none()) =>
            {
                Ok(())
            }
            Err(bace_gameplay_api::CastRejection::Busy) => Err(crate::SkillRefreshError::Busy),
            Err(_) => Err(crate::SkillRefreshError::InvalidInput),
        }
    }
    pub(super) fn refresh_changed_physical_qualities(
        &mut self,
    ) -> Result<(), crate::SkillRefreshError> {
        self.physical_refresh_scratch.clear();
        self.physical_refresh_scratch
            .extend(self.physical_refresh_dirty.iter().copied());
        for index in 0..self.physical_refresh_scratch.len() {
            let actor = self.physical_refresh_scratch[index];
            if self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.npcs.reserved(actor)
                || self.magic.registry_reserved(actor)
            {
                continue;
            }
            self.refresh_npc_combat_skills(actor)?;
            match self.refresh_physical_qualities_actor(actor) {
                Ok(()) => {
                    self.physical_refresh_dirty.remove(&actor);
                    self.locomotion_dirty.insert(actor);
                }
                Err(crate::SkillRefreshError::Busy) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
