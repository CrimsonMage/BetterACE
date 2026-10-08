//! Named service preparation and joined player/checkpoint durability. The source
//! remains reserved until the simulation adopts the exact committed mutation.
use super::*;
use crate::{
    npc_persistence::*, npc_service::NpcDurableAdoption,
    online_player_saves::OnlinePlayerSaveService,
};
use bace_simulation::{NpcSourceCheckpoint, PlayerReadSnapshot};

#[derive(Clone)]
pub(super) enum Owner {
    Motion(bace_world::WorldMotionEvent),
    Cast(bace_gameplay_api::ServerCastOutcome),
    Inventory(bace_simulation::NpcInventoryTicket),
    Portal(bace_simulation::NpcPortalTicket),
    Delete(bace_simulation::NpcDeleteSourceTicket),
    Player,
    Credits(bace_simulation::NpcTrainingCreditTicket),
    Book(bace_simulation::NpcSpellbookTicket),
    Skill(bace_simulation::NpcSkillResetTicket),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Queued,
    Preparing,
    Previewing,
    Ready,
    Saving,
    Delivering,
    Moving,
    Direct,
}
pub(super) struct Work {
    pub(super) proposal: NpcProposal,
    pub(super) phase: Phase,
    pub(super) owner: Owner,
    pub(super) cast: Option<super::casting::Cast>,
    pub(super) gift: Option<super::giving::Gift>,
    pub(super) motion: Option<super::motions::Motion>,
    pub(super) checkpoint: Option<NpcSourceCheckpoint>,
    pub(super) capture: Option<u64>,
    pub(super) snapshot: Option<(Arc<PlayerReadSnapshot>, u64)>,
    pub(super) frozen: Option<(PendingNpcStage, NpcDurableAdoption)>,
    pub(super) deletion_sources: Option<Vec<crate::game_inventory::FrozenInventoryItem>>,
    pub(super) deletion_loading: bool,
    pub(super) critical: Option<u32>,
    pub(super) rows: Vec<bace_persistence::SaveSnapshot>,
    pub(super) committed: Option<bool>,
}
impl Work {
    pub(super) fn ticket(&self) -> u64 {
        self.proposal.ticket
    }
    pub(super) fn set_deletion_sources(
        &mut self,
        result: Result<Vec<crate::game_inventory::FrozenInventoryItem>, String>,
    ) -> Result<(), String> {
        self.deletion_loading = false;
        self.deletion_sources = Some(result?);
        Ok(())
    }

