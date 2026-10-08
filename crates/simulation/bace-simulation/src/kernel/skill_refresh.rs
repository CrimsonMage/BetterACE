//! Composition of prepared DAT/modifier inputs with the sole progression owner.
use super::*;
use crate::combat::{PreparedCharacterSkillInputs, SkillRefreshError};
use bace_gameplay_api::{
    AttributeId, ProgressionChange, ProgressionTarget, RankEffect, TraitDetails,
};
impl Kernel {
    /// Freeze the source rank announcement from the same accepted simulation
    /// state that produced the private trait update. Missing prepared live
    /// inputs retain the obligation instead of inventing a base value.
    pub(super) fn attach_rank_effect(
        &self,
        actor: EntityId,
        change: &mut ProgressionChange,
    ) -> Result<(), SkillRefreshError> {
        if change.before.ranks == change.after.ranks {
            return Ok(());
        }
        let character = self
            .characters
            .get(actor)
            .ok_or(SkillRefreshError::MissingActor)?;
        let reached_maximum = change.after.ranks
            == character
                .maximum_rank(change.after.target)
                .ok_or(SkillRefreshError::InvalidInput)?;
        let base = match change.after.target {
            ProgressionTarget::Attribute(_) => {
                let Some(TraitDetails::Attribute { starting_value }) = change.after.details else {
                    return Ok(());
                };
                starting_value.checked_add(u32::from(change.after.ranks))
            }
            ProgressionTarget::Skill(skill) => self
                .combat
                .skills
                .get(&actor)
                .and_then(|profile| profile.values.get(&skill))
                .map(|value| value.base),
            ProgressionTarget::Vital(vital) => {
                let Some(inputs) = self.vital_inputs.get(&actor) else {
                    return Ok(());
                };
                let Some(TraitDetails::Vital { starting_value, .. }) = change.after.details else {
                    return Ok(());
                };
                let index = match vital {
                    bace_gameplay_api::VitalId::MaxHealth => 0,
                    bace_gameplay_api::VitalId::MaxStamina => 1,
                    bace_gameplay_api::VitalId::MaxMana => 2,
                };
                let mut attributes = [0u32; 6];
                for (index, value) in attributes.iter_mut().enumerate() {
                    let id = AttributeId::try_from(index as u32 + 1)
                        .map_err(|_| SkillRefreshError::InvalidInput)?;
                    let Some(TraitDetails::Attribute { starting_value }) = character
                        .projection(ProgressionTarget::Attribute(id))
                        .and_then(|p| p.details)
                    else {
                        return Err(SkillRefreshError::InvalidInput);
                    };
                    let ranks = character
                        .projection(ProgressionTarget::Attribute(id))
                        .ok_or(SkillRefreshError::InvalidInput)?
                        .ranks;
                    *value = starting_value
                        .checked_add(u32::from(ranks))
                        .ok_or(SkillRefreshError::InvalidInput)?;
                }
                let gear = if index == 0 {
                    inputs.equipped_health.iter().try_fold(0u32, |sum, (item, health)| {
                        if self.inventory.item(*item).is_some_and(|i| matches!(i.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)) {
                            sum.checked_add(*health)
                        } else {
                            Some(sum)
                        }
                    }).ok_or(SkillRefreshError::InvalidInput)?
                } else {
                    0
                };
                let enlightenment = self
                    .characters
                    .native_services(actor)
                    .ok_or(SkillRefreshError::InvalidInput)?
                    .enlightenment;
                let base_bonus = if index == 0 {
                    enlightenment
                        .checked_mul(2)
                        .and_then(|v| v.checked_add(gear))
                        .ok_or(SkillRefreshError::InvalidInput)?
                } else {
                    0
                };
                Some(
                    bace_character::project_vital_values(bace_character::VitalValueInputs {
                        formula: inputs.formulas[index],
                        starting_value,
                        ranks: u32::from(change.after.ranks),
                        current_attributes: attributes,
                        base_bonus,
                        multiplier: 1.0,
                        vitae: 1.0,
                        additive: 0.0,
                    })
                    .map_err(|_| SkillRefreshError::InvalidInput)?
                    .before_multipliers,
                )
            }
        };
        if let Some(base) = base {
            change.rank_effect = Some(RankEffect {
                base,
                reached_maximum,
            });
        }
        Ok(())
    }
    pub fn configure_combat_random_shared(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
        epoch: u64,
    ) -> Result<(), SkillRefreshError> {
        self.combat.configure_random(root, epoch)
    }
    pub fn register_character_skill_inputs(
        &mut self,
        actor: EntityId,
        prepared: PreparedCharacterSkillInputs,
    ) -> Result<(), SkillRefreshError> {
        if self.characters.reserved(actor) || self.combat.active(actor) || self.magic.busy(actor) {
            return Err(SkillRefreshError::Busy);
        }
        self.install_skill_inputs(actor, prepared)
    }
    /// Reuse bounded actor scratch and refresh only changed owner revisions.
    /// Reserved operations retain refresh debt. Active casts keep their admitted
    /// skill snapshot while future casts and live defenses receive new values.
    pub fn refresh_changed_magic_skills(&mut self) -> Result<(), SkillRefreshError> {
        self.combat.refresh_scratch.clear();
        self.combat
            .refresh_scratch
            .extend(self.combat.skills.iter().filter_map(|(actor, skills)| {
                (!self.npc_combat_assets.contains_key(actor)
                    && skills.registry_revision
                        != self.magic.registry(*actor).map(|r| r.revision()))
                .then_some(*actor)
            }));
        for index in 0..self.combat.refresh_scratch.len() {
            let actor = self.combat.refresh_scratch[index];
            if self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.npcs.reserved(actor)
                || self.magic.registry_reserved(actor)
                || self.world.has_reserved_vitals(actor)
            {
                continue;
            }
            self.refresh_character_skills(actor)?;
        }
        Ok(())
    }
    pub fn refresh_character_skills(&mut self, actor: EntityId) -> Result<(), SkillRefreshError> {
        if self.refresh_npc_combat_skills(actor)? {
            return Ok(());
        }
        let Some(current) = self.combat.skills.get(&actor) else {
            return Ok(());
        };
        if self
            .characters
            .get(actor)
            .is_some_and(|c| c.revision() == current.revision)
            && current.registry_revision == self.magic.registry(actor).map(|r| r.revision())
        {
            return Ok(());
        }
        self.install_skill_inputs(actor, current.prepared.clone())
    }
    /// Read-only admission before a valuable operation is submitted to storage.
    pub fn validate_proposed_skill_refresh(
        &self,
        actor: EntityId,
        proposed: &CharacterProgression,
    ) -> Result<(), SkillRefreshError> {
        let Some(current) = self.combat.skills.get(&actor) else {
            return Ok(());
        };
        self.validate_prepared_skill_refresh(actor, proposed, &current.prepared)
    }
    fn validate_prepared_skill_refresh(
        &self,
        actor: EntityId,
        character: &CharacterProgression,
        prepared: &PreparedCharacterSkillInputs,
    ) -> Result<(), SkillRefreshError> {
        self.combat.can_set_skills(actor)?;
        let values = project_skills(character, prepared, self.magic.registry(actor))?;
        if self.magic.has_caster(actor)
            && [31, 32, 33, 34, 43, 15, 16]
                .iter()
                .any(|id| !values.iter().any(|v| v.skill == *id))
        {
            return Err(SkillRefreshError::MissingSkill);
        }
        Ok(())
    }
    pub fn character_skill_projection(
        &self,
        actor: EntityId,
        skill: u32,
    ) -> Option<(u64, bace_combat::specialization::CombatSkill)> {
        let profile = self.combat.skills.get(&actor)?;
        Some((profile.revision, *profile.values.get(&skill)?))
    }
    fn install_skill_inputs(
        &mut self,
        actor: EntityId,
        prepared: PreparedCharacterSkillInputs,
    ) -> Result<(), SkillRefreshError> {
        let character = self
            .characters
            .get(actor)
            .ok_or(SkillRefreshError::MissingActor)?;
        self.validate_prepared_skill_refresh(actor, character, &prepared)?;
        let values = project_skills(character, &prepared, self.magic.registry(actor))?;
        let maxima = self.project_live_maxima(actor)?;
        if let Some(maxima) = maxima {
            self.world
                .validate_vital_maxima(actor, maxima)
                .map_err(|_| SkillRefreshError::Busy)?;
        }
        let revision = character.revision();
        let (base_attributes, current_attributes) =
            super::skill_modifiers::attributes(character, &prepared, self.magic.registry(actor))?;
        self.combat
            .refresh_physical_skills(actor, &values, current_attributes, base_attributes)?;
        let physical_skill = |id: u32, base: bool| {
            values.iter().find(|v| v.skill == id).map_or(
                bace_gameplay_api::weapon_combat::PhysicalSkill {
                    advancement: 0,
                    current: 0,
                },
                |v| bace_gameplay_api::weapon_combat::PhysicalSkill {
                    advancement: v.advancement as u32,
                    current: if base { v.base } else { v.current },
                },
            )
        };
        let base_attribute = |id| -> Result<u32, SkillRefreshError> {
            let Some(value) = character.projection(ProgressionTarget::Attribute(id)) else {
                return Ok(0);
            };
            let Some(TraitDetails::Attribute { starting_value }) = value.details else {
                return Err(SkillRefreshError::InvalidInput);
            };
            starting_value
                .checked_add(u32::from(value.ranks))
                .ok_or(SkillRefreshError::InvalidInput)
        };
        self.magic
            .refresh_damage_skills(
                actor,
                [
                    physical_skill(15, false),
                    physical_skill(51, false),
                    physical_skill(20, false),
                    physical_skill(19, false),
                    physical_skill(48, true),
                ],
                [
                    base_attribute(AttributeId::Strength)?,
                    base_attribute(AttributeId::Endurance)?,
                ],
            )
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        if self.magic.has_caster(actor) {
            let current = |id| {
                values
                    .iter()
                    .find(|v| v.skill == id)
                    .map(|v| v.current)
                    .ok_or(SkillRefreshError::MissingSkill)
            };
            self.magic
                .refresh_caster_skills(
                    actor,
                    [
                        current(34)?,
                        current(33)?,
                        current(31)?,
                        current(32)?,
                        current(43)?,
                    ],
                    current(15)?,
                    current(16)?,
                )
                .map_err(|_| SkillRefreshError::Busy)?;
        }
        if self.magic.registry(actor).is_some() {
            let skill = |id| {
                values
                    .iter()
                    .find(|v| v.skill == id)
                    .map(|v| bace_combat::specialization::CombatSkill {
                        advancement: v.advancement,
                        base: v.base,
                        current: v.current,
                    })
                    .unwrap_or(bace_combat::specialization::CombatSkill {
                        advancement: bace_gameplay_api::SkillAdvancement::Inactive,
                        base: 0,
                        current: 0,
                    })
            };
            self.magic
                .refresh_magic_defenses(
                    actor,
                    crate::magic::MagicDefenseProfile {
                        player: true,
                        magic_defense: skill(15),
                        shield: prepared.shield.map(|s| (skill(48), s.magic_absorption)),
                    },
                )
                .map_err(|_| SkillRefreshError::InvalidInput)?;
        }
        self.combat.set_skills(actor, prepared, &values, revision)?;
        match self.refresh_physical_qualities_actor(actor) {
            Ok(()) => {
                self.physical_refresh_dirty.remove(&actor);
            }
            Err(SkillRefreshError::Busy) => {
                self.physical_refresh_dirty.insert(actor);
            }
            Err(error) => return Err(error),
        }
        if let Some(maxima) = maxima {
            self.world
                .replace_vital_maxima(actor, maxima)
                .map_err(|_| SkillRefreshError::InvalidInput)?;
        }
        let profile = self
            .combat
            .skills
            .get_mut(&actor)
            .expect("installed profile");
        profile.registry_revision = self.magic.registry(actor).map(|r| r.revision());
        profile.dirty_signature = [
            self.magic.dirty_skill_modifier(actor, 6),
            self.magic.dirty_skill_modifier(actor, 44),
        ];
        self.locomotion_dirty.insert(actor);
        Ok(())
    }
    /// Read projection for an authoritative healer owner. This computes the
    /// source skill contest inputs; it does not spend a kit or mutate a vital.
    pub fn project_healing_check(
        &self,
        actor: EntityId,
        kit_boost: i32,
        vital: bace_entity::EntityVital,
    ) -> Result<bace_combat::specialization::HealingCheck, SkillRefreshError> {
        let profile = self
            .combat
            .skills
            .get(&actor)
            .ok_or(SkillRefreshError::MissingSkill)?;
        let skill = *profile
            .values
            .get(&21)
            .ok_or(SkillRefreshError::MissingSkill)?;
        let body = self
            .world
            .combatant(actor)
            .ok_or(SkillRefreshError::MissingActor)?;
        let pool = body.vital(vital).ok_or(SkillRefreshError::InvalidInput)?;
        bace_combat::specialization::healing_check(
            skill,
            kit_boost,
            pool.maximum - pool.current,
            body.mode() != 1,
        )
        .map_err(|_| SkillRefreshError::InvalidInput)
    }
    /// Shared accepted view for melee, missile and magic damage owners.
    pub fn project_specialized_defense_rating(
        &self,
        actor: EntityId,
        kind: bace_combat::specialization::DefenseKind,
    ) -> Result<u32, SkillRefreshError> {
        use bace_combat::specialization::{DefenseKind, specialized_defense_rating};
        let skill = match kind {
            DefenseKind::Melee => 6,
            DefenseKind::Missile => 7,
            DefenseKind::Magic => 15,
        };
        let profile = self
            .combat
            .skills
            .get(&actor)
            .ok_or(SkillRefreshError::MissingSkill)?;
        let skill = *profile
            .values
            .get(&skill)
            .ok_or(SkillRefreshError::MissingSkill)?;
        let player = self
            .world
            .combatant(actor)
            .ok_or(SkillRefreshError::MissingActor)?
            .profile()
            .player;
        Ok(specialized_defense_rating(player, kind, skill))
    }
    pub fn project_shield_armor_cap(&self, actor: EntityId) -> Result<u32, SkillRefreshError> {
        let profile = self
            .combat
            .skills
            .get(&actor)
            .ok_or(SkillRefreshError::MissingSkill)?;
        let skill = *profile
            .values
            .get(&48)
            .ok_or(SkillRefreshError::MissingSkill)?;
        Ok(bace_combat::specialization::shield_armor_cap(skill))
    }
    pub fn pending_dirty_fighting(&self) -> Option<crate::combat::DirtyFightingImpact> {
        self.combat.peek_dirty()
    }
    /// Acknowledge only after every listed effect has been admitted atomically
    /// by magic. Output pressure must leave this queued and pause new strikes.
    pub fn acknowledge_dirty_fighting(
        &mut self,
        expected: crate::combat::DirtyFightingImpact,
    ) -> Result<(), SkillRefreshError> {
        if self.combat.peek_dirty() != Some(expected) {
            return Err(SkillRefreshError::InvalidInput);
        }
        self.combat.take_dirty();
        Ok(())
    }
}

