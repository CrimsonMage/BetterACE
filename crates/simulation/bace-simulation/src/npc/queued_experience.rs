//! Source GrantXP queues a recipient action; immediate outer emote rows run
//! before that action. Admission and recipient adoption are separate stages.
use super::*;
impl Npcs {
    pub(crate) fn preview_experience_admission(
        &self,
        expected: &NpcProposal,
        logical_now: f64,
        tick: u64,
    ) -> Result<NpcSourceCheckpoint, NpcFailure> {
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected
            || pending.detached
            || pending.adopted
            || !matches!(
                expected.effect,
                NpcEffect::QueuedExperience {
                    phase: NpcQueuedExperiencePhase::AwaitingAdmission,
                    ..
                }
            )
        {
            return Err(NpcFailure::Conflict);
        }
        let source = self
            .sources
            .get(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        let mut checkpoint = self.source_checkpoint(expected.context.source, tick)?;
        checkpoint.vm = source
            .manager
            .preview_detachment_no_delay(expected.ticket, logical_now)
            .map_err(|_| NpcFailure::Conflict)?;
        checkpoint.logical_now = logical_now;
        let pending = checkpoint
            .pending
            .iter_mut()
            .find(|p| p.proposal.ticket == expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if let NpcEffect::QueuedExperience { phase, .. } = &mut pending.proposal.effect {
            *phase = NpcQueuedExperiencePhase::Ready;
        }
        pending.detached = true;
        Ok(checkpoint)
    }
    pub(crate) fn admit_queued_experience(
        &mut self,
        expected: &NpcProposal,
        tick: u64,
    ) -> Result<(), NpcFailure> {
        let pending = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if pending.proposal != *expected
            || pending.adopted
            || pending.detached
            || !matches!(
                expected.effect,
                NpcEffect::QueuedExperience {
                    phase: NpcQueuedExperiencePhase::AwaitingAdmission,
                    ..
                }
            )
        {
            return Err(NpcFailure::Conflict);
        }
        self.release_journal_hold(expected, tick)?;
        let source = self
            .sources
            .get_mut(&expected.context.source)
            .ok_or(NpcFailure::MissingActor)?;
        if !source.recovery_ready {
            return Err(NpcFailure::DurabilityPending);
        }
        source
            .manager
            .detach_pending_no_delay(expected.ticket, tick as f64 / 30.0 + source.clock_offset)
            .map_err(|_| NpcFailure::Conflict)?;
        let pending = self
            .pending
            .get_mut(&expected.ticket)
            .expect("retained pending");
        if let NpcEffect::QueuedExperience { phase, .. } = &mut pending.proposal.effect {
            *phase = NpcQueuedExperiencePhase::Ready;
        }
        pending.detached = true;
        self.proposals.retain(|p| p.ticket != expected.ticket);
        Ok(())
    }
    pub(super) fn prepare_queued_experience(
        &mut self,
        world: &World,
        characters: &Characters,
        services: &dyn NpcServiceView,
        errors: &mut Vec<(EntityId, NativeError)>,
    ) {
        if self.shared_experience {
            return;
        }
        self.resumed.clear();
        self.resumed.extend(
            self.pending
                .iter()
                .filter(|(_, p)| {
                    p.detached
                        && self
                            .sources
                            .get(&p.proposal.context.source)
                            .is_some_and(|s| s.journal_hold.is_none())
                        && !p.adopted
                        && matches!(
                            p.proposal.effect,
                            NpcEffect::QueuedExperience {
                                phase: NpcQueuedExperiencePhase::Ready,
                                ..
                            }
                        )
                })
                .map(|(id, _)| *id),
        );
        for index in 0..self.resumed.len() {
            if self.proposals.len() == self.capacity {
                break;
            }
            let ticket = self.resumed[index];
            let pending = &self.pending[&ticket];
            let context = pending.proposal.context;
            let Some(source) = self.sources.get(&context.source) else {
                continue;
            };
            if !source.recovery_ready || source.manager.has_immediate_invocation(context.operation)
            {
                continue;
            }
            let NpcEffect::QueuedExperience { actor, amount, .. } = pending.proposal.effect else {
                continue;
            };
            if self.reserved(actor) || characters.reserved(actor) || services.reserved(actor) {
                continue;
            }
            let result = (|| {
                services.experience_recipient_supported(actor)?;
                let table = self
                    .level_table
                    .as_ref()
                    .ok_or(NpcFailure::MissingContent)?;
                let change = characters
                    .earned_experience(actor, table, amount)
                    .map_err(|_| NpcFailure::MissingContent)?;
                if change.services.after.level > change.services.before.level {
                    // Until the separate valuable world-vital restore stage is
                    // implemented, admit only the exact source no-change case.
                    for vital in [
                        bace_entity::EntityVital::Health,
                        bace_entity::EntityVital::Stamina,
                        bace_entity::EntityVital::Mana,
                    ] {
                        let pool = world
                            .vital(actor, vital)
                            .map_err(|_| NpcFailure::MissingContent)?;
                        if pool.current != pool.maximum {
                            return Err(NpcFailure::Unsupported);
                        }
                    }
                }
                Ok(change)
            })();
            match result {
                Ok(change) => {
                    let pending = self.pending.get_mut(&ticket).expect("retained");
                    pending.proposal.effect = NpcEffect::EarnedExperience { actor, change };
                    self.proposals.retain(|p| p.ticket != ticket);
                    self.proposals.push_back(pending.proposal.clone());
                }
                Err(error) => errors.push((context.source, NativeError::Owner(error))),
            }
        }
    }
}

impl Npcs {
    pub(crate) fn ready_shared_experience(&self) -> Option<NpcProposal> {
        self.pending
            .values()
            .find(|p| {
                p.detached
                    && !p.adopted
                    && matches!(
                        p.proposal.effect,
                        NpcEffect::QueuedExperience {
                            phase: NpcQueuedExperiencePhase::Ready,
                            ..
                        }
                    )
                    && self
                        .sources
                        .get(&p.proposal.context.source)
                        .is_some_and(|s| {
                            s.recovery_ready
                                && s.journal_hold.is_none()
                                && !s
                                    .manager
                                    .has_immediate_invocation(p.proposal.context.operation)
                        })
            })
            .map(|p| p.proposal.clone())
    }
    pub(crate) fn attach_shared_experience(
        &mut self,
        expected: &NpcProposal,
        change: bace_character::EarnedExperienceChange,
    ) -> Result<NpcProposal, NpcFailure> {
        let p = self
            .pending
            .get_mut(&expected.ticket)
            .filter(|p| p.proposal == *expected && p.detached && !p.adopted)
            .ok_or(NpcFailure::Conflict)?;
        let NpcEffect::QueuedExperience { actor, .. } = expected.effect else {
            return Err(NpcFailure::Conflict);
        };
        p.proposal.effect = NpcEffect::EarnedExperience { actor, change };
        self.proposals.retain(|p| p.ticket != expected.ticket);
        Ok(p.proposal.clone())
    }
    pub(crate) fn validate_shared_experience(
        &self,
        expected: &NpcProposal,
    ) -> Result<(), NpcFailure> {
        let p = self
            .pending
            .get(&expected.ticket)
            .ok_or(NpcFailure::Conflict)?;
        if p.proposal != *expected
            || !p.detached
            || p.adopted
            || !matches!(p.proposal.effect, NpcEffect::EarnedExperience { .. })
        {
            return Err(NpcFailure::Conflict);
        }
        Ok(())
    }
    pub(crate) fn mark_shared_experience_adopted(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<(), NpcFailure> {
        self.validate_shared_experience(expected)?;
        self.pending
            .get_mut(&expected.ticket)
            .expect("validated")
            .adopted = true;
        Ok(())
    }
}
