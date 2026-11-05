//! NPC motion delivery consumes actual epoch/token callbacks after cold source
//! preparation; no elapsed-time estimate acknowledges an animation completion.
use super::effects::{Owner, Phase};
use super::*;
use crate::npc_motion_assets::{NpcMotionWork, NpcMotionWorker};
use crate::npc_persistence::{NpcServiceCompletionInput, freeze_service_completion};
use crate::npc_service::NpcDurableAdoption;
const MAX_MOTION_START_ATTEMPTS: u16 = 120;
const MAX_STALE_MOTION_REPREPARES: u16 = 3;
pub(super) enum Motion {
    Inspect,
    Inspecting,
    Cold {
        work: Arc<NpcMotionWork>,
        submitted: bool,
    },
    Ready {
        work: Arc<NpcMotionWork>,
        chain: Arc<bace_motion::PreparedMotionChain>,
    },
    Starting {
        work: Arc<NpcMotionWork>,
        chain: Arc<bace_motion::PreparedMotionChain>,
    },
    Running,
    Polling,
    Skip,
    SkipPreview,
    SkipReady,
    SkipSaving,
    Held(String),
}
pub(super) fn awaiting_command(state: &Motion) -> bool {
    matches!(
        state,
        Motion::Inspecting | Motion::Starting { .. } | Motion::Polling | Motion::SkipPreview
    )
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    source: EntityId,
    outcome: &bace_simulation::NpcServiceOutcome,
    online: &crate::online_player_saves::OnlinePlayerSaveService,
) -> Result<(), String> {
    let work = npc
        .work
        .get_mut(&source)
        .ok_or("NPC motion source missing")?;
    let state = work.motion.as_mut().ok_or("NPC motion owner missing")?;
    match &outcome.result {
        Ok(R::MotionSource {
            actor,
            epoch,
            revision,
            before,
            scale,
        }) if matches!(state, Motion::Inspecting) => {
            let NpcEffect::Service(NpcOperation::Motion {
                target,
                motion,
                extent,
                style,
                substyle,
            }) = work.proposal.effect
            else {
                return Err("NPC motion opcode mismatch".into());
            };
            let authored = if let Some(definition) = npc.definitions.get(actor) {
                definition.source.authored.clone()
            } else {
                Arc::new(
                    online
                        .baseline(actor.0)
                        .ok_or("NPC target motion source unavailable")?
                        .0
                        .player
                        .entity
                        .state
                        .clone(),
                )
            };
            let table = authored
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == 2)
                .ok_or("NPC actor motion table missing")?
                .value;
            *state = Motion::Cold {
                work: Arc::new(NpcMotionWork {
                    source,
                    actor: *actor,
                    ticket: work.proposal.ticket,
                    epoch: *epoch,
                    revision: *revision,
                    table,
                    before: *before,
                    scale: *scale as f32,
                    motion,
                    speed: extent,
                    target,
                    style,
                    substyle,
                }),
                submitted: false,
            };
        }
        Ok(R::MotionStarted(_)) if matches!(state, Motion::Starting { .. }) => {
            work.motion_retries = 0;
            *state = Motion::Running;
        }
        Ok(R::MotionBusy) if matches!(state, Motion::Starting { .. }) => {
            work.motion_retries = work.motion_retries.saturating_add(1);
            if work.motion_retries >= MAX_MOTION_START_ATTEMPTS {
                *state = Motion::Skip;
            } else if let Motion::Starting { work, chain } = state {
                *state = Motion::Ready {
                    work: work.clone(),
                    chain: chain.clone(),
                };
            }
        }
        Ok(R::Checkpoint(checkpoint)) if matches!(state, Motion::SkipPreview) => {
            work.checkpoint = Some(checkpoint.clone());
            work.phase = Phase::Ready;
            *state = Motion::SkipReady;
        }
        Ok(R::MotionProgress(event)) if matches!(state, Motion::Polling) => {
            if let Some(event) = event
                && matches!(
                    event.event,
                    bace_motion::MotionExecutionEvent::Completed
                        | bace_motion::MotionExecutionEvent::Cancelled
                )
            {
                work.owner = Owner::Motion(*event);
                work.phase = Phase::Queued;
                work.motion = None;
            } else {
                *state = Motion::Running;
            }
        }
        Err(bace_gameplay_api::NpcFailure::Conflict)
            if matches!(state, Motion::Inspecting | Motion::Starting { .. }) =>
        {
            work.motion_retries = work.motion_retries.saturating_add(1);
            *state = if work.motion_retries >= MAX_STALE_MOTION_REPREPARES {
                Motion::Skip
            } else {
                Motion::Inspect
            };
        }
        Err(error) => {
            let phase = match state {
                Motion::Inspecting => "inspect",
                Motion::Starting { .. } => "start",
                Motion::Polling => "poll",
                Motion::SkipPreview => "skip preview",
                _ => "unexpected",
            };
            let message = format!(
                "NPC motion owner held: {error:?}; phase {phase}, source {:#010x}, target {:?}, ticket {}",
                source.0, work.proposal.context.target, work.proposal.ticket
            );
            *state = Motion::Held(message.clone());
            npc.failure = Some(message);
        }
        _ => return Err("NPC motion phase/result mismatch".into()),
    }
    Ok(())
}
impl GameRuntime {
    pub(super) fn poll_npc_motion_worker(&mut self) -> Result<(), String> {
        if self.npc.motion_unexpected.is_some() {
            return Err("unmatched NPC motion completion retained".into());
        }
        if let Some(worker) = &self.npc.motion_worker
            && let Some(result) = worker.poll()?
        {
            let source = result.work.source;
            let Some(work) = self.npc.work.get_mut(&source) else {
                self.npc.motion_unexpected = Some(result);
                return Err("NPC motion result source missing".into());
            };
            if work.proposal.ticket != result.work.ticket
                || !matches!(&work.motion,Some(Motion::Cold{work,submitted:true})if Arc::ptr_eq(work,&result.work))
            {
                self.npc.motion_unexpected = Some(result);
                return Err("NPC motion result ticket mismatch".into());
            }
            work.motion = Some(match result.result {
                Ok(chain) => Motion::Ready {
                    work: result.work,
                    chain,
                },
                Err(error) => {
                    self.npc.failure = Some(error.clone());
                    Motion::Held(error)
                }
            });
        }
        Ok(())
    }
    pub(super) fn poll_npc_motion(&mut self, source: EntityId) -> Result<(), String> {
        if self.npc.coordinator.busy(source) {
            return Ok(());
        }
        let work = self
            .npc
            .work
            .get_mut(&source)
            .ok_or("NPC motion work missing")?;
        work.phase = Phase::Preparing;
        let state = work.motion.get_or_insert(Motion::Inspect);
        match state {
            Motion::Inspect => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::InspectMotion {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC motion inspection pressure")?;
                *state = Motion::Inspecting;
            }
            Motion::Cold {
                work: request,
                submitted,
            } if !*submitted => {
                if self.npc.motion_worker.is_none() {
                    self.npc.motion_worker =
                        Some(NpcMotionWorker::start(self.bootstrap.assets.clone())?);
                }
                if self
                    .npc
                    .motion_worker
                    .as_ref()
                    .expect("NPC motion worker")
                    .submit(request.clone())
                    .is_ok()
                {
                    *submitted = true;
                }
            }
            Motion::Ready {
                work: request,
                chain,
            } => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::BeginPreparedMotion {
                            proposal: work.proposal.clone(),
                            actor: request.actor,
                            epoch: request.epoch,
                            revision: request.revision,
                            before: request.before,
                            chain: chain.clone(),
                        },
                    )
                    .map_err(|_| "NPC motion begin pressure")?;
                *state = Motion::Starting {
                    work: request.clone(),
                    chain: chain.clone(),
                };
            }
            Motion::Skip => {
                let ticket = work.proposal.ticket;
                let retries = work.motion_retries;
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PreviewSkippedMotion {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC motion skip preview pressure")?;
                *state = Motion::SkipPreview;
                self.diagnostic(
                    "warn",
                    "game.npc_motion_skipped",
                    &format!(
                        "source {:#010x}, ticket {}, failed starts {}; advancing ACE-conditional motion through durable stage",
                        source.0, ticket, retries
                    ),
                );
            }
            Motion::SkipReady => {
                let (binding, workflow_version) = self
                    .npc
                    .coordinator
                    .binding(source)
                    .ok_or("NPC motion skip binding missing")?;
                let leases = work
                    .proposal
                    .context
                    .target
                    .and_then(|actor| {
                        self.online_saves
                            .baseline(actor.0)
                            .map(|(_, _, lease)| lease)
                    })
                    .into_iter()
                    .collect();
                let pending = freeze_service_completion(NpcServiceCompletionInput {
                    binding,
                    workflow_version,
                    world_epoch: self.bootstrap.world_owner.epoch(),
                    proposal: work.proposal.clone(),
                    checkpoint: work
                        .checkpoint
                        .as_ref()
                        .ok_or("NPC motion skip checkpoint missing")?
                        .clone(),
                    leases,
                })
                .map_err(|e| e.to_string())?;
                work.frozen = Some((
                    pending,
                    NpcDurableAdoption::SkippedMotion {
                        proposal: work.proposal.clone(),
                    },
                ));
                work.phase = Phase::Saving;
                *state = Motion::SkipSaving;
            }
            Motion::Running => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PollMotion {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC motion callback pressure")?;
                *state = Motion::Polling;
            }
            Motion::Held(error) => return Err(error.clone()),
            _ => {}
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use bace_gameplay_api::NpcContext;
    use tokio::time::Instant;

    #[test]
    fn transient_start_conflicts_reinspect_then_skip_without_lifecycle_hold() {
        let source = EntityId(0x8000_0001);
        let proposal = NpcProposal {
            ticket: 7,
            context: NpcContext {
                source,
                target: None,
                operation: 1,
            },
            effect: NpcEffect::Service(NpcOperation::Motion {
                target: false,
                motion: 0x1300_00aa,
                extent: 1.0,
                style: None,
                substyle: None,
            }),
        };
        let mut npc = NpcRuntime::new();
        let mut work = super::super::effects::Work::new(proposal);
        work.motion = Some(Motion::Inspecting);
        npc.work.insert(source, work);
        let online =
            crate::online_player_saves::OnlinePlayerSaveService::new(1, 1024, Instant::now())
                .unwrap();
        let conflict = bace_simulation::NpcServiceOutcome {
            correlation: 1,
            result: Err(bace_gameplay_api::NpcFailure::Conflict),
        };
        for attempt in 1..=MAX_STALE_MOTION_REPREPARES {
            accept(&mut npc, source, &conflict, &online).unwrap();
            let work = npc.work.get_mut(&source).unwrap();
            assert_eq!(work.motion_retries, attempt);
            if attempt == MAX_STALE_MOTION_REPREPARES {
                assert!(matches!(work.motion, Some(Motion::Skip)));
            } else {
                assert!(matches!(work.motion, Some(Motion::Inspect)));
                work.motion = Some(Motion::Inspecting);
            }
            assert!(npc.failure.is_none());
        }
    }
}
