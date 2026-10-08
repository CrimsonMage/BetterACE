//! Item XP runs before the queued player Vitae stage, using one immutable candidate chain.
use super::Kernel;
use bace_gameplay_api::social::SocialError as E;
use bace_types::EntityId;
use std::collections::BTreeMap;
impl Kernel {
    pub(super) fn attach_experience_supplements(
        &mut self,
        mut ticket: crate::AllegianceTicket,
        items: &BTreeMap<EntityId, u64>,
        normal: &BTreeMap<EntityId, u64>,
    ) -> Result<crate::AllegianceTicket, E> {
        let result = (|| {
            for (&actor, &amount) in items {
                let reward = self
                    .prepare_item_experience(actor, amount)
                    .map_err(|_| E::Invalid)?;
                let reserved = self
                    .reserve_item_experience(&reward, &self.allegiances.registries.clone())
                    .map_err(|_| E::Busy)?;
                ticket.item_experience.push((reward, reserved));
            }
            for (&actor, &amount) in normal {
                if self.magic.registry(actor).is_some_and(|r| {
                    r.entries()
                        .iter()
                        .any(|e| e.spell == 666 && e.spec.value < 1.)
                }) {
                    let candidate = ticket
                        .item_experience
                        .iter()
                        .flat_map(|(r, _)| &r.registries)
                        .find(|p| p.actor == actor);
                    if let Some(patch) = self
                        .prepare_player_vitae_recovery_after_item_xp(actor, amount, candidate)
                        .map_err(|_| E::Invalid)?
                    {
                        ticket.vitae.push(patch);
                    }
                }
            }
            for (actor, change) in &ticket.player_changes {
                if change.services.after.level > change.services.before.level {
                    for vital in [
                        bace_entity::EntityVital::Health,
                        bace_entity::EntityVital::Stamina,
                        bace_entity::EntityVital::Mana,
                    ] {
                        let pool = self.world.vital(*actor, vital).map_err(|_| E::Missing)?;
                        ticket.vitals.push(bace_entity::VitalMutation {
                            actor: *actor,
                            vital,
                            before: pool.current,
                            after: pool.maximum,
                        });
                    }
                }
            }
            if !ticket.vitals.is_empty() {
                let resources = ticket
                    .vitals
                    .iter()
                    .map(|v| (v.actor, v.vital))
                    .collect::<Vec<_>>();
                self.world
                    .reserve_vitals(&resources, experience_vital_token(ticket.operation))
                    .map_err(|_| E::Busy)?;
            }
            for (actor, _) in &ticket.player_changes {
                ticket.player_revision(*actor).ok_or(E::Overflow)?;
            }
            self.validate_item_experience_events_batch(
                ticket.item_experience.iter().map(|(reward, _)| reward),
            )
            .map_err(|_| E::Capacity)?;
            let patches = ticket
                .item_experience
                .iter()
                .flat_map(|(r, _)| r.registries.clone())
                .collect::<Vec<_>>();
            self.validate_player_vitae_recoveries_after_item_xp(&ticket.vitae, &patches)
                .map_err(|_| E::Capacity)?;
            Ok(())
        })();
        if let Err(error) = result {
            self.world
                .release_vitals(experience_vital_token(ticket.operation));
            for (_, reservation) in &ticket.item_experience {
                self.reject_item_experience(reservation.as_ref())
                    .map_err(|_| E::Stale)?;
            }
            self.characters
                .reject_social_rewards(ticket.operation, &ticket.player_changes)
                .map_err(|_| E::Stale)?;
            self.allegiances.pending = None;
            return Err(error);
        }
        self.allegiances.pending = Some(ticket.clone());
        Ok(ticket)
    }
}

pub(super) fn experience_vital_token(operation: u64) -> bace_world::VitalReservationToken {
    bace_world::VitalReservationToken {
        domain: bace_world::VitalReservationDomain::Experience,
        operation,
    }
}
