//! Same-transaction proficiency supplement; no independent XP award or RNG draw.
use super::Kernel;
use bace_crafting::{CraftError as E, PreparedRecipe};
use bace_entity::{EntityVital, VitalMutation};
use bace_types::EntityId;
impl Kernel {
    pub(super) fn stage_crafting_proficiency(
        &mut self,
        operation: u64,
        recipe: &PreparedRecipe,
    ) -> Result<(), E> {
        let ticket = self.crafting.pending(operation).ok_or(E::InvalidState)?;
        let actor = ticket.actor;
        let crate::CraftingDecision::Tinker(decision) = &ticket.decision else {
            return Ok(());
        };
        self.characters
            .reserve_crafting(actor, operation)
            .map_err(|_| E::Busy)?;
        let Some((skill, difficulty)) = recipe.proficiency.filter(|_| decision.success) else {
            return Ok(());
        };
        let table = self
            .allegiances
            .level_table
            .as_ref()
            .ok_or(E::InvalidState)?;
        let services = self
            .characters
            .native_services(actor)
            .ok_or(E::InvalidState)?;
        let usage = bace_character::ProficiencyUse {
            skill,
            difficulty,
            unix_time: self.social.epoch as f64 + self.tick as f64 / 30.0,
            olthoi: self
                .social
                .directory
                .presence(actor)
                .ok_or(E::InvalidState)?
                .olthoi,
        };
        let Some(change) = self
            .characters
            .get(actor)
            .ok_or(E::InvalidState)?
            .propose_proficiency(
                services,
                table,
                usage,
                decision.actor_revision != decision.expected_actor_revision,
            )
            .map_err(|_| E::InvalidState)?
        else {
            return Ok(());
        };
        self.characters
            .get(actor)
            .ok_or(E::InvalidState)?
            .inspect_proficiency(services, table, &change, |proposed| {
                self.validate_proposed_skill_refresh(actor, proposed)
            })
            .map_err(|_| E::InvalidState)?
            .map_err(|_| E::InvalidState)?;
        let mut vitals = Vec::new();
        if change.earned.services.after.level > change.earned.services.before.level {
            for vital in [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana] {
                let pool = self
                    .world
                    .vital(actor, vital)
                    .map_err(|_| E::InvalidState)?;
                vitals.push(VitalMutation {
                    actor,
                    vital,
                    before: pool.current,
                    after: pool.maximum,
                });
            }
        }
        let vitae = if change.grants_experience
            && self.magic.registry(actor).is_some_and(|r| {
                r.entries()
                    .iter()
                    .any(|e| e.spell == 666 && e.spec.value < 1.0)
            }) {
            self.prepare_player_vitae_recovery(actor, change.earned.credited)
                .map_err(|_| E::InvalidState)?
        } else {
            None
        };
        if !vitals.is_empty() {
            self.world
                .reserve_vitals(
                    &vitals
                        .iter()
                        .map(|v| (v.actor, v.vital))
                        .collect::<Vec<_>>(),
                    token(operation),
                )
                .map_err(|_| E::Busy)?;
        }
        let character = self.characters.get(actor).ok_or(E::InvalidState)?;
        let skill_base = if change.after.ranks != change.before.ranks {
            let profile = self.combat.skills.get(&actor).ok_or(E::InvalidState)?;
            let values = super::skill_refresh::project_skills(
                character,
                &profile.prepared,
                self.magic.registry(actor),
            )
            .map_err(|_| E::InvalidState)?;
            Some(
                values
                    .iter()
                    .find(|v| v.skill == skill)
                    .ok_or(E::InvalidState)?
                    .base
                    .checked_add(u32::from(change.after.ranks - change.before.ranks))
                    .ok_or(E::Overflow)?,
            )
        } else {
            None
        };
        let skill_maximum = character.maximum_rank(change.after.target) == Some(change.after.ranks);
        let experience = change.grants_experience.then(|| {
            use bace_gameplay_api::experience::{ExperienceEvent, ExperienceState};
            let c = &change.earned;
            ExperienceEvent {
                actor,
                before: ExperienceState {
                    total: c.services.before.total_experience,
                    available: c.experience.before_available - u64::from(change.spent),
                    level: c.services.before.level,
                    available_skill_credits: c.before_skill_credits.unwrap_or(0),
                },
                after: ExperienceState {
                    total: c.services.after.total_experience,
                    available: c.experience.after_available,
                    level: c.services.after.level,
                    available_skill_credits: c.after_skill_credits.unwrap_or(0),
                },
                update_properties: true,
                maximum_level: table.maximum_level(),
                quest_amount: None,
                next_credit_level: (c.services.after.level > c.services.before.level
                    && c.earned_skill_credits == 0
                    && c.services.after.level < table.maximum_level())
                .then(|| table.next_skill_credit_level(c.services.after.level)),
                vitals: vitals
                    .iter()
                    .map(|v| {
                        (
                            match v.vital {
                                EntityVital::Health => 2,
                                EntityVital::Stamina => 4,
                                EntityVital::Mana => 6,
                            },
                            v.after,
                        )
                    })
                    .collect(),
            }
        });
        let patch = crate::CraftingProficiency {
            change,
            vitals,
            vitae,
            skill_base,
            skill_maximum,
            experience,
        };
        self.crafting.attach_proficiency(operation, patch)?;
        self.validate_crafting_proficiency(operation)
    }
    pub(super) fn validate_crafting_proficiency(&self, operation: u64) -> Result<(), E> {
        let ticket = self.crafting.pending(operation).ok_or(E::InvalidState)?;
        if matches!(ticket.decision, crate::CraftingDecision::Tinker(_))
            && self.characters.crafting_operation(ticket.actor) != Some(operation)
        {
            return Err(E::InvalidState);
        }
        let Some(p) = &ticket.proficiency else {
            return Ok(());
        };
        self.characters
            .get(ticket.actor)
            .ok_or(E::InvalidState)?
            .validate_proficiency(
                self.characters
                    .native_services(ticket.actor)
                    .ok_or(E::InvalidState)?,
                self.allegiances
                    .level_table
                    .as_ref()
                    .ok_or(E::InvalidState)?,
                &p.change,
            )
            .map_err(|_| E::InvalidState)?;
        if !p.vitals.is_empty() {
            self.world
                .validate_vital_batch_reserved(&p.vitals, None, token(operation))
                .map_err(|_| E::InvalidState)?;
        }
        if let Some(vitae) = &p.vitae {
            self.validate_player_vitae_recovery(vitae)
                .map_err(|_| E::Capacity)?;
        }
        Ok(())
    }
    pub(super) fn adopt_crafting_proficiency(
        &mut self,
        ticket: &crate::CraftingTicket,
    ) -> Result<(), E> {
        let Some(p) = &ticket.proficiency else {
            return Ok(());
        };
        self.characters
            .adopt_crafting_proficiency(
                ticket.actor,
                ticket.operation,
                &p.change,
                self.allegiances
                    .level_table
                    .as_ref()
                    .ok_or(E::InvalidState)?,
            )
            .map_err(|_| E::InvalidState)?;
        if let Some(vitae) = &p.vitae {
            self.adopt_player_vitae_recovery(vitae.clone())
                .map_err(|_| E::InvalidState)?;
        }
        if !p.vitals.is_empty() {
            self.world
                .apply_vital_batch_reserved(&p.vitals, None, token(ticket.operation))
                .map_err(|_| E::InvalidState)?;
            // These exact current-vital values were frozen with the skill award.
            // Record only them, preserving any independent pose/timer dirty state.
            for vital in &p.vitals {
                if let Some(seen) = self.player_world_seen.get_mut(&vital.actor) {
                    let index = match vital.vital {
                        bace_entity::EntityVital::Health => 0,
                        bace_entity::EntityVital::Stamina => 1,
                        bace_entity::EntityVital::Mana => 2,
                    };
                    if let Some(pool) = &mut seen.vitals[index] {
                        pool.current = vital.after;
                    }
                }
            }
        }
        self.world.release_vitals(token(ticket.operation));
        self.refresh_character_skills(ticket.actor)
            .map_err(|_| E::InvalidState)?;
        Ok(())
    }
    pub(super) fn release_crafting_proficiency(
        &mut self,
        actor: EntityId,
        operation: u64,
    ) -> Result<(), E> {
        self.characters
            .release_crafting(actor, operation)
            .map_err(|_| E::InvalidState)?;
        self.world.release_vitals(token(operation));
        Ok(())
    }
}
fn token(operation: u64) -> bace_world::VitalReservationToken {
    bace_world::VitalReservationToken {
        domain: bace_world::VitalReservationDomain::Crafting,
        operation,
    }
}
