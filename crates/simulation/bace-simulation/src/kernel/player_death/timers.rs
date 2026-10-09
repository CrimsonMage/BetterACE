use super::*;
use crate::PlayerDeathEvent;
impl Kernel {
    pub(in crate::kernel) fn clear_death_protection_for_teleport(&mut self, actor: EntityId) {
        if let Some(state) = self.player_deaths.states.get_mut(&actor) {
            state.protection_elapsed = None;
        }
        self.synchronize_death_projection(actor);
    }

    pub(super) fn synchronize_death_projection(&mut self, actor: EntityId) {
        if let Some(state) = self.player_deaths.states.get(&actor)
            && let Some(c) = self.world.combatant_mut(actor)
        {
            c.set_lifestone_protected(state.protection_elapsed.is_some());
            c.set_death_pk_status(state.pk_status);
        }
    }
    pub fn dispel_lifestone_protection(&mut self, actor: EntityId) -> Result<(), E> {
        let Some(state) = self.player_deaths.states.get(&actor) else {
            return Ok(());
        };
        if state.protection_elapsed.is_none() {
            return Ok(());
        }
        if self.player_deaths.events.len() >= self.player_deaths.capacity
            || self.characters.reserved(actor)
        {
            return Err(E::Busy);
        }
        let recipient = self.characters.entered_binding(actor);
        self.characters
            .touch_auxiliary(actor)
            .map_err(|_| E::Invalid)?;
        self.player_deaths
            .states
            .get_mut(&actor)
            .expect("state checked")
            .protection_elapsed = None;
        self.synchronize_death_projection(actor);
        self.player_deaths
            .events
            .push_back(PlayerDeathEvent::ProtectionDispelled { actor, recipient });
        Ok(())
    }
    pub(super) fn step_player_death_timers(&mut self, now: u64) -> Result<(), SimulationError> {
        self.player_deaths.timer_scratch.clear();
        self.player_deaths.timer_scratch.extend(
            self.player_deaths
                .timers
                .iter()
                .filter_map(|(&id, &last)| (now.saturating_sub(last) >= 150).then_some(id)),
        );
        for i in 0..self.player_deaths.timer_scratch.len() {
            let actor = self.player_deaths.timer_scratch[i];
            if self.characters.reserved(actor) {
                continue;
            }
            if self.player_deaths.events.len() + 2 > self.player_deaths.capacity {
                break;
            }
            let mut state = self
                .player_deaths
                .states
                .get(&actor)
                .ok_or(SimulationError::AuxiliaryRevision)?
                .clone();
            let before = state.clone();
            if let Some(elapsed) = state.protection_elapsed {
                state.protection_elapsed = (elapsed + 5. < 60.).then_some(elapsed + 5.);
            }
            let safe = self.death_policy.safe_training_academy
                && self.world.properties(actor).is_some_and(|p| {
                    matches!(
                        p.get(bace_entity::PropertyFamily::Bool, 107),
                        Some(bace_entity::PropertyValue::Bool(true))
                    )
                });
            if let Some(elapsed) = state.pk_respite_elapsed
                && !safe
            {
                let level = self.death_int(actor, 99).unwrap_or(0);
                if level == 0 && !self.death_policy.pk_server && !self.death_policy.pkl_server {
                    state.pk_respite_elapsed = None;
                } else if elapsed + 5. >= f64::from(self.death_policy.pk_respite_seconds) {
                    state.pk_respite_elapsed = None;
                    state.pk_status = if self.death_policy.pk_server
                        || level == 1 && !self.death_policy.pkl_server
                    {
                        4
                    } else if self.death_policy.pkl_server || level == 2 {
                        64
                    } else {
                        state.pk_status
                    };
                } else {
                    state.pk_respite_elapsed = Some(elapsed + 5.);
                }
            }
            self.player_deaths.timers.insert(actor, now);
            if before != state {
                self.characters
                    .touch_auxiliary(actor)
                    .map_err(|_| SimulationError::AuxiliaryRevision)?;
                if before.protection_elapsed.is_some() && state.protection_elapsed.is_none() {
                    self.player_deaths
                        .events
                        .push_back(PlayerDeathEvent::ProtectionExpired {
                            actor,
                            recipient: self.characters.entered_binding(actor),
                        });
                }
                if before.pk_status != state.pk_status {
                    self.player_deaths
                        .events
                        .push_back(PlayerDeathEvent::PkStatus {
                            actor,
                            status: state.pk_status,
                            recipient: self.characters.entered_binding(actor),
                        });
                }
                self.player_deaths.states.insert(actor, state);
                self.synchronize_death_projection(actor);
            }
        }
        Ok(())
    }
}
