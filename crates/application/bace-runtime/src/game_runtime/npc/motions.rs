//! NPC motion delivery consumes actual epoch/token callbacks after cold source
//! preparation; no elapsed-time estimate acknowledges an animation completion.
use super::effects::{Owner, Phase};
use super::*;
use crate::npc_motion_assets::{NpcMotionWork, NpcMotionWorker};
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
    Starting,
    Running,
    Polling,
    Held(String),
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
        Ok(R::MotionStarted(_)) if matches!(state, Motion::Starting) => *state = Motion::Running,
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
        Err(error) => {
            let message = format!("NPC motion owner held: {error:?}");
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
                *state = Motion::Starting;
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
