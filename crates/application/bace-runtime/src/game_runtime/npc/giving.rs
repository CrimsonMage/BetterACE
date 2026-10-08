//! Give's turn, durable independent-chain admission, cold item factory, and
//! inventory receipt remain separate stages with the exact source ticket.
use super::effects::{Owner, Phase};
use super::*;
use crate::{
    npc_items::{NpcGivePlan, NpcItemJob, NpcItemResult, NpcItemWorker, PreparedNpcGift},
    npc_persistence::{NpcServiceAdmissionInput, freeze_service_admission},
};
pub(super) enum Gift {
    Begin,
    Beginning,
    Turn,
    Turning,
    Admission,
    Preview,
    AdmissionSaving,
    Plan,
    Cold {
        job: Arc<NpcItemJob>,
        submitted: bool,
    },
    Allocate(NpcGivePlan),
    Allocating,
    Built(Arc<PreparedNpcGift>),
    PreparingInventory(Arc<PreparedNpcGift>),
    Held(String),
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    source: EntityId,
    event: &NpcCoordinatorEvent,
) -> Result<bool, String> {
    let Some(work) = npc.work.get_mut(&source) else {
        return Ok(false);
    };
    if !matches!(work.owner, Owner::Player) || work.gift.is_none() {
        return Ok(false);
    }
    match event {
        NpcCoordinatorEvent::Command { outcome, .. } => match &outcome.result {
            Ok(R::GiveStarted { detached, turning }) => {
                work.gift = Some(if *detached {
                    Gift::Plan
                } else if *turning {
                    Gift::Turn
                } else {
                    Gift::Admission
                });
                work.phase = Phase::Preparing;
            }
            Ok(R::MovementProgress(done)) => {
                work.gift = Some(if *done { Gift::Admission } else { Gift::Turn })
            }
            Ok(R::Checkpoint(checkpoint)) if matches!(work.gift, Some(Gift::Preview)) => {
                work.checkpoint = Some(checkpoint.clone());
                work.phase = Phase::Ready;
            }
            Ok(R::Applied) if work.phase == Phase::Delivering => {
                work.gift = Some(if work.committed == Some(true) {
                    Gift::Plan
                } else {
                    Gift::Admission
                });
                work.phase = Phase::Preparing;
                work.committed = None;
                work.checkpoint = None;
            }
            Ok(R::Inventory(ticket)) => {
                let Some(Gift::PreparingInventory(gift)) = work.gift.take() else {
                    return Err("NPC Give inventory result phase".into());
                };
                work.gift = Some(Gift::Built(gift));
                work.owner = Owner::Inventory(ticket.clone());
                work.phase = Phase::Queued;
            }
            Err(error) => {
                let message = format!("NPC Give owner held: {error:?}");
                work.gift = Some(Gift::Held(message.clone()));
                npc.failure = Some(message);
            }
            _ => return Err("NPC Give owner result mismatch".into()),
        },
        NpcCoordinatorEvent::Durable { .. } | NpcCoordinatorEvent::Held { .. } => return Ok(false),
        _ => return Err("NPC Give unexpected event".into()),
    }
    Ok(true)
}
impl GameRuntime {
    pub(super) fn poll_npc_item_worker(&mut self) -> Result<(), String> {
        if self.npc.item_unexpected.is_some() {
            return Err("NPC item completion mismatch retained".into());
        }
        if let Some(worker) = &self.npc.item_worker
            && let Some(result) = worker.poll()?
        {
            let (source, ticket) = match result.job.as_ref() {
                NpcItemJob::Plan { source, ticket, .. }
                | NpcItemJob::Build { source, ticket, .. } => (*source, *ticket),
            };
            let Some(work) = self.npc.work.get_mut(&source) else {
                self.npc.item_unexpected = Some(result);
                return Err("NPC item source missing".into());
            };
            if work.proposal.ticket != ticket
                || !matches!(&work.gift,Some(Gift::Cold{job,submitted:true})if Arc::ptr_eq(job,&result.job))
            {
                self.npc.item_unexpected = Some(result);
                return Err("NPC item ticket mismatch".into());
            }
            work.gift = Some(match result.result {
                Ok(NpcItemResult::Plan(plan)) => Gift::Allocate(plan),
                Ok(NpcItemResult::Built(gift)) => Gift::Built(Arc::new(gift)),
                Err(error) => {
                    self.npc.failure = Some(error.clone());
                    Gift::Held(error)
                }
            });
        }
        if let Some((source, ticket, plan, result)) = ready(&mut self.npc.gift_ids) {
            let work = self
                .npc
                .work
                .get_mut(&source)
                .ok_or("NPC reserved GUID source missing")?;
            if work.proposal.ticket != ticket || !matches!(work.gift, Some(Gift::Allocating)) {
                return Err("NPC reserved GUID ticket mismatch".into());
            }
            work.gift = Some(match result {
                Ok(ids) if ids.len() == plan.roots => Gift::Cold {
                    job: Arc::new(NpcItemJob::Build {
                        source,
                        ticket,
                        actor: work
                            .proposal
                            .context
                            .target
                            .ok_or("NPC Give target missing")?,
                        plan,
                        ids: ids.into_iter().map(EntityId).collect(),
                    }),
                    submitted: false,
                },
                Ok(_) => Gift::Held("NPC GUID allocator count mismatch".into()),
                Err(error) => Gift::Held(error),
            });
        }
        Ok(())
    }
    pub(super) fn poll_npc_give(&mut self, source: EntityId) -> Result<(), String> {
        if self.npc.coordinator.busy(source) {
            return Ok(());
        }
        let work = self
            .npc
            .work
            .get_mut(&source)
            .ok_or("NPC Give source work missing")?;
        let NpcEffect::Service(NpcOperation::Give {
            template,
            count,
            palette,
            shade,
        }) = work.proposal.effect
        else {
            return Err("NPC Give opcode mismatch".into());
        };
        let gift = work.gift.get_or_insert(Gift::Begin);
        if work.phase == Phase::Ready && matches!(gift, Gift::Preview) {
            let (binding, workflow_version) = self
                .npc
                .coordinator
                .binding(source)
                .ok_or("NPC Give binding missing")?;
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
            let pending = freeze_service_admission(NpcServiceAdmissionInput {
                binding,
                stage: workflow_version as u64,
                world_epoch: self.bootstrap.world_owner.epoch(),
                workflow_version,
                proposal: work.proposal.clone(),
                post_delay: 0.0,
                committed_checkpoint: work
                    .checkpoint
                    .as_ref()
                    .ok_or("NPC Give admission preview missing")?
                    .clone(),
                leases,
            })
            .map_err(|e| e.to_string())?;
            work.frozen = Some((pending, crate::npc_service::NpcDurableAdoption::Player));
            work.phase = Phase::Saving;
            *gift = Gift::AdmissionSaving;
            return Ok(());
        }
        if matches!(work.phase, Phase::Saving | Phase::Delivering) {
            return Ok(());
        }
        work.phase = Phase::Preparing;
        match gift {
            Gift::Begin => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::BeginGive {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC Give turn queue pressure")?;
                *gift = Gift::Beginning;
            }
            Gift::Turn => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PollMovement {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC Give turn callback pressure")?;
                *gift = Gift::Turning;
            }
            Gift::Admission => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PreviewServiceAdmissionNow {
                            proposal: work.proposal.clone(),
                            post_delay: 0.0,
                        },
                    )
                    .map_err(|_| "NPC Give admission pressure")?;
                *gift = Gift::Preview;
            }
            Gift::Plan => {
                let definition = self
                    .npc
                    .definitions
                    .get(&source)
                    .ok_or("NPC Give pinned generation missing")?;
                let (binding, _) = self
                    .npc
                    .coordinator
                    .binding(source)
                    .ok_or("NPC Give source journal missing")?;
                if definition.content_hash != binding.content_generation {
                    return Err("NPC Give generation differs from source continuation".into());
                }
                *gift = Gift::Cold {
                    job: Arc::new(NpcItemJob::Plan {
                        source,
                        ticket: work.proposal.ticket,
                        generation: definition.generation.clone(),
                        template,
                        count,
                        palette,
                        shade,
                    }),
                    submitted: false,
                };
            }
            Gift::Cold { job, submitted } if !*submitted => {
                if self.npc.item_worker.is_none() {
                    self.npc.item_worker =
                        Some(NpcItemWorker::start(self.bootstrap.assets.clone())?);
                }
                if self
                    .npc
                    .item_worker
                    .as_ref()
                    .expect("started NPC item worker")
                    .submit(job.clone())
                    .is_ok()
                {
                    *submitted = true;
                }
            }
            Gift::Allocate(plan) if self.npc.gift_ids.is_none() => {
                let plan = plan.clone();
                let ticket = work.proposal.ticket;
                let store = self.bootstrap.store.clone();
                *gift = Gift::Allocating;
                self.npc.gift_ids = Some(Box::pin(async move {
                    let result = store
                        .allocate_dynamic_ids(plan.roots as u16)
                        .await
                        .map_err(|e| e.to_string());
                    (source, ticket, plan, result)
                }));
            }
            Gift::Built(prepared) => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PrepareInventoryBatch {
                            proposal: work.proposal.clone(),
                            items: prepared.items.clone(),
                            containers: prepared.containers.clone(),
                        },
                    )
                    .map_err(|_| "NPC Give inventory queue pressure")?;
                *gift = Gift::PreparingInventory(prepared.clone());
            }
            Gift::Held(error) => return Err(error.clone()),
            _ => {}
        }
        Ok(())
    }
}
