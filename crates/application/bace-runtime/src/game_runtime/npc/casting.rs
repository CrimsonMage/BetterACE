//! Retained NPC server-cast preparation. Source epoch, scalar revision and actual
//! motion state fence the immutable cold program before domain admission.
use super::effects::{Owner, Phase};
use super::*;
use crate::server_magic_assets::{PreparedServerMagic, ServerMagicWork, ServerMagicWorker};
#[derive(Clone)]
pub(super) struct Fence {
    epoch: u16,
    revision: u64,
    motion: Option<bace_motion::SourceMotionState>,
    qualities: bace_entity::EntityProperties,
}
pub(super) enum Cast {
    Inspect,
    Inspecting,
    Captured(Fence),
    Cold {
        fence: Fence,
        request: Arc<ServerMagicWork>,
        submitted: bool,
    },
    Install {
        fence: Fence,
        source: Arc<bace_content::WeenieV1>,
        ready: Box<PreparedServerMagic>,
    },
    Registering,
    Start,
    Starting,
    Running,
    Polling,
    Held(String),
}
pub(super) fn accept(
    npc: &mut NpcRuntime,
    source: EntityId,
    outcome: &bace_simulation::NpcServiceOutcome,
) -> Result<(), String> {
    let work = npc.work.get_mut(&source).ok_or("NPC cast owner missing")?;
    let cast = work.cast.as_mut().ok_or("NPC cast preparation missing")?;
    match &outcome.result {
        Ok(R::CastSource {
            epoch,
            property_revision,
            motion,
            qualities,
            ..
        }) if matches!(cast, Cast::Inspecting) => {
            *cast = Cast::Captured(Fence {
                epoch: *epoch,
                revision: *property_revision,
                motion: *motion,
                qualities: qualities.clone(),
            })
        }
        Ok(R::Applied) if matches!(cast, Cast::Registering) => *cast = Cast::Start,
        Ok(R::CastStarted(_)) if matches!(cast, Cast::Starting) => *cast = Cast::Running,
        Ok(R::CastProgress(None)) if matches!(cast, Cast::Polling) => *cast = Cast::Running,
        Ok(R::CastProgress(Some(outcome))) if matches!(cast, Cast::Polling) => {
            work.owner = Owner::Cast(outcome.clone());
            work.phase = Phase::Queued;
            work.cast = None;
        }
        Err(error) => {
            let message = format!("NPC cast preparation/admission: {error:?}");
            *cast = Cast::Held(message.clone());
            npc.failure = Some(message);
        }
        _ => return Err("NPC cast phase/result mismatch".into()),
    }
    Ok(())
}
impl GameRuntime {
    pub fn retry_npc_cast(&mut self, source: EntityId) -> Result<(), String> {
        let work = self.npc.work.get_mut(&source).ok_or("NPC cast missing")?;
        if !matches!(work.cast, Some(Cast::Held(_))) {
            return Err("NPC cast is not held".into());
        }
        work.cast = Some(Cast::Inspect);
        self.npc.failure = None;
        Ok(())
    }
    pub(super) fn poll_npc_cast_worker(&mut self) -> Result<(), String> {
        if self.npc.server_magic_unexpected.is_some() {
            return Err("unmatched NPC server spell result retained".into());
        }
        if let Some(worker) = &mut self.npc.server_magic
            && let Some(result) = worker.poll()?
        {
            let source = result.work.actor;
            let Some(work) = self.npc.work.get_mut(&source) else {
                self.npc.server_magic_unexpected = Some(result);
                return Err("NPC spell result source missing".into());
            };
            let Some(Cast::Cold {
                fence,
                request,
                submitted: true,
            }) = &work.cast
            else {
                self.npc.server_magic_unexpected = Some(result);
                return Err("NPC spell result phase mismatch".into());
            };
            if !Arc::ptr_eq(request, &result.work) || result.work.token != work.proposal.ticket {
                self.npc.server_magic_unexpected = Some(result);
                return Err("NPC spell result ticket mismatch".into());
            }
            let fence = fence.clone();
            match result.result {
                Ok(ready) => {
                    work.cast = Some(Cast::Install {
                        fence,
                        source: result.work.source.clone(),
                        ready: Box::new(ready),
                    })
                }
                Err(error) => {
                    work.cast = Some(Cast::Held(error.clone()));
                    self.npc.failure = Some(error);
                }
            }
        }
        Ok(())
    }
    pub(super) fn poll_npc_cast(&mut self, source: EntityId) -> Result<(), String> {
        let destination = self
            .npc
            .work
            .get(&source)
            .and_then(|w| w.cast.as_ref())
            .and_then(|cast| match cast {
                Cast::Install { ready, .. } => match ready.spell.definition.spell.spell.effect {
                    bace_magic::SpellEffect::Portal(bace_magic::PortalEffect::Sending {
                        cell,
                        ..
                    })
                    | bace_magic::SpellEffect::FellowshipPortal(
                        bace_magic::PortalEffect::Sending { cell, .. },
                    ) => Some(cell),
                    _ => None,
                },
                _ => None,
            });
        if let Some(cell) = destination
            && !self.ensure_npc_region(source, cell)?
        {
            return Ok(());
        }
        if self.npc.coordinator.busy(source) {
            return Ok(());
        }
        let work = self
            .npc
            .work
            .get_mut(&source)
            .ok_or("NPC cast source work missing")?;
        let NpcEffect::Service(NpcOperation::Cast { spell, instant, .. }) = work.proposal.effect
        else {
            return Err("NPC cast source opcode".into());
        };
        let state = work.cast.get_or_insert(Cast::Inspect);
        work.phase = Phase::Preparing;
        match state {
            Cast::Inspect => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::InspectCast {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC cast inspection pressure")?;
                *state = Cast::Inspecting;
            }
            Cast::Captured(fence) => {
                let definition = self
                    .npc
                    .definitions
                    .get(&source)
                    .ok_or("NPC cast immutable source unavailable")?;
                let (binding, _) = self
                    .npc
                    .coordinator
                    .binding(source)
                    .ok_or("NPC cast source journal missing")?;
                if definition.content_hash != binding.content_generation
                    || definition.source.program_hash != binding.program_hash
                {
                    return Err("NPC caster generation differs from pinned continuation".into());
                }
                let authored = crate::npc_recovery::overlay_npc_qualities(
                    &definition.source.authored,
                    &fence.qualities,
                )?;
                let request = Arc::new(ServerMagicWork {
                    token: work.proposal.ticket,
                    actor: source,
                    source_revision: fence.revision,
                    source: Arc::new(authored),
                    generation: definition.generation.clone(),
                    spell,
                    instant,
                    motion: fence.motion,
                });
                *state = Cast::Cold {
                    fence: fence.clone(),
                    request,
                    submitted: false,
                };
            }
            Cast::Cold {
                request, submitted, ..
            } if !*submitted => {
                if self.npc.server_magic.is_none() {
                    self.npc.server_magic =
                        Some(ServerMagicWorker::start(self.bootstrap.assets.clone())?);
                }
                if self
                    .npc
                    .server_magic
                    .as_mut()
                    .expect("started NPC caster preparation")
                    .submit(request.clone())
                    .is_ok()
                {
                    *submitted = true;
                }
            }
            Cast::Install {
                fence,
                source: authored,
                ready,
            } => {
                let action = A::RegisterCast {
                    proposal: work.proposal.clone(),
                    epoch: fence.epoch,
                    property_revision: fence.revision,
                    motion: fence.motion,
                    source: authored.clone(),
                    definition: ready.spell.definition.clone(),
                    shapes: ready.spell.projectile.clone().into_iter().collect(),
                };
                self.npc
                    .coordinator
                    .enqueue(source, action)
                    .map_err(|_| "NPC spell install pressure")?;
                *state = Cast::Registering;
            }
            Cast::Start => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::BeginCast {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC cast begin pressure")?;
                *state = Cast::Starting;
            }
            Cast::Running => {
                self.npc
                    .coordinator
                    .enqueue(
                        source,
                        A::PollCast {
                            proposal: work.proposal.clone(),
                        },
                    )
                    .map_err(|_| "NPC cast completion pressure")?;
                *state = Cast::Polling;
            }
            Cast::Held(error) => return Err(error.clone()),
            _ => {}
        }
        Ok(())
    }
}