    pub(super) fn involves(&self, actor: EntityId) -> bool {
        self.proposal.context.source == actor || self.proposal.context.target == Some(actor)
    }
    pub(super) fn new(proposal: NpcProposal) -> Self {
        Self {
            proposal,
            phase: Phase::Queued,
            owner: Owner::Player,
            cast: None,
            gift: None,
            motion: None,
            checkpoint: None,
            capture: None,
            snapshot: None,
            frozen: None,
            deletion_sources: None,
            deletion_loading: false,
            critical: None,
            rows: vec![],
            committed: None,
        }
    }
    pub(super) fn actor_revision(&self) -> Option<(EntityId, u64)> {
        match &self.owner {
            Owner::Cast(_) | Owner::Motion(_) => None,
            Owner::Inventory(ticket) => Some((ticket.inventory.actor, ticket.character_revision)),
            Owner::Portal(ticket) => Some((ticket.portal.actor, ticket.portal.before_revision)),
            Owner::Delete(_) => None,
            Owner::Credits(t) => Some((t.actor, t.change.before_revision)),
            Owner::Book(t) => Some((t.actor, t.before_revision)),
            Owner::Skill(t) => Some((t.actor, t.before_revision)),
            Owner::Player => match &self.proposal.effect {
                NpcEffect::Experience { actor, credit } => Some((*actor, credit.before_revision)),
                NpcEffect::Luminance { actor, credit } => Some((*actor, credit.before_revision)),
                NpcEffect::EarnedExperience { actor, change } => {
                    Some((*actor, change.experience.before_revision))
                }
                NpcEffect::CharacterService { actor, change } => {
                    Some((*actor, change.before_revision))
                }
                NpcEffect::Contract {
                    actor,
                    before_revision,
                    ..
                } => Some((*actor, *before_revision)),
                NpcEffect::Property {
                    actor,
                    aggregate: Some(fence),
                    ..
                }
                | NpcEffect::Quest {
                    actor,
                    aggregate: Some(fence),
                    ..
                } => Some((*actor, fence.before_revision)),
                _ => None,
            },
        }
    }
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    online: &mut OnlinePlayerSaveService,
    event: NpcCoordinatorEvent,
) -> Result<(), (String, NpcCoordinatorEvent)> {
    let source = match &event {
        NpcCoordinatorEvent::Checkpoint { source, .. }
        | NpcCoordinatorEvent::Command { source, .. }
        | NpcCoordinatorEvent::Durable { source, .. }
        | NpcCoordinatorEvent::HandIn { source, .. }
        | NpcCoordinatorEvent::Held { source, .. } => *source,
    };
    if let Some(used) = npc.uses.get_mut(&source)
        && let NpcCoordinatorEvent::Command { outcome, .. } = &event
    {
        used.result = Some(match outcome.result {
            Ok(R::Started(_)) => 0,
            Err(bace_gameplay_api::NpcFailure::DurabilityPending) => 0x001d,
            Err(_) => 0x0036,
            _ => return Err(("NPC Use receipt mismatch".into(), event)),
        });
        return Ok(());
    }
    if npc.handins.contains_key(&source) {
        return super::handin::accept(npc, online, event);
    }
    if npc.idle.contains_key(&source) {
        return super::terminal::accept(npc, event);
    }
    if let Some((ticket, checkpoint)) = &mut npc.shared
        && ticket
            .npc
            .as_ref()
            .is_some_and(|p| p.context.source == source)
        && let NpcCoordinatorEvent::Command { outcome, .. } = &event
    {
        match &outcome.result {
            Ok(R::Checkpoint(value)) => {
                *checkpoint = Some(value.clone());
                return Ok(());
            }
            Err(error) => {
                return Err((format!("NPC shared checkpoint held: {error:?}"), event));
            }
            _ => return Err(("NPC shared result mismatch".into(), event)),
        }
    }
    match super::giving::accept(npc, source, &event) {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(error) => return Err((error, event)),
    }
    if npc.work.get(&source).is_some_and(|w| w.motion.is_some())
        && let NpcCoordinatorEvent::Command { outcome, .. } = &event
    {
        return super::motions::accept(npc, source, outcome, online)
            .map_err(|error| (error, event));
    }
    if npc.work.get(&source).is_some_and(|w| w.cast.is_some())
        && let NpcCoordinatorEvent::Command { outcome, .. } = &event
    {
        return super::casting::accept(npc, source, outcome).map_err(|error| (error, event));
    }
    let Some(work) = npc.work.get_mut(&source) else {
        if let NpcCoordinatorEvent::Command { outcome, .. } = &event
            && let Err(error) = outcome.result
        {
            npc.failure = Some(format!("NPC source registration/recovery held: {error:?}"));
            return Ok(());
        }
        if let NpcCoordinatorEvent::Held { message, .. } = &event {
            npc.failure = Some(message.clone());
            return Ok(());
        }
        return Ok(());
    };
    let mut finished = false;
    match &event {
        NpcCoordinatorEvent::Held { message, .. } => {
            npc.failure = Some(message.clone());
        }
        NpcCoordinatorEvent::Durable { resolution, .. } => match resolution.as_ref() {
            NpcStageResolution::Uncertain { message } => {
                npc.failure = Some(message.clone());
            }
            NpcStageResolution::Rejected { .. } => {
                work.committed = Some(false);
                work.phase = Phase::Delivering;
            }
            NpcStageResolution::Committed { .. } => {
                work.committed = Some(true);
                work.phase = Phase::Delivering;
            }
        },
        NpcCoordinatorEvent::Command { outcome, .. } => match &outcome.result {
            Err(error) => {
                npc.failure = Some(format!("NPC owner operation held: {error:?}"));
                if work.phase != Phase::Delivering && work.phase != Phase::Moving {
                    work.phase = Phase::Queued;
                }
                return Ok(());
            }
            Ok(result) => match result {
                R::Inventory(ticket) => {
                    work.owner = Owner::Inventory(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::Teleport(ticket) => {
                    work.owner = Owner::Portal(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::Deletion(ticket) => {
                    work.owner = Owner::Delete(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::TrainingCredits(ticket) => {
                    work.owner = Owner::Credits(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::Spellbook(ticket) => {
                    work.owner = Owner::Book(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::SkillReset(ticket) => {
                    work.owner = Owner::Skill(ticket.clone());
                    work.phase = Phase::Queued;
                }
                R::Checkpoint(checkpoint) => {
                    work.checkpoint = Some(checkpoint.clone());
                    work.phase = Phase::Ready;
                }
                R::MovementProgress(done) => {
                    finished = *done;
                }
                R::Applied if work.phase == Phase::Direct => finished = true,
                R::Applied if work.phase == Phase::Delivering => finished = true,
                R::Applied if work.phase == Phase::Moving => {}
                R::Door(_) | R::Signal { .. } | R::Home(_) | R::Killed { .. } => finished = true,
                _ => {
                    return Err((
                        "NPC result does not match retained service phase".into(),
                        event,
                    ));
                }
            },
        },
        NpcCoordinatorEvent::Checkpoint { .. } => {
            return Err(("NPC terminal result missing idle owner".into(), event));
        }
        NpcCoordinatorEvent::HandIn { .. } => {
            return Err(("NPC hand-in result missing ingress owner".into(), event));
        }
    }
    if finished {
        if work.committed == Some(true)
            && matches!(work.owner, Owner::Inventory(_))
            && npc.inventory_output.len() == 64
        {
            return Err(("NPC inventory output capacity".into(), event));
        }
        if matches!(work.owner, Owner::Delete(_)) && work.committed == Some(true) {
            if npc.retired.len() == 256 {
                return Err((
                    "NPC source retirement suppression outbox full".into(),
                    event,
                ));
            }
            npc.retired.push_back(source);
        }
        if let Some(actor) = work.critical {
            let result = if work.committed == Some(true) {
                online.finish_critical_for(&[actor], &work.rows)
            } else {
                online.cancel_critical(&[actor])
            };
            if let Err(error) = result {
                return Err((error, event));
            }
        }
        if work.committed == Some(true)
            && let Owner::Inventory(ticket) = &work.owner
        {
            let Some((snapshot, _)) = &work.snapshot else {
                return Err(("NPC inventory output snapshot missing".into(), event));
            };
            let gift = if let Some(super::giving::Gift::Built(gift)) = &work.gift {
                Some(gift.clone())
            } else {
                None
            };
            npc.inventory_output
                .push_back(super::inventory_output::Delivery {
                    binding: snapshot.binding(),
                    ticket: ticket.clone(),
                    gift,
                    cursor: 0,
                });
        }
        if work.committed == Some(false) {
            let proposal = work.proposal.clone();
            if matches!(work.owner, Owner::Cast(_) | Owner::Motion(_)) {
                work.phase = Phase::Queued;
                work.committed = None;
                work.checkpoint = None;
                work.frozen = None;
                work.rows.clear();
            } else {
                *work = Work::new(proposal);
            }
        } else {
            npc.work.remove(&source);
        }
    }
    Ok(())
}
impl GameRuntime {
    pub(super) fn poll_npc_effects(&mut self) -> Result<(), String> {
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let ids: Vec<_> = self
            .npc
            .work
            .range((
                self.npc.effect_cursor.map_or(Unbounded, Excluded),
                Unbounded,
            ))
            .chain(self.npc.work.range((
                Unbounded,
                self.npc.effect_cursor.map_or(Unbounded, Included),
            )))
            .map(|(&id, _)| id)
            .take(self.limits.work_per_poll.min(self.npc.work.len()))
            .collect();
        if let Some(&last) = ids.last() {
            self.npc.effect_cursor = Some(last);
        }
        let mut failure = None;
        for source in ids {
            let result = (|| -> Result<(), String> {
                if self.npc.coordinator.busy(source) {
                    return Ok(());
                }
                if self.npc.work.get(&source).is_some_and(|w| {
                    matches!(
                        w.proposal.effect,
                        NpcEffect::Service(NpcOperation::Cast { .. })
                    ) && matches!(w.owner, Owner::Player)
                }) {
                    self.poll_npc_cast(source)?;
                    return Ok(());
                }
                if self.npc.work.get(&source).is_some_and(|w| {
                    matches!(
                        w.proposal.effect,
                        NpcEffect::Service(NpcOperation::Give { .. })
                    ) && matches!(w.owner, Owner::Player)
                        && !matches!(w.phase, Phase::Saving | Phase::Delivering)
                }) {
                    self.poll_npc_give(source)?;
                    return Ok(());
                }
                if self.npc.work.get(&source).is_some_and(|w| {
                    matches!(
                        w.proposal.effect,
                        NpcEffect::Service(NpcOperation::Motion { .. })
                    ) && matches!(w.owner, Owner::Player)
                }) {
                    self.poll_npc_motion(source)?;
                    return Ok(());
                }
                let destination =
                    self.npc
                        .work
                        .get(&source)
                        .and_then(|w| match &w.proposal.effect {
                            NpcEffect::Service(NpcOperation::TeleportTarget(destination))
                                if matches!(w.owner, Owner::Player) =>
                            {
                                destination.cell.map(|c| c.0)
                            }
                            _ => None,
                        });
                if let Some(cell) = destination
                    && !self.ensure_npc_region(source, cell)?
                {
                    return Ok(());
                }
                let token = self.token()?;
                if self
                    .npc
                    .work
                    .get(&source)
                    .is_some_and(|w| w.phase == Phase::Ready)
                {
                    return self.poll_npc_ready(source, token);
                }
                let work = self.npc.work.get_mut(&source).expect("retained work");
                if work.phase == Phase::Queued {
                    let action = match (&work.proposal.effect, &work.owner) {
                        (_, Owner::Cast(_) | Owner::Motion(_)) => {
                            work.phase = Phase::Previewing;
                            A::PreviewOwnerCompletion {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::Take { .. }), Owner::Player) => {
                            work.phase = Phase::Preparing;
                            A::PrepareInventory {
                                proposal: work.proposal.clone(),
                                item: None,
                            }
                        }
                        (NpcEffect::Service(NpcOperation::TeleportTarget(_)), Owner::Player) => {
                            work.phase = Phase::Preparing;
                            A::PrepareTeleport {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (_, Owner::Delete(ticket)) => {
                            work.phase = Phase::Previewing;
                            A::PreviewDeletionNow {
                                ticket: ticket.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::DeleteSelf), Owner::Player) => {
                            work.phase = Phase::Preparing;
                            A::PrepareDeletion {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::QueuedExperience {
                                phase: bace_simulation::NpcQueuedExperiencePhase::AwaitingAdmission,
                                ..
                            },
                            _,
                        ) => {
                            work.phase = Phase::Previewing;
                            A::PreviewExperienceAdmissionNow {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::Service(NpcOperation::Reward {
                                kind: NpcRewardKind::TrainingCredits,
                                ..
                            }),
                            Owner::Player,
                        ) => {
                            work.phase = Phase::Preparing;
                            A::PrepareTrainingCredits {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::Service(NpcOperation::Reward {
                                kind: NpcRewardKind::TeachSpell,
                                ..
                            }),
                            Owner::Player,
                        ) => {
                            work.phase = Phase::Preparing;
                            A::PrepareSpellbook {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::Service(NpcOperation::Reward {
                                kind: NpcRewardKind::UntrainSkill,
                                ..
                            }),
                            Owner::Player,
                        ) => {
                            work.phase = Phase::Preparing;
                            A::PrepareSkillReset {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::Service(
                                NpcOperation::Move { .. } | NpcOperation::Turn { .. },
                            ),
                            _,
                        ) => {
                            work.phase = Phase::Moving;
                            A::BeginMovement {
                                proposal: work.proposal.clone(),
                                home: None,
                            }
                        }
                        (NpcEffect::Service(NpcOperation::OpenSelf(_)), _) => {
                            work.phase = Phase::Direct;
                            A::Door {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::Signal(_)), _) => {
                            work.phase = Phase::Direct;
                            A::Signal {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::Generate), _) => {
                            work.phase = Phase::Direct;
                            A::Generate {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::ResetHome), _) => {
                            work.phase = Phase::Direct;
                            A::ResetHome {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (NpcEffect::Service(NpcOperation::KillSelf), _) => {
                            work.phase = Phase::Direct;
                            A::KillSelf {
                                proposal: work.proposal.clone(),
                            }
                        }
                        (
                            NpcEffect::Property {
                                actor,
                                aggregate: None,
                                ..
                            }
                            | NpcEffect::Quest {
                                actor,
                                aggregate: None,
                                ..
                            },
                            _,
                        ) if *actor == source => {
                            work.phase = Phase::Previewing;
                            A::PreviewOwnerCompletion {
                                proposal: work.proposal.clone(),
                            }
                        }
                        _ if work.actor_revision().is_some() => {
                            work.phase = Phase::Previewing;
                            A::PreviewOwnerCompletion {
                                proposal: work.proposal.clone(),
                            }
                        }
                        _ => {
                            return Err(format!(
                                "NPC source {} requires a prepared named service: {:?}",
                                source.0, work.proposal.effect
                            ));
                        }
                    };
                    if self.npc.coordinator.enqueue(source, action).is_err() {
                        return Err("NPC service command admission retained".into());
                    }
                } else if work.phase == Phase::Moving {
                    self.npc
                        .coordinator
                        .enqueue(
                            source,
                            A::PollMovement {
                                proposal: work.proposal.clone(),
                            },
                        )
                        .map_err(|_| "NPC movement poll held")?;
                } else if work.phase == Phase::Saving
                    && let Some((mut pending, adoption)) = work.frozen.take()
                {
                    let prepared = super::source_inventory::prepare(
                        &mut self.npc.source_inventory,
                        self.world.as_ref().map(|w| &w.regions),
                        &self.npc.definitions,
                        work.checkpoint.as_ref().and_then(|c| c.inventory.as_ref()),
                        self.bootstrap.world_owner.epoch(),
                    )
                    .and_then(|frozen| {
                        if let Some(frozen) = frozen {
                            pending
                                .attach_source_inventory(&frozen)
                                .map_err(|e| e.to_string())?;
                        }
                        Ok(())
                    });
                    if let Err(error) = prepared {
                        work.frozen = Some((pending, adoption));
                        return Err(error);
                    }
                    if let Err(pending) =
                        self.npc
                            .coordinator
                            .enqueue_stage(source, pending, adoption.clone())
                    {
                        work.frozen = Some((*pending, adoption));
                    }
                }

                Ok(())
            })();
            if let Err(error) = result {
                self.npc.failure = Some(error.clone());
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
