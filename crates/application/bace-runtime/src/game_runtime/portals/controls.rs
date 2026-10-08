//! Authenticated readiness is separate from the server's geometry-ready proof.
use super::*;
use crate::game_runtime::progression::ProgressionIngress;
use bace_simulation::{Command, PortalResolution, PortalResolutionCommand};
pub(super) struct Transit {
    pub operation: u64,
    pub view: bace_simulation::PortalAcceptedView,
    pub destination_ready: bool,
    pub materialized: bool,
}
impl PortalRuntime {
    fn accept_control_outcome(
        &mut self,
        outcome: PortalResolutionOutcome,
    ) -> Result<
        Option<(
            bace_gameplay_api::ActionContext,
            bace_interactions::PortalError,
        )>,
        String,
    > {
        if self.controls.get(&outcome.correlation) != Some(&outcome.resolution) {
            self.unmatched_resolution = Some(outcome);
            return Err("portal control correlation mismatch".into());
        }
        if outcome.result == Err(bace_interactions::PortalError::Capacity) {
            self.retry_controls.insert(outcome.correlation);
            return Ok(None);
        }
        if let Err(error) = outcome.result {
            if let PortalResolution::ClientReady { context, .. } = outcome.resolution {
                self.controls.remove(&outcome.correlation);
                self.retry_controls.remove(&outcome.correlation);
                return Ok(Some((context, error)));
            }
            self.unmatched_resolution = Some(outcome);
            return Err(format!("portal control rejected; retained: {error:?}"));
        }
        if let PortalResolution::DestinationReady {
            actor,
            operation,
            epoch,
        } = outcome.resolution
        {
            let transit = self
                .transits
                .get_mut(&actor)
                .ok_or("portal ready transit missing")?;
            if transit.operation != operation || transit.view.epoch != epoch {
                return Err("portal ready transit mismatch".into());
            }
            transit.destination_ready = true;
        }
        self.controls.remove(&outcome.correlation);
        Ok(None)
    }
}
impl GameRuntime {
    pub fn portal_control_failure(
        &self,
        key: SessionKey,
    ) -> Option<bace_interactions::PortalError> {
        self.portals
            .control_failures
            .get(&key)
            .map(|(_, error)| *error)
    }
    pub(super) fn accept_portal_control(
        &mut self,
        outcome: PortalResolutionOutcome,
    ) -> Result<(), String> {
        let initial = match &outcome.resolution {
            PortalResolution::ClientReady {
                context,
                operation: 0,
                ..
            } if outcome.result.is_ok() => Some(*context),
            _ => None,
        };
        let failure = self.portals.accept_control_outcome(outcome)?;
        if let Some(context) = initial {
            let (key, binding) = self
                .sessions
                .iter()
                .find_map(|(key, session)| {
                    session
                        .loading
                        .as_ref()
                        .filter(|loading| {
                            loading.loaded.binding.actor == context.actor
                                && loading.loaded.binding.account == context.account
                                && loading.loaded.binding.session == context.session
                        })
                        .map(|loading| (*key, loading.loaded.binding))
                })
                .ok_or("initial portal ready session missing")?;
            self.portals.initial_ready.insert(key, binding);
        }
        if let Some((context, error)) = failure
            && let Some(key) = self.sessions.iter().find_map(|(key, s)| {
                s.loading
                    .as_ref()
                    .filter(|l| {
                        l.loaded.binding.actor == context.actor
                            && l.loaded.binding.account == context.account
                            && l.loaded.binding.session == context.session
                    })
                    .map(|_| *key)
            })
        {
            self.portals.control_failures.insert(key, (context, error));
        }
        Ok(())
    }
    pub(super) fn poll_portal_controls(&mut self) -> Result<(), String> {
        self.portals.initial_ready.retain(|key, binding| {
            self.sessions
                .get(key)
                .and_then(|session| session.loading.as_ref())
                .is_some_and(|loading| loading.loaded.binding == *binding)
        });
        self.portals.initial_materialized.retain(|key, binding| {
            self.sessions
                .get(key)
                .and_then(|session| session.loading.as_ref())
                .is_some_and(|loading| loading.loaded.binding == *binding)
        });
        self.portals.control_failures.retain(|key, (context, _)| {
            self.sessions
                .get(key)
                .and_then(|s| s.loading.as_ref())
                .is_some_and(|l| {
                    l.loaded.binding.actor == context.actor
                        && l.loaded.binding.session == context.session
                })
        });

        if let Some(correlation) = self.portals.retry_controls.first().copied() {
            let resolution = self
                .portals
                .controls
                .get(&correlation)
                .ok_or("portal retry control missing")?
                .clone();
            match self
                .simulation
                .input()
                .try_submit(Command::PortalResolution(PortalResolutionCommand {
                    correlation,
                    resolution,
                })) {
                Ok(()) => {
                    self.portals.retry_controls.remove(&correlation);
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => return Ok(()),
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("portal retry channel closed".into());
                }
            }
        }

        self.portals.transits.retain(|actor, t| {
            self.portals.detaching.contains_key(actor)
                || !t.materialized
                || self.portals.controls.values().any(|c| c.actor() == *actor)
        });
        if self.portals.controls.len() >= 64 {
            return Ok(());
        }
        let ready =
            self.portals.transits.iter().find_map(|(actor, t)| {
                (!self.portals.detaching.contains_key(actor)
                    && !t.destination_ready
                    && !self.portals.controls.values().any(
                        |c| matches!(c,PortalResolution::DestinationReady{actor:a,..}if a==actor),
                    )
                    && self.world.as_ref().is_some_and(|w| {
                        w.regions
                            .prepared_region((t.view.position.cell >> 16) as u16)
                            .is_some()
                    }))
                .then_some((*actor, t.operation, t.view.epoch))
            });
        if let Some((actor, operation, epoch)) = ready {
            let correlation = self.token()?;
            let resolution = PortalResolution::DestinationReady {
                actor,
                operation,
                epoch,
            };
            match self
                .simulation
                .input()
                .try_submit(Command::PortalResolution(PortalResolutionCommand {
                    correlation,
                    resolution: resolution.clone(),
                })) {
                Ok(()) => {
                    self.portals.controls.insert(correlation, resolution);
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("portal destination ready channel closed".into());
                }
            }
        }
        Ok(())
    }
    pub(in crate::game_runtime) fn handle_portal_control(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<ProgressionIngress, String> {
        let Some(session) = self.sessions.get(&key) else {
            return Ok(ProgressionIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        if self.portals.detaching.contains_key(&binding.actor) {
            return Ok(ProgressionIngress::Blocked);
        }
        if !self.players.entered(binding.actor) {
            return Ok(ProgressionIngress::Unsupported);
        }
        let request =
            match bace_wire::WorldControlRequest::decode(&message.bytes, self.limits.message_bytes)
            {
                Ok(request) => request,
                Err(bace_wire::WireError::UnexpectedOpcode(_)) => {
                    return Ok(ProgressionIngress::Unsupported);
                }
                Err(e) => return Err(format!("portal control input: {e:?}")),
            };
        if request.action != bace_wire::WorldControlAction::LoginComplete {
            return Ok(ProgressionIngress::Unsupported);
        }
        if self.portals.controls.len()>=64||self.portals.controls.values().any(|c|matches!(c,PortalResolution::ClientReady{context,..}if context.actor==binding.actor)){return Ok(ProgressionIngress::Blocked);}
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("portal control binding mismatch".into());
        }
        let context = ready_context(binding, message.sequence, request)?;
        let (operation, epoch) = self
            .portals
            .transits
            .get(&binding.actor)
            .map_or((0, 0), |t| (t.operation, t.view.epoch));
        if operation == 0 && self.portals.initial_ready.get(&key) == Some(&binding) {
            return Ok(ProgressionIngress::Accepted);
        }
        let resolution = PortalResolution::ClientReady {
            context,
            operation,
            epoch,
        };
        let correlation = self.token()?;
        match self
            .simulation
            .input()
            .try_submit(Command::PortalResolution(PortalResolutionCommand {
                correlation,
                resolution: resolution.clone(),
            })) {
            Ok(()) => {
                self.portals.controls.insert(correlation, resolution);
                Ok(ProgressionIngress::Accepted)
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(ProgressionIngress::Blocked),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("portal control owner channel closed".into())
            }
        }
    }
}

fn ready_context(
    binding: bace_gameplay_api::CharacterBinding,
    reliable_sequence: u32,
    request: bace_wire::WorldControlRequest,
) -> Result<bace_gameplay_api::ActionContext, String> {
    if request.action != bace_wire::WorldControlAction::LoginComplete
        || request.action_sequence.is_none()
    {
        return Err("portal ready envelope missing".into());
    }
    Ok(bace_gameplay_api::ActionContext {
        actor: binding.actor,
        account: binding.account,
        session: binding.session,
        sequence: reliable_sequence,
    })
}
#[cfg(test)]
mod tests;
