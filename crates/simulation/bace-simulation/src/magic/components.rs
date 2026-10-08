//! Cast/inventory correlation stays with the simulation owner across worker retries.
use super::*;
pub(crate) struct PendingComponents {
    pub actor: EntityId,
    pub cast: u64,
    pub required: Vec<(u32, u32)>,
    pub consumed: Vec<(u32, u32)>,
}
impl Magic {
    pub(crate) fn pending_components(&self) -> Option<PendingComponents> {
        self.attempts.iter().find_map(|(&actor, attempt)| {
            if !attempt.component_request_sent
                || attempt.components_confirmed
                || self
                    .component_operations
                    .values()
                    .any(|binding| binding.actor == actor && binding.cast == attempt.cast)
            {
                return None;
            }
            Some(PendingComponents {
                actor,
                cast: attempt.cast,
                required: attempt.prepared.components.clone(),
                consumed: attempt.resources.as_ref()?.consumed.clone(),
            })
        })
    }
    pub(crate) fn bind_component_operation(
        &mut self,
        operation: u64,
        mut resources: MagicResourceCommit,
    ) {
        resources.operation = operation;
        self.component_operations.insert(operation, resources);
    }
    pub(crate) fn finish_component_operation(&mut self, operation: u64, success: bool) {
        if let Some(binding) = self.component_operations.remove(&operation)
            && let Some(attempt) = self
                .attempts
                .get_mut(&binding.actor)
                .filter(|a| a.cast == binding.cast)
        {
            attempt.components_confirmed = true;
            attempt.component_failure = (!success).then_some(CastRejection::MissingComponents);
        }
    }
}

impl Magic {
    pub(crate) fn register_components(
        &mut self,
        actor: EntityId,
        spell: u32,
        required: Vec<(u32, u32)>,
        modifiers: Vec<(u32, f32)>,
        loss: f32,
    ) -> Result<(), CastRejection> {
        if self.busy(actor) {
            return Err(CastRejection::Busy);
        }
        if !self.casters.contains_key(&actor) {
            return Err(CastRejection::MissingActor);
        }
        if self.actor_components.len() >= 65536
            && !self.actor_components.contains_key(&(actor, spell))
        {
            return Err(CastRejection::Capacity);
        }
        if required.len() > 64
            || required.len() != modifiers.len()
            || !loss.is_finite()
            || loss < 0.0
            || required
                .iter()
                .zip(&modifiers)
                .any(|((id, count), (other, modifier))| {
                    *id == 0
                        || *id != *other
                        || *count == 0
                        || *count > 256
                        || !modifier.is_finite()
                        || *modifier < 0.0
                })
            || required.iter().map(|(_, n)| u64::from(*n)).sum::<u64>() > 256
        {
            return Err(CastRejection::InvalidState);
        }
        let mut prepared = self
            .actor_components
            .get(&(actor, spell))
            .or_else(|| self.spells.get(&spell))
            .ok_or(CastRejection::UnknownSpell)?
            .as_ref()
            .clone();
        prepared.components = required;
        prepared.component_modifiers = modifiers;
        prepared.component_loss = loss;
        self.actor_components
            .insert((actor, spell), Arc::new(prepared));
        Ok(())
    }
}