pub(super) fn project_skills(
    character: &CharacterProgression,
    prepared: &PreparedCharacterSkillInputs,
    registry: Option<&bace_magic::EnchantmentRegistry>,
) -> Result<Vec<bace_character::SkillValues>, SkillRefreshError> {
    if prepared.inputs.len() > 256
        || prepared.inputs.is_empty()
        || prepared.shield.is_some_and(|s| {
            !s.effective_armor.is_finite()
                || !s.magic_absorption.is_finite()
                || !(0.0..=1.0).contains(&s.magic_absorption)
        })
    {
        return Err(SkillRefreshError::InvalidInput);
    }
    let current_prepared = super::skill_modifiers::compose(prepared, registry)?;
    let projected = &current_prepared;
    let mut base_attributes = [0u32; 6];
    let mut current_attributes = [0u32; 6];
    let mut attribute_count = 0;
    for (index, modifier) in projected.attribute_modifiers.iter().enumerate() {
        if !modifier.multiplier.is_finite() || modifier.multiplier < 0.0 {
            return Err(SkillRefreshError::InvalidInput);
        }
        let id =
            AttributeId::try_from(index as u32 + 1).map_err(|_| SkillRefreshError::InvalidInput)?;
        if let Some(p) = character.projection(ProgressionTarget::Attribute(id)) {
            let Some(TraitDetails::Attribute { starting_value }) = p.details else {
                return Err(SkillRefreshError::InvalidInput);
            };
            let base = starting_value
                .checked_add(p.ranks.into())
                .ok_or(SkillRefreshError::InvalidInput)?;
            if base > i32::MAX as u32 {
                return Err(SkillRefreshError::InvalidInput);
            }
            let current = (base as f32 * modifier.multiplier + modifier.additive as f32)
                .round()
                .max(if base >= 10 { 10.0 } else { 1.0 });
            if !current.is_finite() || current as f64 > i32::MAX as f64 {
                return Err(SkillRefreshError::InvalidInput);
            }
            base_attributes[index] = base;
            current_attributes[index] = current as u32;
            attribute_count += 1;
        }
    }
    if attribute_count != 0 && attribute_count != 6 {
        return Err(SkillRefreshError::InvalidInput);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut values = Vec::with_capacity(prepared.inputs.len());
    for (skill, input) in &projected.inputs {
        if !ids.insert(*skill) {
            return Err(SkillRefreshError::InvalidInput);
        }
        let mut input = *input;
        if attribute_count == 6 {
            input.base_attributes = base_attributes;
            input.current_attributes = current_attributes;
        }
        let value = character
            .skill_values(*skill, input)
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        if value.current > i32::MAX as u32 {
            return Err(SkillRefreshError::InvalidInput);
        }
        values.push(value);
    }
    if !ids.contains(&prepared.attack_skill)
        || character
            .traits()
            .any(|t| matches!(t.target,ProgressionTarget::Skill(id) if !ids.contains(&id)))
    {
        return Err(SkillRefreshError::MissingSkill);
    }
    Ok(values)
}
