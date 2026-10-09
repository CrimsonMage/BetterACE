//! Single-owner dispatch. The outer kernel queue must reserve one correlated
//! outcome slot before invoking this method; queue admission is not a receipt.
use super::Kernel;
use crate::{NpcServiceAction as A, NpcServiceCommand, NpcServiceOutcome, NpcServiceResult as R};
impl Kernel {
    pub fn dispatch_npc_service(&mut self, command: NpcServiceCommand) -> NpcServiceOutcome {
        let checkpoint_complete = match &command.action {
            A::CommitIdle { source, .. } => Some(*source),
            A::CommitHandIn { ticket, .. }
                if ticket.checkpoint.vm.work.is_empty() && ticket.checkpoint.pending.is_empty() =>
            {
                Some(ticket.request.source)
            }
            _ => None,
        };
        let correlation = command.correlation;
        let inventory_release = super::source_inventory::release_action(&command.action);
        let release = match &command.action {
            A::RejectTrainingCredits { ticket } => Some(ticket.npc.clone()),
            A::RejectSpellbook { ticket } => Some(ticket.npc.clone()),
            A::RejectSkillReset { ticket } => Some(ticket.npc.clone()),
            A::RejectInventory { ticket } => Some(ticket.npc.clone()),
            A::RejectTeleport { ticket } => Some(ticket.npc.clone()),
            A::RejectDeletion { ticket } => Some(ticket.npc.clone()),
            _ => None,
        };
        let result = if !command.valid_bounds() {
            Err(bace_gameplay_api::NpcFailure::Capacity)
        } else {
            match command.action {
                A::BindAdmitted { actor, identity } => self
                    .npcs
                    .bind_admitted(actor, identity)
                    .map(|()| R::Applied),
                A::RestoreObjectRegistry {
                    source,
                    revision,
                    entries,
                } => {
                    if !self.npcs.recovery_held(source) {
                        Err(bace_gameplay_api::NpcFailure::Conflict)
                    } else {
                        self.restore_npc_object_registry(source, revision, entries)
                            .map(|()| R::Applied)
                            .map_err(|_| bace_gameplay_api::NpcFailure::DurabilityPending)
                    }
                }
                A::InspectMotion { proposal } => self.inspect_npc_motion_source(&proposal).map(
                    |(actor, epoch, revision, before, scale)| R::MotionSource {
                        actor,
                        epoch,
                        revision,
                        before,
                        scale,
                    },
                ),
                A::BeginPreparedMotion {
                    proposal,
                    actor,
                    epoch,
                    revision,
                    before,
                    chain,
                } => self
                    .begin_npc_prepared_motion(&proposal, actor, epoch, revision, before, chain)
                    .map(R::MotionStarted),
                A::BeginGive { proposal } => self
                    .begin_npc_give(&proposal)
                    .map(|(detached, turning)| R::GiveStarted { detached, turning }),
                A::PreviewServiceAdmissionNow {
                    proposal,
                    post_delay,
                } => self
                    .checkpoint_npc_source(proposal.context.source)
                    .and_then(|s| {
                        self.preview_npc_service_admission(&proposal, s.logical_now, post_delay)
                    })
                    .and_then(|snapshot| {
                        self.npcs.hold_journal(&proposal, self.tick)?;
                        Ok(R::Checkpoint(snapshot))
                    }),
                A::InspectCast { proposal } => self.inspect_npc_cast_source(&proposal).map(
                    |(epoch, property_revision, motion, scale, qualities)| R::CastSource {
                        epoch,
                        property_revision,
                        motion,
                        scale,
                        qualities,
                    },
                ),
                A::RegisterCast {
                    proposal,
                    epoch,
                    property_revision,
                    motion,
                    source,
                    definition,
                    shapes,
                } => self
                    .register_npc_cast_program(
                        &proposal,
                        epoch,
                        property_revision,
                        motion,
                        source,
                        definition,
                        shapes,
                    )
                    .map(|()| R::Applied),
                A::PrepareInventoryBatch {
                    proposal,
                    items,
                    containers,
                } => self
                    .prepare_npc_inventory_batch(&proposal, items, containers)
                    .map(R::Inventory),
                A::Use {
                    context,
                    source,
                    event,
                    operation,
                } => self
                    .use_native_npc(context, source, event, operation)
                    .map(R::Started),
                A::FreezeIdle { source, operation } => self
                    .npcs
                    .freeze_idle(source, operation, self.tick)
                    .map(R::Checkpoint),
                A::FreezeBootstrapIdle {
                    source,
                    operation,
                    event,
                } => self
                    .npcs
                    .freeze_bootstrap_idle(source, operation, event, self.tick)
                    .map(R::Checkpoint),
                A::ReleaseIdle { source, operation } | A::CommitIdle { source, operation } => self
                    .npcs
                    .release_idle(source, operation, self.tick)
                    .map(|()| R::Applied),
                A::ReleaseJournal { proposal } => self
                    .npcs
                    .release_journal_hold(&proposal, self.tick)
                    .map(|()| R::Applied),
                A::PreviewDeletionNow { ticket } => self
                    .checkpoint_npc_source(ticket.npc.context.source)
                    .and_then(|s| self.preview_npc_deletion_checkpoint(&ticket, s.logical_now))
                    .and_then(|s| {
                        self.npcs.hold_journal(&ticket.npc, self.tick)?;
                        Ok(R::Checkpoint(s))
                    }),
                A::PreviewExperienceAdmissionNow { proposal } => self
                    .checkpoint_npc_source(proposal.context.source)
                    .and_then(|s| {
                        self.preview_npc_queued_experience_admission(&proposal, s.logical_now)
                    })
                    .and_then(|s| {
                        self.npcs.hold_journal(&proposal, self.tick)?;
                        Ok(R::Checkpoint(s))
                    }),
                A::PreviewOwnerCompletion { proposal } => self
                    .preview_npc_owner_completion(&proposal)
                    .map(R::Checkpoint),
                A::RestoreIdleSourceState { checkpoint } => self
                    .restore_idle_npc_source_state(checkpoint)
                    .map(|()| R::Applied),
                A::RegisterRecovery {
                    actor,
                    program,
                    use_radius,
                    properties,
                } => match properties {
                    Some(properties) => self.register_native_npc_with_properties(
                        actor, program, use_radius, properties,
                    ),
                    None => self.register_native_npc(actor, program, use_radius),
                }
                .and_then(|()| self.npcs.hold_recovery(actor))
                .map(|()| R::Applied),
                A::Register {
                    actor,
                    program,
                    use_radius,
                    properties,
                } => match properties {
                    Some(properties) => self.register_native_npc_with_properties(
                        actor, program, use_radius, properties,
                    ),
                    None => self.register_native_npc(actor, program, use_radius),
                }
                .map(|()| R::Applied),
                A::Start {
                    source,
                    target,
                    trigger,
                    event,
                    operation,
                    check_range,
                } => self
                    .start_npc_emote(source, target, trigger, event, operation, check_range)
                    .map(R::Started)
                    .map_err(|e| match e {
                        bace_emotes::NativeError::Owner(e) => e,
                        bace_emotes::NativeError::Capacity | bace_emotes::NativeError::Budget => {
                            bace_gameplay_api::NpcFailure::Capacity
                        }
                        bace_emotes::NativeError::Busy => {
                            bace_gameplay_api::NpcFailure::DurabilityPending
                        }
                        _ => bace_gameplay_api::NpcFailure::InvalidInput,
                    }),
                A::PrepareDeletion { proposal } => {
                    self.prepare_npc_deletion(&proposal).map(R::Deletion)
                }
                A::PreviewDeletion {
                    ticket,
                    logical_now,
                } => self
                    .preview_npc_deletion_checkpoint(&ticket, logical_now)
                    .map(R::Checkpoint),
                A::CommitRetirement { ticket, receipt } => self
                    .confirm_npc_retirement_committed(&ticket, &receipt)
                    .map(|()| R::Applied),
                A::CommitTransientDeletion { ticket } => self
                    .confirm_npc_transient_deletion(&ticket)
                    .map(|()| R::Applied),
                A::RejectDeletion { ticket } => {
                    self.reject_npc_deletion(&ticket).map(|()| R::Applied)
                }
                A::RegisterArchive {
                    checkpoint,
                    program,
                    use_radius,
                } => self
                    .register_archived_npc(checkpoint, program, use_radius)
                    .map(|()| R::Applied),
                A::ReleaseArchive { source } => {
                    self.release_npc_archive(source).map(|()| R::Applied)
                }
                A::KillSelf { proposal } => self
                    .apply_npc_kill_self(&proposal)
                    .map(|damage| R::Killed { damage }),
                A::PrepareTeleport { proposal } => {
                    self.prepare_npc_teleport(&proposal).map(R::Teleport)
                }
                A::CommitTeleport { ticket, receipt } => self
                    .confirm_npc_teleport_committed(&ticket, &receipt)
                    .map(|()| R::Applied),
                A::RejectTeleport { ticket } => {
                    self.reject_npc_teleport(&ticket).map(|()| R::Applied)
                }
                A::ResetHome { proposal } => self.apply_npc_reset_home(&proposal).map(R::Home),
                A::PrepareSkillReset { proposal } => {
                    self.prepare_npc_skill_reset(&proposal).map(R::SkillReset)
                }
                A::CommitSkillReset { ticket } => self
                    .confirm_npc_skill_reset_committed(&ticket)
                    .map(|()| R::Applied),
                A::RejectSkillReset { ticket } => {
                    self.reject_npc_skill_reset(&ticket).map(|()| R::Applied)
                }
                A::PrepareHandIn { request } => self.prepare_npc_handin(request).map(R::HandIn),
                A::CommitHandIn { ticket, receipt } => self
                    .confirm_npc_handin_committed(&ticket, &receipt)
                    .map(|()| R::Applied),
                A::RejectHandIn { ticket } => self.reject_npc_handin(&ticket).map(|()| R::Applied),
                A::PrepareInventory { proposal, item } => self
                    .prepare_npc_inventory(&proposal, item)
                    .map(R::Inventory),
                A::CommitInventory { ticket, receipt } => self
                    .confirm_npc_inventory_committed(&ticket, &receipt)
                    .map(|()| R::Applied),
                A::RejectInventory { ticket } => {
                    self.reject_npc_inventory(&ticket).map(|()| R::Applied)
                }
                A::PrepareSpellbook { proposal } => {
                    self.prepare_npc_spellbook(&proposal).map(R::Spellbook)
                }
                A::CommitSpellbook { ticket } => self
                    .confirm_npc_spellbook_committed(&ticket)
                    .map(|()| R::Applied),
                A::RejectSpellbook { ticket } => {
                    self.reject_npc_spellbook(&ticket).map(|()| R::Applied)
                }
                A::PrepareTrainingCredits { proposal } => self
                    .prepare_npc_training_credits(&proposal)
                    .map(R::TrainingCredits),
                A::CommitTrainingCredits { ticket } => self
                    .confirm_npc_training_credits_committed(&ticket)
                    .map(|()| R::Applied),
                A::RejectTrainingCredits { ticket } => self
                    .reject_npc_training_credits(&ticket)
                    .map(|()| R::Applied),
                A::BeginCast { proposal } => self.begin_npc_cast(&proposal).map(R::CastStarted),
                A::PollCast { proposal } => self.poll_npc_cast(&proposal).map(R::CastProgress),
                A::CommitCast { proposal, outcome } => self
                    .adopt_npc_cast_completion(&proposal, &outcome)
                    .map(|()| R::Applied),
                A::BeginMotion { proposal, chain } => self
                    .begin_npc_motion(&proposal, chain)
                    .map(R::MotionStarted),
                A::PollMotion { proposal } => {
                    self.poll_npc_motion(&proposal).map(R::MotionProgress)
                }
                A::CommitMotion { proposal, event } => self
                    .adopt_npc_motion_completion(&proposal, event)
                    .map(|()| R::Applied),
                A::BeginMovement { proposal, home } => self
                    .begin_npc_movement(&proposal, home)
                    .map(|()| R::Applied),
                A::PollMovement { proposal } => {
                    self.poll_npc_movement(&proposal).map(R::MovementProgress)
                }
                A::CancelMovement { proposal } => {
                    self.cancel_npc_movement(&proposal).map(|()| R::Applied)
                }
                A::Door { proposal } => self.apply_npc_door(&proposal).map(R::Door),
                A::Generate { proposal } => self.apply_npc_generate(&proposal).map(|()| R::Applied),
                A::Signal { proposal } => self
                    .apply_npc_signal(&proposal)
                    .map(|listeners| R::Signal { listeners }),
                A::PreviewCompletion {
                    proposal,
                    completion,
                    logical_now,
                } => self
                    .preview_npc_committed_checkpoint(&proposal, completion, logical_now)
                    .map(R::Checkpoint),
                A::PreviewServiceAdmission {
                    proposal,
                    logical_now,
                    post_delay,
                } => self
                    .preview_npc_service_admission(&proposal, logical_now, post_delay)
                    .map(R::Checkpoint),
                A::AdmitService {
                    proposal,
                    post_delay,
                } => self
                    .admit_npc_service(&proposal, post_delay)
                    .map(|()| R::Applied),
                A::PreviewExperienceAdmission {
                    proposal,
                    logical_now,
                } => self
                    .preview_npc_queued_experience_admission(&proposal, logical_now)
                    .map(R::Checkpoint),
                A::AdmitExperience { proposal } => self
                    .admit_npc_queued_experience(&proposal)
                    .map(|()| R::Applied),
                A::CommitPlayerEffect { proposal } => {
                    self.confirm_npc_committed(&proposal).map(|()| R::Applied)
                }
                A::Checkpoint { source } => self.checkpoint_npc_source(source).map(R::Checkpoint),
                A::Restore { checkpoint } => {
                    self.restore_npc_source(checkpoint).map(|()| R::Applied)
                }
                A::RecoveryReady { source } => self
                    .acknowledge_npc_recovery_ready(source)
                    .map(|()| R::Applied),
            }
        };
        let result = result.and_then(|value| {
            let value = match value {
                R::Checkpoint(snapshot) => {
                    let mut snapshot = self.capture_npc_source_state(snapshot)?;
                    if snapshot.archive.is_none() {
                        snapshot.inventory = self.capture_npc_source_inventory(snapshot.source)?;
                    }
                    R::Checkpoint(snapshot)
                }
                R::HandIn(mut ticket) => {
                    ticket.checkpoint.inventory =
                        match self.capture_npc_source_inventory(ticket.request.source) {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                self.reject_npc_handin(&ticket).expect(
                                "same-tick failed source capture rolls back hand-in reservation",
                            );
                                return Err(error);
                            }
                        };
                    self.npcs
                        .handins
                        .get_mut(&ticket.inventory.operation)
                        .ok_or(bace_gameplay_api::NpcFailure::Conflict)?
                        .0 = ticket.clone();
                    R::HandIn(ticket)
                }
                other => other,
            };
            if let Some(proposal) = release {
                self.npcs.release_journal_hold(&proposal, self.tick)?;
            }
            if let Some((source, ticket, committed)) = inventory_release {
                self.release_npc_source_inventory(source, ticket, committed)?;
            }
            if let Some(source) = checkpoint_complete {
                self.npcs.mark_checkpoint_complete(source);
            }
            Ok(value)
        });
        NpcServiceOutcome {
            correlation,
            result,
        }
    }
}
