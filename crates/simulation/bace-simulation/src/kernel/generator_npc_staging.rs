//! Newly created physical NPCs remain rollbackable until their batch receipt.
use super::*;
use crate::generators::*;
use bace_gameplay_api::generators::*;
impl Kernel {
    pub(super) fn stage_generator_npc(
        &mut self,
        intent: &GeneratorSpawnIntent,
        entity: EntityId,
        mut template: GeneratedNpcTemplate,
        ordinal: u32,
    ) -> Result<GeneratorLocation, GeneratorServiceError> {
        if template.loot.is_some() == template.ace_loot.is_some() {
            return Err(GeneratorServiceError::Missing);
        }
        if template.physical.is_some() {
            self.world
                .validate_locomotion_styles(entity, &template.locomotion_styles)
                .map_err(|_| GeneratorServiceError::Capacity)?;
            self.combat
                .validate_physical_motions(entity, &template.physical_motions)
                .map_err(|_| GeneratorServiceError::Invalid)?;
        }
        if !template.death_motions.is_empty() {
            self.world
                .validate_death_motions(entity, &template.death_motions)
                .map_err(|_| GeneratorServiceError::Invalid)?;
        }
        if let Some(assets) = &template.combat_assets {
            if !template.requires_equipment && !assets.registry_items.is_empty() {
                return Err(GeneratorServiceError::Invalid);
            }
            if template
                .physical
                .as_ref()
                .is_none_or(|p| **p != *assets.physical)
            {
                return Err(GeneratorServiceError::Invalid);
            }
            self.preflight_npc_combat_assets(&[(entity, assets.clone())])
                .map_err(|_| GeneratorServiceError::Invalid)?;
        }
        self.preflight_npc_scripts(
            template
                .script
                .as_ref()
                .map(|script| (entity, script))
                .into_iter(),
        )
        .map_err(|_| GeneratorServiceError::Invalid)?;
        let mut loot_event = intent.random_identity;
        if ordinal > 0 {
            let mut random = bace_spawning::generator_event_stream(
                self.generators
                    .root
                    .as_ref()
                    .ok_or(GeneratorServiceError::Missing)?,
                intent,
                ordinal
                    .checked_add(0x40000)
                    .ok_or(GeneratorServiceError::Invalid)?,
            )?;
            loot_event[..8].copy_from_slice(
                &random
                    .next_u64()
                    .map_err(|_| GeneratorServiceError::Invalid)?
                    .to_le_bytes(),
            );
            loot_event[8..].copy_from_slice(
                &random
                    .next_u64()
                    .map_err(|_| GeneratorServiceError::Invalid)?
                    .to_le_bytes(),
            );
        }
        for location in self.generator_placement_candidates(intent, ordinal)? {
            let q = location.rotation;
            let norm = q.iter().map(|v| v * v).sum::<f32>();
            if !norm.is_finite()
                || !(0.999..=1.001).contains(&norm)
                || q[0].abs() > 0.0002
                || q[1].abs() > 0.0002
            {
                return Err(GeneratorServiceError::Geometry);
            }
            template.geometry.heading = 2.0 * q[2].atan2(q[3]);
            template.blueprint.cell = bace_types::CellId(location.cell);
            template.blueprint.position = bace_geometry::Vec3::new(
                location.origin[0],
                location.origin[1],
                location.origin[2],
            );
            let origin = crate::pve::GeneratedNpcOrigin {
                generator: intent.key.generator.entity,
                incarnation: intent.key.generator.incarnation,
                content_revision: intent.key.generator.content_revision,
                profile: intent.key.profile_id,
                child_incarnation: intent.key.occurrence,
            };
            match self.population.spawn_generated(
                entity,
                template.blueprint.clone(),
                crate::pve::PreparedNpcPhysical {
                    geometry: template.geometry.clone(),
                    resources: template.resources,
                },
                origin,
                &mut self.world,
                self.tick,
            ) {
                Ok(()) => {
                    if !template.death_motions.is_empty() {
                        self.world
                            .register_death_motions(entity, template.death_motions.clone())
                            .expect("preflighted death program slots");
                    }
                    let profile_failed = template.physical.as_ref().is_some_and(|profile| {
                        self.combat
                            .register_physical(entity, profile.clone(), &self.world)
                            .is_err()
                            || self
                                .world
                                .register_locomotion_styles(
                                    entity,
                                    template.locomotion_styles.clone(),
                                )
                                .is_err()
                            || template
                                .physical_motions
                                .iter()
                                .any(|(motion, speed, chain)| {
                                    self.combat
                                        .register_physical_motion(
                                            entity,
                                            *motion,
                                            *speed,
                                            chain.clone(),
                                        )
                                        .is_err()
                                })
                    });
                    if !profile_failed && let Some(assets) = &template.combat_assets {
                        self.adopt_npc_combat_assets(entity, assets.clone())
                            .expect("same-owner NPC combat join preflight");
                    }
                    let loot_failed = !profile_failed
                        && if let Some(loot) = &template.loot {
                            self.population
                                .native_loot(entity, loot.clone(), loot_event)
                                .is_err()
                        } else if let Some(policy) = &template.ace_loot {
                            self.population
                                .ace_loot(entity, policy.clone(), loot_event)
                                .is_err()
                        } else {
                            true
                        };
                    if profile_failed || loot_failed {
                        self.rollback_npc_combat_assets(entity)
                            .map_err(|_| GeneratorServiceError::Busy)?;
                        self.combat.retire_actor(entity);
                        self.population
                            .remove_generated(entity, origin, &mut self.world)
                            .map_err(|_| GeneratorServiceError::Invalid)?;
                        return Err(if profile_failed {
                            GeneratorServiceError::Capacity
                        } else {
                            GeneratorServiceError::Invalid
                        });
                    }
                    if let Some(script) = template.script.clone() {
                        self.admit_npc_script(entity, script)
                            .expect("same-owner NPC script preflight");
                    }
                    return Ok(location);
                }
                Err(crate::pve::PveError::MissingGeometry) => continue,
                Err(crate::pve::PveError::Capacity) => return Err(GeneratorServiceError::Capacity),
                Err(_) => return Err(GeneratorServiceError::Invalid),
            }
        }
        Err(GeneratorServiceError::Placement)
    }
}
