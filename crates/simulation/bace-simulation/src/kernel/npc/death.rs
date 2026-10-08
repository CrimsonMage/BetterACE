//! KillSelf's Smite(creature) changes health through the authoritative vital
//! owner. Population/player-death services retain corpse, reward and save work.
use super::Kernel;
use crate::{NpcEffect, NpcProposal};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
impl Kernel {
    pub fn apply_npc_kill_self(&mut self, expected: &NpcProposal) -> Result<u32, E> {
        self.npcs.validate_service(expected)?;
        if !matches!(expected.effect, NpcEffect::Service(NpcOperation::KillSelf)) {
            return Err(E::Unsupported);
        }
        let actor = expected.context.source;
        let pool = self
            .world
            .vital(actor, bace_entity::EntityVital::Health)
            .map_err(|_| E::MissingActor)?;
        let health = pool.current;
        let target_incarnation = self
            .world
            .combatant(actor)
            .ok_or(E::MissingActor)?
            .incarnation();
        if health > 0 {
            if !self.npcs.damage_room() {
                return Err(E::Capacity);
            }
            self.world
                .apply_vital_batch(
                    &[bace_entity::VitalMutation {
                        actor,
                        vital: bace_entity::EntityVital::Health,
                        before: health,
                        after: 0,
                    }],
                    Some(actor),
                )
                .map_err(|_| E::DurabilityPending)?;
            self.npcs
                .damage_events
                .push_back(crate::CombatEvent::Damage {
                    target_incarnation,
                    attacker: Some(actor),
                    target: actor,
                    amount: health,
                    current: 0,
                    maximum: pool.maximum,
                    killed: true,
                    death_blow: None,
                    revision: self
                        .world
                        .combatant(actor)
                        .ok_or(E::MissingActor)?
                        .revision(),
                });
        }
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.confirm_npc_committed(expected)?;
        Ok(health)
    }
}