/// Immutable companion to an inventory ticket. Mana is persisted with burned
/// components; recovery is the pre-effect checkpoint, not a fabricated future success.
#[derive(Clone, Debug, PartialEq)]
pub struct MagicResourceCommit {
    pub operation: u64,
    pub actor: EntityId,
    pub cast: u64,
    pub mana: VitalMutation,
    pub recovery: bace_magic::CastRecovery,
    pub prepared_at: f64,
    pub before_revision: u64,
    pub after_revision: u64,
}
impl Magic {
    pub(crate) fn prepare_component_resources(
        &self,
        actor: EntityId,
        cast: u64,
        world: &World,
        now: f64,
    ) -> Result<MagicResourceCommit, CastRejection> {
        if self.events.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        let attempt = self
            .attempts
            .get(&actor)
            .filter(|a| a.cast == cast && a.component_request_sent && !a.components_confirmed)
            .ok_or(CastRejection::InvalidState)?;
        let cost = attempt
            .resources
            .as_ref()
            .ok_or(CastRejection::InvalidState)?
            .cost;
        let before = world
            .vital(actor, EntityVital::Mana)
            .map_err(|_| CastRejection::MissingActor)?
            .current;
        let mana = VitalMutation {
            actor,
            vital: EntityVital::Mana,
            before,
            after: before
                .checked_sub(cost)
                .ok_or(CastRejection::InsufficientMana)?,
        };
        world
            .validate_vital_batch(&[mana], None)
            .map_err(|_| CastRejection::InvalidState)?;
        let recovery = self
            .recovery
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?
            .snapshot(now)
            .map_err(rejection)?;
        Ok(MagicResourceCommit {
            operation: 0,
            actor,
            cast,
            mana,
            recovery,
            prepared_at: now,
            before_revision: 0,
            after_revision: 0,
        })
    }
    pub(crate) fn rebase_component_snapshot(
        &mut self,
        operation: u64,
        revision: u64,
        now: f64,
    ) -> Result<(), CastRejection> {
        let commit = self
            .component_operations
            .get(&operation)
            .ok_or(CastRejection::InvalidState)?;
        if revision < commit.before_revision {
            return Err(CastRejection::InvalidState);
        }
        let after = revision.checked_add(1).ok_or(CastRejection::InvalidState)?;
        let recovery = self
            .recovery
            .get(&commit.actor)
            .ok_or(CastRejection::MissingActor)?
            .snapshot(now)
            .map_err(rejection)?;
        let commit = self
            .component_operations
            .get_mut(&operation)
            .expect("checked component owner");
        commit.before_revision = revision;
        commit.after_revision = after;
        commit.recovery = recovery;
        commit.prepared_at = now;
        Ok(())
    }
    pub(crate) fn component_operation(&self, actor: EntityId, cast: u64) -> Option<u64> {
        self.component_operations
            .iter()
            .find_map(|(&operation, r)| (r.actor == actor && r.cast == cast).then_some(operation))
    }
    pub(crate) fn component_resources(&self, operation: u64) -> Option<&MagicResourceCommit> {
        self.component_operations.get(&operation)
    }
    pub(crate) fn fail_component_request(
        &mut self,
        actor: EntityId,
        cast: u64,
        reason: CastRejection,
    ) {
        if let Some(attempt) = self.attempts.get_mut(&actor).filter(|a| a.cast == cast) {
            attempt.components_confirmed = true;
            attempt.component_failure = Some(reason);
        }
    }
    pub(crate) fn validate_component_resources(
        &self,
        operation: u64,
        world: &World,
    ) -> Result<(), CastRejection> {
        let Some(commit) = self.component_operations.get(&operation) else {
            return Ok(());
        };
        if self.events.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        if self
            .attempts
            .get(&commit.actor)
            .is_none_or(|a| a.cast != commit.cast || a.mana_applied)
        {
            return Err(CastRejection::InvalidState);
        }
        world
            .validate_vital_batch_reserved(
                &[commit.mana],
                None,
                bace_world::VitalReservationToken {
                    domain: bace_world::VitalReservationDomain::SpellComponents,
                    operation,
                },
            )
            .map_err(|_| CastRejection::InvalidState)
    }
    pub(crate) fn adopt_component_resources(&mut self, operation: u64, world: &mut World) {
        if let Some(commit) = self.component_operations.get(&operation) {
            let mutation = commit.mana;
            let values = world
                .apply_vital_batch_reserved(
                    &[mutation],
                    None,
                    bace_world::VitalReservationToken {
                        domain: bace_world::VitalReservationDomain::SpellComponents,
                        operation,
                    },
                )
                .expect("preflighted exact component mana");
            self.attempts
                .get_mut(&commit.actor)
                .expect("reserved component cast")
                .mana_applied = true;
            if mutation.before != mutation.after {
                self.events.push_back(MagicEvent::Vital {
                    incarnation: world
                        .combatant(mutation.actor)
                        .map_or(0, |c| c.incarnation()),
                    actor: mutation.actor,
                    vital: mutation.vital,
                    before: mutation.before,
                    after: mutation.after,
                    revision: values[0].revision,
                });
            }
        }
    }
}

impl Magic {
    pub(crate) fn register_gestures(
        &mut self,
        actor: EntityId,
        spell: u32,
        gestures: Vec<PreparedCastGesture>,
    ) -> Result<(), CastRejection> {
        if self.busy(actor) {
            return Err(CastRejection::Busy);
        }
        if !self.casters.contains_key(&actor) || gestures.is_empty() || gestures.len() > 32 {
            return Err(CastRejection::InvalidState);
        }
        if gestures.iter().any(|g| {
            g.motion_chain
                .as_ref()
                .is_none_or(|chain| chain.motion != g.gesture.motion || chain.speed != 2.0)
                || !g.gesture.minimum_seconds.is_finite()
                || !(0.0..=8.0).contains(&g.gesture.minimum_seconds)
        }) {
            return Err(CastRejection::MissingAssets);
        }
        if self.actor_components.len() >= 65536
            && !self.actor_components.contains_key(&(actor, spell))
        {
            return Err(CastRejection::Capacity);
        }
        let mut prepared = self
            .actor_components
            .get(&(actor, spell))
            .or_else(|| self.spells.get(&spell))
            .ok_or(CastRejection::UnknownSpell)?
            .as_ref()
            .clone();
        prepared.gestures = gestures;
        self.actor_components
            .insert((actor, spell), Arc::new(prepared));
        Ok(())
    }
}
