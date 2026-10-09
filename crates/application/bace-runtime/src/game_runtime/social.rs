//! One retained ingress action per authenticated session and one accepted output
//! pump. Database reads never authorize or mutate the simulation owner.
use super::*;
use crate::{
    chat_service::ChatApiService,
    gameplay_dispatch::{GameplayDispatch, decode_social_gameplay, decode_turbine_gameplay},
    social_lookup::{PendingSocialAction, SocialLookupAdmissionError, SocialLookupService},
    social_service::SocialService,
};
use bace_gameplay_api::{
    ActionContext,
    social::{
        AllegianceRequest, FellowshipRequest, SocialError, SocialIdentity, SocialOutcome,
        SocialRequest,
    },
};
use bace_session::{DispatchError, SessionState};
use bace_simulation::Command;
use bace_transport::ReceivedMessage;
use std::collections::BTreeSet;
use std::sync::mpsc::TrySendError;

#[derive(Clone)]
enum Action {
    Social(SocialRequest),
    Resolved(SocialRequest, Option<SocialIdentity>),
    Fellowship(FellowshipRequest),
    Allegiance(AllegianceRequest),
}
struct Pending {
    context: ActionContext,
    action: Action,
    submitted: bool,
}
impl Pending {
    fn from_command(command: &Command) -> Option<Self> {
        let (context, action) = match command {
            Command::Social { context, request } => (*context, Action::Social(request.clone())),
            Command::SocialResolved {
                context,
                request,
                identity,
            } => (
                *context,
                Action::Resolved(request.clone(), identity.clone()),
            ),
            Command::Fellowship { context, request } => {
                (*context, Action::Fellowship(request.clone()))
            }
            Command::Allegiance { context, request } => {
                (*context, Action::Allegiance(request.clone()))
            }
            _ => return None,
        };
        Some(Self {
            context,
            action,
            submitted: false,
        })
    }
    fn command(&self) -> Command {
        let context = self.context;
        match &self.action {
            Action::Social(request) => Command::Social {
                context,
                request: request.clone(),
            },
            Action::Resolved(request, identity) => Command::SocialResolved {
                context,
                request: request.clone(),
                identity: identity.clone(),
            },
            Action::Fellowship(request) => Command::Fellowship {
                context,
                request: request.clone(),
            },
            Action::Allegiance(request) => Command::Allegiance {
                context,
                request: request.clone(),
            },
        }
    }
}
pub(super) struct SocialRuntime {
    api: Option<ChatApiService>,
    lookup: SocialLookupService,
    output: SocialService,
    pending: BTreeMap<SessionKey, Pending>,
    lookup_keys: BTreeMap<bace_gameplay_api::SessionId, SessionKey>,
    failures: BTreeMap<SessionKey, SocialError>,
    unexpected: Option<SocialOutcome>,
}
impl SocialRuntime {
    /// Terminal adapter handoff, after the caller proves `has_pending() == false`.
    pub(super) fn into_shutdown(self) -> (Option<ChatApiService>, SocialLookupService) {
        (self.api, self.lookup)
    }
    pub(super) fn new(api: Option<ChatApiService>, lookup: SocialLookupService) -> Self {
        let publisher = api.as_ref().map(ChatApiService::publisher);
        Self {
            api,
            lookup,
            output: SocialService::new(publisher),
            pending: BTreeMap::new(),
            lookup_keys: BTreeMap::new(),
            failures: BTreeMap::new(),
            unexpected: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.lookup.has_pending()
            || self.output.has_pending()
            || !self.pending.is_empty()
            || self.unexpected.is_some()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SocialIngress {
    Accepted,
    Blocked,
    Unsupported,
}
impl GameRuntime {
    pub fn chat_api_address(&self) -> Option<std::net::SocketAddr> {
        self.social.api.as_ref().map(ChatApiService::local_address)
    }
    pub fn chat_feed_failures(&self) -> u64 {
        self.social.output.feed_failures()
    }
    pub fn social_failure(&self, key: SessionKey) -> Option<SocialError> {
        self.social.failures.get(&key).copied()
    }
    pub fn social_lookup_failure(&self, key: SessionKey) -> Option<&str> {
        self.social
            .lookup
            .failure(bace_gameplay_api::SessionId(key.generation))
    }
    pub fn retry_social_lookup(&mut self, key: SessionKey) -> Result<(), String> {
        self.social
            .lookup
            .retry(bace_gameplay_api::SessionId(key.generation))
    }
    pub(super) fn forget_social_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.social_ingress_blocked(key) {
            return Err("social action still pending during logout".into());
        }
        self.social.failures.remove(&key);
        self.social
            .lookup_keys
            .remove(&bace_gameplay_api::SessionId(key.generation));
        Ok(())
    }
    pub(super) fn social_ingress_blocked(&self, key: SessionKey) -> bool {
        self.social.pending.contains_key(&key)
            || self
                .social
                .lookup
                .pending_session(bace_gameplay_api::SessionId(key.generation))
    }
    pub(super) fn handle_social_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<SocialIngress, String> {
        if self.social_ingress_blocked(key) {
            return Ok(SocialIngress::Blocked);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(SocialIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(SocialIngress::Unsupported);
        };
        if !self.players.entered(loading.loaded.binding.actor) {
            return Ok(SocialIngress::Blocked);
        }
        if session.terminated || session.disconnected {
            return Ok(SocialIngress::Blocked);
        }
        let binding = loading.loaded.binding;
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("social authenticated binding mismatch".into());
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let Some(bytes) = message.bytes.get(..4) else {
            return Err("truncated gameplay opcode".into());
        };
        let opcode = u32::from_le_bytes(bytes.try_into().expect("four bytes"));
        let decoded = if opcode == bace_wire::opcode::GameMessageOpcode::TurbineChat.0 {
            decode_turbine_gameplay(
                SessionState::WorldConnected,
                context,
                &message.bytes,
                self.limits.message_bytes,
                4096,
            )
        } else if opcode == bace_wire::opcode::GameMessageOpcode::GameAction.0 {
            decode_social_gameplay(
                SessionState::WorldConnected,
                context,
                &message.bytes,
                self.limits.message_bytes,
                4096,
            )
        } else {
            return Ok(SocialIngress::Unsupported);
        };
        let mut decoded = match decoded {
            Ok(decoded) => decoded,
            Err(
                DispatchError::UnsupportedAction(_)
                | DispatchError::Wire(bace_wire::WireError::UnexpectedOpcode(_)),
            ) => return Ok(SocialIngress::Unsupported),
            Err(error) => return Err(format!("malformed social input: {error:?}")),
        };
        // All live gameplay adapters share reliable-message order, including
        // Turbine frames which contain no GameAction sequence of their own.
        match &mut decoded {
            GameplayDispatch::Simulation(command) => match command.as_mut() {
                Command::Social { context: c, .. }
                | Command::Fellowship { context: c, .. }
                | Command::Allegiance { context: c, .. } => *c = context,
                _ => return Err("social decoder produced unrelated owner command".into()),
            },
            GameplayDispatch::TargetQuery { context: c, .. }
            | GameplayDispatch::CorpseConsent { context: c, .. }
            | GameplayDispatch::SocialLookup { context: c, .. }
            | GameplayDispatch::StaffLine { context: c, .. }
            | GameplayDispatch::Map { context: c, .. } => *c = context,
        }
        match decoded {
            GameplayDispatch::CorpseConsent { context, request } => {
                self.handle_corpse_consent_dispatch(key, context, request)
            }
            GameplayDispatch::SocialLookup { context, request } => {
                match self
                    .social
                    .lookup
                    .begin(PendingSocialAction { context, request })
                {
                    Ok(()) => {
                        self.social.lookup_keys.insert(context.session, key);
                        Ok(SocialIngress::Accepted)
                    }
                    Err((SocialLookupAdmissionError::Capacity, _)) => Ok(SocialIngress::Blocked),
                    Err((error, _)) => Err(format!("social lookup admission: {error}")),
                }
            }
            GameplayDispatch::Simulation(command) => {
                if matches!(command.as_ref(), Command::Allegiance { .. })
                    && !self.allegiance_seed_ready()
                {
                    return Ok(SocialIngress::Blocked);
                }
                let pending =
                    Pending::from_command(&command).ok_or("unsupported social owner command")?;
                if self.social.pending.len() >= self.limits.sessions {
                    return Ok(SocialIngress::Blocked);
                }
                self.social.failures.remove(&key);
                self.social.pending.insert(key, pending);
                Ok(SocialIngress::Accepted)
            }
            dispatch @ (GameplayDispatch::StaffLine { .. }
            | GameplayDispatch::Map { .. }
            | GameplayDispatch::TargetQuery { .. }) => self.handle_staff_dispatch(key, dispatch),
        }
    }
    pub(super) fn poll_social_ingress(&mut self) -> Result<(), String> {
        if self.social.unexpected.is_some() {
            return Err("uncorrelated social outcome retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.social_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) =
                correlate_outcome(&mut self.social.pending, &mut self.social.failures, outcome)
            {
                self.social.unexpected = Some(*outcome);
                return Err("uncorrelated social outcome retained".into());
            }
        }
        self.social.lookup.poll(self.limits.work_per_poll)?;
        let input = self.simulation.input();
        let social = &mut self.social;
        social.lookup.flush(self.limits.work_per_poll, |command| {
            let Some(mut pending) = Pending::from_command(&command) else {
                return Err(Box::new(command));
            };
            let Some(key) = social.lookup_keys.get(&pending.context.session).copied() else {
                return Err(Box::new(command));
            };
            if social.pending.contains_key(&key) || social.pending.len() >= self.limits.sessions {
                return Err(Box::new(command));
            }
            match input.try_submit(command) {
                Ok(()) => {
                    pending.submitted = true;
                    social.pending.insert(key, pending);
                    social
                        .lookup_keys
                        .remove(&bace_gameplay_api::SessionId(key.generation));
                    Ok(())
                }
                Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) => {
                    Err(Box::new(command))
                }
            }
        });
        for pending in self
            .social
            .pending
            .values_mut()
            .filter(|p| !p.submitted)
            .take(self.limits.work_per_poll)
        {
            match input.try_submit(pending.command()) {
                Ok(()) => pending.submitted = true,
                Err(_) => break,
            }
        }
        Ok(())
    }
    pub(super) fn poll_social_outputs(&mut self) -> Result<(), String> {
        let limits = bace_replication::BatchLimits {
            max_messages: 64,
            max_bytes: self.limits.message_bytes,
            max_message_bytes: self.limits.message_bytes,
            max_string_bytes: 4096,
        };
        let awaiting_entry: BTreeSet<_> = self.players.awaiting_entry_actors().collect();
        let players = &mut self.players;
        let network = &self.network;
        self.social.output.pump_ready(
            &self.simulation,
            self.limits.work_per_poll,
            |recipient| !awaiting_entry.contains(&recipient),
            |recipient, event| players.project_social(recipient, event, limits),
            |command| {
                network.try_send(command).map_err(|error| match error {
                    TrySendError::Full(command) | TrySendError::Disconnected(command) => command,
                })
            },
        )?;
        if self.social.api.as_ref().is_some_and(|api| !api.healthy()) {
            return Err("chat API worker stopped unexpectedly".into());
        }
        Ok(())
    }
}

fn correlate_outcome(
    pending: &mut BTreeMap<SessionKey, Pending>,
    failures: &mut BTreeMap<SessionKey, SocialError>,
    outcome: SocialOutcome,
) -> Result<(), Box<SocialOutcome>> {
    let key = pending
        .iter()
        .find_map(|(key, p)| (p.context == outcome.context && p.submitted).then_some(*key));
    let Some(key) = key else {
        return Err(Box::new(outcome));
    };
    if outcome.retryable && outcome.result == Err(SocialError::Busy) {
        pending.get_mut(&key).expect("correlated action").submitted = false;
    } else {
        pending.remove(&key);
        if let Err(error) = outcome.result {
            failures.insert(key, error);
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "social_tests.rs"]
mod tests;
