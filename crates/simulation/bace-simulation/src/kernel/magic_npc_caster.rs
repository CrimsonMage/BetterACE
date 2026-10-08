//! NPC casting/defense uses accepted creature skills and raw sources. No player
//! progression/account is fabricated for an NPC. Registry refresh is derived only.
mod restoration;
use super::*;
use crate::{PreparedNpcCombatAssets, SkillRefreshError as E};
use bace_gameplay_api::{CastRejection, weapon_combat::PhysicalSkill};
use std::sync::Arc;
impl Kernel {
    pub(crate) fn preflight_npc_combat_assets(
        &self,
        assets: &[(EntityId, Arc<PreparedNpcCombatAssets>)],
    ) -> Result<(), E> {
        if assets.len() > 4096
            || self.npc_combat_assets.len()
                + assets
                    .iter()
                    .filter(|(id, _)| !self.npc_combat_assets.contains_key(id))
                    .count()
                > self.outcome_capacity
            || self.combat.skills.len()
                + assets
                    .iter()
                    .filter(|(id, _)| !self.combat.skills.contains_key(id))
                    .count()
                > self.outcome_capacity
        {
            return Err(E::Capacity);
        }
        self.magic
            .preflight_npc_assets(assets)
            .map_err(magic_error)?;
        self.magic
            .validate_npc_proc_batches(assets)
            .map_err(magic_error)?;
        for (actor, input) in assets {
            input
                .source
                .validate(Default::default())
                .map_err(|_| E::InvalidInput)?;
            if input.physical.player
                || input.skills.len() > 256
                || input.skills.len() != input.skill_inputs.inputs.len()
            {
                return Err(E::InvalidInput);
            }
            if !self.npc_combat_assets.contains_key(actor) && self.magic.has_caster(*actor) {
                return Err(E::InvalidInput);
            }
            let source = input.bind_physical_source(*actor)?;
            self.combat
                .validate_physical_refresh_source(*actor, &source)?;
            npc_values(input, self.magic.registry(*actor))?;
            self.prepare_npc_registry_restore(*actor, input)?;
        }
        Ok(())
    }
    pub(crate) fn adopt_npc_combat_assets(
        &mut self,
        actor: EntityId,
        mut input: Arc<PreparedNpcCombatAssets>,
    ) -> Result<(), E> {
        self.preflight_npc_combat_assets(&[(actor, input.clone())])?;
        let restored = self.prepare_npc_registry_restore(actor, &input)?;
        if self
            .world
            .combatant(actor)
            .is_none_or(|c| c.profile().player)
            || self
                .world
                .vital(actor, bace_entity::EntityVital::Mana)
                .is_err()
            || self
                .combat
                .physical_profile(actor)
                .is_none_or(|p| p.equipment != input.physical.equipment)
        {
            return Err(E::MissingActor);
        }
        self.magic
            .register_instant_batch(input.server_magic.clone())
            .map_err(magic_error)?;
        if self.world.properties(actor).is_none() {
            self.world
                .register_properties(actor, input.properties.clone())
                .expect("NPC body identity and absent property owner preflight");
        }
        let source = input.bind_physical_source(actor)?;
        let (values, attributes) = if let Some(restored) = &restored {
            (restored.values.clone(), restored.attributes)
        } else {
            npc_values(&input, self.magic.registry(actor))?
        };
        // All fallible shape/profile/skill/registry capacity checks precede owner
        // publication. These calls consume only the same immutable checked data.
        let fresh = !self.npc_combat_assets.contains_key(&actor);
        let registry_ids: Vec<_> = std::iter::once(actor)
            .chain(input.registry_items.iter().copied())
            .filter(|id| self.magic.registry(*id).is_none())
            .collect();
        self.magic
            .adopt_npc_assets(actor, &input, &self.world, self.tick as f64 / 30.)
            .map_err(magic_error)?;
        if let Some(restored) = restored {
            self.magic.adopt_empty_registry_restore(restored.registries);
            self.combat
                .register_physical(actor, restored.physical, &self.world)
                .expect("NPC restored profile preflight");
        }
        self.combat
            .register_physical_refresh_source(actor, source)
            .expect("same-owner NPC source preflight");
        self.combat
            .set_skills(actor, input.skill_inputs.clone(), &values, 0)
            .expect("NPC skill capacity preflight");
        if fresh {
            self.npc_combat_staging.insert(actor, registry_ids);
        } else if let Some(ids) = self.npc_combat_staging.get_mut(&actor) {
            ids.extend(registry_ids);
        }
        for id in std::iter::once(actor).chain(input.registry_items.iter().copied()) {
            let revision = self
                .magic
                .registry(id)
                .expect("adopted NPC registry")
                .revision();
            self.registry_revisions.entry(id).or_insert(revision);
        }
        Arc::make_mut(&mut input).restored_registries = None;
        self.npc_combat_assets.insert(actor, input);
        self.adopt_npc_skill_values(actor, &values, attributes)?;
        self.physical_refresh_dirty.insert(actor);
        Ok(())
    }
    pub(super) fn refresh_npc_combat_skills(&mut self, actor: EntityId) -> Result<bool, E> {
        self.refresh_npc_scalar_source(actor)?;
        let Some(input) = self.npc_combat_assets.get(&actor).cloned() else {
            return Ok(false);
        };
        let (values, attributes) = npc_values(&input, self.magic.registry(actor))?;
        self.combat
            .set_skills(
                actor,
                input.skill_inputs.clone(),
                &values,
                self.world.properties(actor).map_or(0, |p| p.revision()),
            )
            .map_err(|_| E::Capacity)?;
        self.adopt_npc_skill_values(actor, &values, attributes)?;
        Ok(true)
    }
    fn adopt_npc_skill_values(
        &mut self,
        actor: EntityId,
        values: &[bace_character::SkillValues],
        attributes: [u32; 6],
    ) -> Result<(), E> {
        let input = self.npc_combat_assets.get(&actor).ok_or(E::MissingActor)?;
        self.combat
            .refresh_physical_skills(actor, values, attributes, input.base_attributes)?;
        let current = |id| {
            values
                .iter()
                .find(|s| s.skill == id)
                .map_or(0, |s| s.current)
        };
        self.magic
            .refresh_caster_skills(
                actor,
                [34, 33, 31, 32, 43].map(current),
                current(15),
                current(16),
            )
            .map_err(magic_error)?;
        let skills = [15, 51, 20, 19, 48].map(|id| {
            values.iter().find(|s| s.skill == id).map_or(
                PhysicalSkill {
                    advancement: 0,
                    current: 0,
                },
                |s| PhysicalSkill {
                    advancement: s.advancement as u32,
                    current: if id == 48 { s.base } else { s.current },
                },
            )
        });
        self.magic
            .refresh_damage_skills(
                actor,
                skills,
                [input.base_attributes[0], input.base_attributes[1]],
            )
            .map_err(magic_error)?;
        let registry_revision = self.magic.registry(actor).map(|r| r.revision());
        let cache = self.combat.skills.get_mut(&actor).ok_or(E::MissingActor)?;
        cache.registry_revision = registry_revision;
        Ok(())
    }
    /// Exact cast source scalar changes may refresh raw actor properties, while
    /// immutable equipment remains the existing admitted physical-source owner.
    pub(super) fn restore_npc_object_registry(
        &mut self,
        actor: EntityId,
        revision: u64,
        entries: Vec<bace_magic::EnchantmentEntry>,
    ) -> Result<(), CastRejection> {
        self.magic.restore_object_registry(
            actor,
            revision,
            entries,
            &self.world,
            self.tick as f64 / 30.,
        )?;
        self.registry_revisions.insert(actor, revision);
        Ok(())
    }
    pub(super) fn prepare_instant_object_caster(
        &mut self,
        actor: EntityId,
        source: &bace_content::WeenieV1,
    ) -> Result<(), CastRejection> {
        self.magic
            .register_object_caster(actor, source, &self.world)
    }
    pub(super) fn prepare_npc_magic_caster(
        &mut self,
        actor: EntityId,
        source: Arc<bace_content::WeenieV1>,
    ) -> Result<(), CastRejection> {
        let existing = self
            .npc_combat_assets
            .get(&actor)
            .cloned()
            .ok_or(CastRejection::MissingAssets)?;
        if source.weenie_id != existing.source.weenie_id
            || source.weenie_type != existing.source.weenie_type
            || source.properties.attributes != existing.source.properties.attributes
            || source.properties.skills != existing.source.properties.skills
        {
            return Err(CastRejection::InvalidState);
        }
        self.adopt_npc_scalar_source(actor, source)
            .map_err(|_| CastRejection::InvalidState)?;
        self.refresh_npc_combat_skills(actor)
            .map_err(|_| CastRejection::InvalidState)?;
        self.refresh_physical_qualities_actor(actor)
            .map_err(|_| CastRejection::InvalidState)?;
        Ok(())
    }
    pub(super) fn refresh_npc_scalar_source(&mut self, actor: EntityId) -> Result<(), E> {
        let Some(input) = self.npc_combat_assets.get(&actor) else {
            return self
                .magic
                .refresh_object_properties(actor, &self.world)
                .map_err(magic_error);
        };
        let Some(properties) = self.world.properties(actor) else {
            return Ok(());
        };
        let source =
            crate::npc_combat_assets::overlay_npc_source_qualities(&input.source, properties)
                .map_err(|_| E::InvalidInput)?;
        if source != *input.source {
            self.adopt_npc_scalar_source(actor, Arc::new(source))?;
        }
        Ok(())
    }
    fn adopt_npc_scalar_source(
        &mut self,
        actor: EntityId,
        source: Arc<bace_content::WeenieV1>,
    ) -> Result<(), E> {
        let (input, raw) = self.prepare_npc_scalar_source(actor, source)?;
        self.combat
            .register_physical_refresh_source(actor, Arc::new(raw))?;
        self.npc_combat_assets.insert(actor, Arc::new(input));
        self.physical_refresh_dirty.insert(actor);
        Ok(())
    }
    fn prepare_npc_scalar_source(
        &self,
        actor: EntityId,
        source: Arc<bace_content::WeenieV1>,
    ) -> Result<
        (
            PreparedNpcCombatAssets,
            bace_combat::preparation::PreparedPhysicalRefreshSource,
        ),
        E,
    > {
        let mut input = (**self.npc_combat_assets.get(&actor).ok_or(E::MissingActor)?).clone();
        let mut raw = (**self
            .combat
            .physical_refresh_source(actor)
            .ok_or(E::MissingActor)?)
        .clone();
        for q in &mut input.actor_qualities {
            let value = match q.family {
                bace_combat::preparation::PhysicalQualityFamily::Int => source
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == q.stat)
                    .map_or(0., |p| f64::from(p.value)),
                bace_combat::preparation::PhysicalQualityFamily::Float => source
                    .properties
                    .floats
                    .iter()
                    .find(|p| p.id == q.stat)
                    .map_or(
                        if [
                            13, 14, 15, 16, 17, 18, 19, 64, 65, 66, 67, 68, 69, 70, 165, 166,
                        ]
                        .contains(&q.stat)
                        {
                            1.
                        } else {
                            0.
                        },
                        |p| p.value,
                    ),
                bace_combat::preparation::PhysicalQualityFamily::BodyArmor => q.details.raw,
            };
            q.details.raw = value;
            q.value = value;
        }
        raw.weenie = source.clone();
        for q in &input.actor_qualities {
            if let Some(existing) = raw.qualities.iter_mut().find(|p| {
                p.entity == actor.0 && p.family == q.family && p.stat == q.stat && p.part == q.part
            }) {
                existing.details = q.details;
                existing.value = q.value;
            }
        }
        self.combat.validate_physical_refresh_source(actor, &raw)?;
        input.source = source;
        input.equipment = raw.equipment.clone();
        input.physical = raw.profile.clone();
        input.equipment_qualities = raw
            .qualities
            .iter()
            .filter(|q| q.entity != actor.0)
            .copied()
            .collect();
        Ok((input, raw))
    }
    /// Read-only preflight before durable NPC scalar publication. Noncombat
    /// objects retain their ordinary property validation without a fake caster.
    pub(super) fn validate_npc_scalar_properties(
        &self,
        actor: EntityId,
        properties: &bace_entity::EntityProperties,
    ) -> Result<(), E> {
        if matches!(properties.get(bace_entity::PropertyFamily::String, 1), Some(bace_entity::PropertyValue::String(name)) if name.len() > 1024)
        {
            return Err(E::InvalidInput);
        }
        let Some(existing) = self.npc_combat_assets.get(&actor) else {
            return self
                .magic
                .validate_object_properties(actor, properties)
                .map_err(magic_error);
        };
        let source =
            crate::npc_combat_assets::overlay_npc_source_qualities(&existing.source, properties)
                .map_err(|_| E::InvalidInput)?;
        let (input, raw) = self.prepare_npc_scalar_source(actor, Arc::new(source))?;
        let (values, attributes) = npc_values(&input, self.magic.registry(actor))?;
        let mut current = (**self.combat.physical_profile(actor).ok_or(E::MissingActor)?).clone();
        current.skills = values
            .iter()
            .map(|skill| {
                (
                    skill.skill,
                    PhysicalSkill {
                        advancement: skill.advancement as u32,
                        current: skill.current,
                    },
                )
            })
            .collect();
        current.strength = attributes[0];
        let mut qualities = raw.qualities.clone();
        for quality in &mut qualities {
            let registry = self
                .magic
                .registry(EntityId(quality.entity))
                .ok_or(E::MissingActor)?;
            use bace_combat::preparation::PhysicalQualityFamily as F;
            let (details, value) = match quality.family {
                F::Int => bace_magic::enchant_physical_quality(
                    registry,
                    4,
                    quality.stat,
                    quality.details.raw,
                    true,
                ),
                F::Float => bace_magic::enchant_physical_quality(
                    registry,
                    8,
                    quality.stat,
                    quality.details.raw,
                    false,
                ),
                F::BodyArmor => {
                    bace_magic::enchant_body_quality(registry, quality.stat, quality.details.raw)
                        .map(|q| (q, quality.details.raw))
                }
            }
            .map_err(|_| E::InvalidInput)?;
            quality.details = details;
            quality.value = value;
        }
        let physical =
            bace_combat::preparation::refresh_physical_from_source(&raw, &current, &qualities)
                .map_err(|_| E::InvalidInput)?;
        let equipment = raw
            .equipment
            .iter()
            .map(|item| {
                let level = self
                    .item_experience
                    .items
                    .get(&EntityId(item.entity))
                    .and_then(|p| p.experience)
                    .map(|xp| xp.level().map_err(|_| E::InvalidInput))
                    .transpose()?;
                Ok(bace_magic::MagicDamageEquipment {
                    entity: item.entity,
                    revision: item.revision,
                    location: item.location,
                    level,
                    weenie: &item.weenie,
                })
            })
            .collect::<Result<Vec<_>, E>>()?;
        bace_magic::prepare_magic_damage_profile(bace_magic::MagicDamagePreparation {
            player: false,
            weenie: &raw.weenie,
            equipment: &equipment,
            skills: &physical.skills,
            base_attributes: input.base_attributes,
            base_shield_skill: values.iter().find(|s| s.skill == 48).map_or(0, |s| s.base),
        })
        .map_err(|_| E::InvalidInput)?;
        Ok(())
    }
    /// Rollback/retirement must first drain real casts, resource receipts and
    /// pending output. An absent cache is an idempotent no-op for this owner.
    pub(crate) fn retire_npc_combat_assets(&mut self, actor: EntityId) -> Result<(), E> {
        if !self.npc_combat_assets.contains_key(&actor) && !self.magic.has_object_caster(actor) {
            return Ok(());
        }
        if !self.magic.can_retire_npc(actor) {
            return Err(E::Busy);
        }
        let now = self.tick as f64 / 30.;
        if self.magic.registry(actor).is_some() {
            if self.magic.registry_reserved(actor) {
                return Err(E::Busy);
            }
            self.magic
                .reserve_registry(actor, true, now)
                .map_err(|_| E::Busy)?;
            if self.magic.can_retire_npc_registry(actor, now).is_err() {
                let _ = self.magic.reserve_registry(actor, false, now);
                return Err(E::Busy);
            }
            self.magic
                .retire_npc_registry(actor, now)
                .map_err(|_| E::Busy)?;
        }
        self.magic.retire_npc(actor).map_err(magic_error)?;
        self.npc_combat_assets.remove(&actor);
        self.npc_combat_staging.remove(&actor);
        self.registry_revisions.remove(&actor);
        self.physical_refresh_dirty.remove(&actor);
        Ok(())
    }
    pub(crate) fn confirm_npc_combat_assets(&mut self, actor: EntityId) {
        self.npc_combat_staging.remove(&actor);
    }
    /// Only freshly staged registry identities are retired; preexisting foreign
    /// registry owners are preserved. Final publication clears this stage receipt.
    pub(crate) fn rollback_npc_combat_assets(&mut self, actor: EntityId) -> Result<(), E> {
        if !self.npc_combat_assets.contains_key(&actor) {
            return Ok(());
        }
        let ids = self
            .npc_combat_staging
            .get(&actor)
            .cloned()
            .ok_or(E::Busy)?;
        if !self.magic.can_retire_npc(actor)
            || self.combat.active(actor)
            || ids.iter().any(|id| self.magic.registry_reserved(*id))
        {
            return Err(E::Busy);
        }
        let now = self.tick as f64 / 30.;
        let mut held = Vec::new();
        for id in &ids {
            if self.magic.registry(*id).is_none() {
                continue;
            }
            if self.magic.reserve_registry(*id, true, now).is_err() {
                for id in held {
                    self.magic
                        .reserve_registry(id, false, now)
                        .map_err(|_| E::Busy)?;
                }
                return Err(E::Busy);
            }
            held.push(*id);
        }
        if held.iter().any(|id| {
            if *id == actor {
                self.magic.can_retire_npc_registry(*id, now).is_err()
            } else {
                self.magic.can_retire_item_registry(*id, now).is_err()
            }
        }) {
            for id in held {
                self.magic
                    .reserve_registry(id, false, now)
                    .map_err(|_| E::Busy)?;
            }
            return Err(E::Busy);
        }
        for id in held {
            if id == actor {
                self.magic
                    .retire_npc_registry(id, now)
                    .expect("NPC rollback registry preflight");
            } else {
                self.magic
                    .retire_item_registry(id, now)
                    .expect("NPC rollback item preflight");
            }
            self.registry_revisions.remove(&id);
        }
        self.magic
            .rollback_fresh_npc(actor)
            .expect("fresh NPC caster with drained output");
        self.npc_combat_staging.remove(&actor);
        self.npc_combat_assets.remove(&actor);
        self.physical_refresh_dirty.remove(&actor);
        Ok(())
    }
}
fn npc_values(
    input: &PreparedNpcCombatAssets,
    registry: Option<&bace_magic::EnchantmentRegistry>,
) -> Result<(Vec<bace_character::SkillValues>, [u32; 6]), E> {
    let prepared = super::skill_modifiers::compose(&input.skill_inputs, registry)?;
    let mut attributes = input.base_attributes;
    for (index, modifier) in prepared.attribute_modifiers.iter().enumerate() {
        attributes[index] = bace_character::project_attribute_value(
            input.base_attributes[index],
            modifier.multiplier,
            modifier.additive,
        )
        .map_err(|_| E::InvalidInput)?;
    }
    if attributes.iter().any(|v| *v > i32::MAX as u32) {
        return Err(E::InvalidInput);
    }
    let mut values = Vec::with_capacity(input.skills.len());
    let mut seen = std::collections::BTreeSet::new();
    for raw in &input.skills {
        if !seen.insert(raw.skill) {
            return Err(E::InvalidInput);
        }
        let mut formula = prepared
            .inputs
            .iter()
            .find(|(id, _)| *id == raw.skill)
            .ok_or(E::MissingSkill)?
            .1;
        formula.base_attributes = input.base_attributes;
        formula.current_attributes = attributes;
        let value = bace_character::project_skill_values(
            raw.skill,
            raw.advancement,
            raw.ranks,
            raw.initial_level,
            formula,
        )
        .map_err(|_| E::InvalidInput)?;
        if value.current > i32::MAX as u32 {
            return Err(E::InvalidInput);
        }
        values.push(value);
    }
    Ok((values, attributes))
}
fn magic_error(error: CastRejection) -> E {
    match error {
        CastRejection::Busy => E::Busy,
        CastRejection::Capacity => E::Capacity,
        _ => E::InvalidInput,
    }
}
