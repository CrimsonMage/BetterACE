//! Pet release saves are retained through receipt adoption and logout retry.
mod output;
mod preparation;
#[cfg(test)]
mod tests;
mod use_action;
use super::*;
use crate::{
    pet_saves::PetOperationId,
    pet_service::{PetCompletion, PetService, PetWork},
};
use bace_simulation::{PetEvent, PetOutcome};
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};

pub(super) struct PetRuntime {
    pub(super) service: PetService,
    deferred: Option<PetEvent>,
    pub(super) publications: VecDeque<PetEvent>,
    completions: VecDeque<PetCompletion>,
    unexpected: Option<PetOutcome>,
    use_pending: Option<use_action::PendingPetUse>,
    visible: std::collections::BTreeSet<EntityId>,
}

impl PetRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: PetService::new(),
            deferred: None,
            publications: VecDeque::new(),
            completions: VecDeque::new(),
            unexpected: None,
            use_pending: None,
            visible: Default::default(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.service.requires_drain()
            || self.deferred.is_some()
            || !self.publications.is_empty()
            || !self.completions.is_empty()
            || self.unexpected.is_some()
            || self.use_pending.is_some()
    }
}

impl GameRuntime {
    pub(super) fn poll_pets(&mut self) -> Result<(), String> {
        if self.pets.unexpected.is_some() {
            return Err("unmatched pet outcome retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.pet_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.pets.service.accept_outcome(outcome)
                && let Err(outcome) = self.pets.accept_use_outcome(*outcome)
            {
                self.pets.unexpected = Some(*outcome);
                return Err("unmatched pet outcome retained".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            if self.pets.deferred.is_some() {
                break;
            }
            let Ok(event) = self.simulation.pet_events().try_recv() else {
                break;
            };
            if self.pets.publications.len() >= self.limits.messages {
                self.pets.deferred = Some(event);
                break;
            }
            self.accept_pet_event(event)?;
        }
        if let Some(event) = self.pets.deferred.take() {
            if self.pets.service.requires_drain()
                || self.pets.publications.len() >= self.limits.messages
            {
                self.pets.deferred = Some(event);
            } else {
                self.accept_pet_event(event)?;
            }
        }
        let correlation = self.token()?;
        self.pets.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        )?;
        if self.pets.completions.len() < self.limits.messages
            && let Some(completion) = self.pets.service.take_completion()
        {
            self.pets.completions.push_back(completion);
        }
        self.poll_pet_use()?;
        self.project_pet_output()?;
        Ok(())
    }

    fn accept_pet_event(&mut self, event: PetEvent) -> Result<(), String> {
        let PetEvent::ReleaseProposed {
            ticket,
            actor_revision,
        } = event
        else {
            self.pets.publications.push_back(event);
            return Ok(());
        };
        if self.pets.service.requires_drain() {
            self.pets.deferred = Some(PetEvent::ReleaseProposed {
                ticket,
                actor_revision,
            });
            return Ok(());
        }
        let Some(binding) = self.sessions.keys().find_map(|key| {
            self.players
                .binding_for(*key)
                .filter(|binding| binding.actor == ticket.actor)
        }) else {
            self.pets.deferred = Some(PetEvent::ReleaseProposed {
                ticket,
                actor_revision,
            });
            return Err("pet release owner binding unavailable".into());
        };
        let mut bytes = [0; 16];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|e| e.to_string())?;
        let identity = PetOperationId::new(bytes)?;
        let work = PetWork {
            binding,
            ticket,
            actor_revision,
            registry_after: None,
            summon: false,
            identity,
        };
        if let Err(work) = self.pets.service.stage(work) {
            self.pets.deferred = Some(PetEvent::ReleaseProposed {
                ticket: work.ticket,
                actor_revision: work.actor_revision,
            });
        }
        Ok(())
    }
}
