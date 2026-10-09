//! Bounded lifecycle ingress. Unaccepted unsupported packets close only their
//! peer; accepted gameplay and durable receipts remain with their actual owners.
use super::*;
use crate::authentication_pool::{
    AuthenticationCompletion, AuthenticationFailure, AuthenticationJob,
};
use bace_wire::{CharacterLifecycleAction, CharacterLifecycleRequest};
use std::sync::mpsc::TrySendError;
impl GameRuntime {
    pub(super) fn poll_ingress(&mut self) -> Result<(), String> {
        self.poll_authentication()?;
        for _ in 0..self.limits.work_per_poll {
            if self.input.len() >= self.limits.messages
                || self.input_bytes >= self.limits.message_bytes
            {
                break;
            }
            let Ok(event) = self.network.events().try_recv() else {
                break;
            };
            let size = event_bytes(&event);
            self.input_bytes = self
                .input_bytes
                .checked_add(size)
                .ok_or("network retained byte overflow")?;
            self.input.push_back(event);
            // One bounded network message may cross the aggregate high-water;
            // retain it and stop reads rather than dropping an accepted payload.
        }
        let count = self.input.len().min(self.limits.work_per_poll);
        let mut blocked = std::collections::BTreeSet::new();
        for _ in 0..count {
            let Some(event) = self.input.pop_front() else {
                break;
            };
            let size = event_bytes(&event);
            let key = event_key(&event);
            if blocked.contains(&key)
                && !matches!(
                    event,
                    NetworkEvent::Terminated { .. } | NetworkEvent::ReliableBatchAdmission { .. }
                )
                && !priority_control(&event, self.limits.message_bytes)
            {
                self.input.push_back(event);
                continue;
            }
            match self.handle_event(event) {
                Ok(()) => self.input_bytes -= size,
                Err(event) => {
                    blocked.insert(key);
                    self.input.push_back(*event);
                }
            }
        }
        Ok(())
    }
    fn poll_authentication(&mut self) -> Result<(), String> {
        if self.pending_auth.is_none() {
            let mut future = std::pin::pin!(self.authentication.next());
            if let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                self.pending_auth = result;
            }
        }
        let Some(completion) = self.pending_auth.take() else {
            return Ok(());
        };
        let AuthenticationCompletion { key, result } = completion;
        if self.auth_cancelled.remove(&key) {
            self.auth_inflight.remove(&key);
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages || self.players.io_busy() {
            self.pending_auth = Some(AuthenticationCompletion { key, result });
            return Ok(());
        }
        self.auth_inflight.remove(&key);
        match result {
            Ok(account) => {
                if self.draining || self.sessions.len() >= self.limits.sessions {
                    self.network_output
                        .push_back(NetworkCommand::Terminate { key });
                    return Ok(());
                }
                if let Err(error) = self.players.authenticated(key, &account) {
                    self.pending_auth = Some(AuthenticationCompletion {
                        key,
                        result: Ok(account),
                    });
                    return Err(error);
                }
                self.network_output
                    .push_back(NetworkCommand::Authenticated {
                        key,
                        account_id: account.id,
                    });
                self.sessions.insert(
                    key,
                    Session {
                        account,
                        connected: false,
                        closing: false,
                        terminated: false,
                        disconnected: false,
                        loading: None,
                        failure: None,
                    },
                );
            }
            Err(AuthenticationFailure::Banned {
                seconds_remaining,
                reason,
            }) => {
                let bytes = bace_wire::AccountControl::Banned {
                    seconds_remaining,
                    reason: Some(&reason),
                }
                .encode();
                match bytes {
                    Ok(bytes) => self
                        .network_output
                        .push_back(NetworkCommand::RejectBanned { key, bytes }),
                    Err(_) => self
                        .network_output
                        .push_back(NetworkCommand::Terminate { key }),
                }
            }
            Err(_) => self
                .network_output
                .push_back(NetworkCommand::Terminate { key }),
        }
        Ok(())
    }
    fn handle_event(&mut self, event: NetworkEvent) -> Result<(), Box<NetworkEvent>> {
        self.handle_event_inner(event).map_err(Box::new)
    }
    #[expect(
        clippy::result_large_err,
        reason = "internal no-allocation routing retains the exact bounded event; public ownership boundary boxes rejection"
    )]
    fn handle_event_inner(&mut self, event: NetworkEvent) -> Result<(), NetworkEvent> {
        match event {
            NetworkEvent::ReliableBatchAdmission {
                key,
                correlation,
                accepted,
            } => {
                if self.accept_ddd_admission(key, correlation, accepted) {
                    return Ok(());
                }
                if self.reliable_admissions.len() >= self.limits.messages {
                    return Err(NetworkEvent::ReliableBatchAdmission {
                        key,
                        correlation,
                        accepted,
                    });
                }
                self.reliable_admissions
                    .push_back((key, correlation, accepted));
                Ok(())
            }
            NetworkEvent::Login { key, request } => {
                if self.draining {
                    if self.network_output.len() >= self.limits.messages {
                        return Err(NetworkEvent::Login { key, request });
                    }
                    self.network_output
                        .push_back(NetworkCommand::Terminate { key });
                    return Ok(());
                }
                if self.auth_inflight.len() >= self.limits.sessions {
                    return Err(NetworkEvent::Login { key, request });
                }
                match self
                    .authentication
                    .try_submit(AuthenticationJob { key, request })
                {
                    Ok(()) => {
                        self.auth_inflight.insert(key);
                        Ok(())
                    }
                    Err(error) => {
                        let job = error.into_inner();
                        Err(NetworkEvent::Login {
                            key: job.key,
                            request: job.request,
                        })
                    }
                }
            }
            NetworkEvent::Connected { key } => {
                let Some(session) = self.sessions.get_mut(&key) else {
                    return Err(NetworkEvent::Connected { key });
                };
                if !session.connected && !self.shard.accepts(session.account.access_level) {
                    if self.network_output.len() >= self.limits.messages {
                        return Err(NetworkEvent::Connected { key });
                    }
                    let bytes = bace_wire::CharacterReply::Error(
                        bace_wire::opcode::CharacterError::LogonServerFull,
                    )
                    .encode()
                    .expect("constant source response");
                    self.network_output
                        .push_back(NetworkCommand::TerminateAfterFlush {
                            key,
                            queue: 9,
                            bytes,
                        });
                    session.connected = true;
                    session.closing = true;
                    session.terminated = true;
                    return Ok(());
                }
                if !session.connected {
                    session.connected = true;
                    self.login_queue.push_back((key, PlayerIoAction::Roster));
                }
                Ok(())
            }
            NetworkEvent::Message { key, message } => {
                match self.handle_ddd_message(key, &message) {
                    Ok(ddd::DddIngress::Accepted) => return Ok(()),
                    Ok(ddd::DddIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(ddd::DddIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .ok_or_else(|| NetworkEvent::Message {
                                key,
                                message: message.clone(),
                            })?
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_portal_control(key, &message) {
                    Ok(progression::ProgressionIngress::Accepted) => return Ok(()),
                    Ok(progression::ProgressionIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(progression::ProgressionIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                let Some(session) = self.sessions.get(&key) else {
                    return Err(NetworkEvent::Message { key, message });
                };
                if session.terminated || self.draining {
                    return Ok(());
                }
                match self.handle_movement_message(key, &message) {
                    Ok(movement::MovementIngress::Accepted) => return Ok(()),
                    Ok(movement::MovementIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(movement::MovementIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_priority_cast_cancel(key, &message) {
                    Ok(movement::MovementIngress::Accepted) => return Ok(()),
                    Ok(movement::MovementIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(movement::MovementIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_creation_message(key, &message) {
                    Ok(creation::CreationIngress::Accepted) => return Ok(()),
                    Ok(creation::CreationIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(creation::CreationIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_combat_message(key, &message) {
                    Ok(movement::MovementIngress::Accepted) => return Ok(()),
                    Ok(movement::MovementIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(movement::MovementIngress::Unsupported) => {}
                    Err(error) => {
                        self.failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                let session = self.sessions.get(&key).expect("retained session");
                if session.failure.is_some()
                    || (self.social_ingress_blocked(key) || self.staff_ingress_blocked(key))
                    || self.progression_ingress_blocked(key)
                    || self.inventory_ingress_blocked(key)
                    || self.vendor_ingress_blocked(key)
                    || self.crafting_ingress_blocked(key)
                    || self.skill_device_ingress_blocked(key)
                    || self.attribute_transfer_ingress_blocked(key)
                    || self.pet_ingress_blocked(key)
                    || self.creation_ingress_blocked(key)
                    || self.magic_ingress_blocked(key)
                    || self.combat_ingress_blocked(key)
                    || self.npc_session_pending(key)
                    || self.death_session_pending(key)
                    || self.corpse_access_ingress_blocked(key)
                    || self.recall_ingress_blocked(key)
                {
                    return Err(NetworkEvent::Message { key, message });
                }
                match self.handle_recall_message(key, &message) {
                    Ok(progression::ProgressionIngress::Accepted) => return Ok(()),
                    Ok(progression::ProgressionIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(progression::ProgressionIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_magic_message(key, &message) {
                    Ok(magic::MagicIngress::Accepted) => return Ok(()),
                    Ok(magic::MagicIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(magic::MagicIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_binding_message(key, &message) {
                    Ok(progression::ProgressionIngress::Accepted) => return Ok(()),
                    Ok(progression::ProgressionIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(progression::ProgressionIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_corpse_use_message(key, &message) {
                    Ok(progression::ProgressionIngress::Accepted) => return Ok(()),
                    Ok(progression::ProgressionIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(progression::ProgressionIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_vendor_message(key, &message) {
                    Ok(inventory::InventoryIngress::Accepted) => return Ok(()),
                    Ok(inventory::InventoryIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(inventory::InventoryIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                match self.handle_npc_message(key, &message) {
                    Ok(inventory::InventoryIngress::Accepted) => return Ok(()),
                    Ok(inventory::InventoryIngress::Blocked) => {
                        return Err(NetworkEvent::Message { key, message });
                    }
                    Ok(inventory::InventoryIngress::Unsupported) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .expect("retained session")
                            .failure = Some(error);
                        return Err(NetworkEvent::Message { key, message });
                    }
                }
                let decoded = CharacterLifecycleRequest::decode(
                    &message.bytes,
                    self.limits.message_bytes,
                    1024,
                );
                if matches!(
                    decoded.as_ref().map(|request| &request.action),
                    Ok(CharacterLifecycleAction::EnterWorld { .. })
                ) && self
                    .dat
                    .as_ref()
                    .is_some_and(|dat| !dat.ready_for_world(key))
                {
                    return match self.reject_ddd(key, "DAT negotiation is not complete") {
                        Ok(ddd::DddIngress::Accepted) => Ok(()),
                        Ok(ddd::DddIngress::Blocked) => Err(NetworkEvent::Message { key, message }),
                        Ok(ddd::DddIngress::Unsupported) | Err(_) => {
                            Err(NetworkEvent::Message { key, message })
                        }
                    };
                }
                let session = self.sessions.get(&key).expect("retained session");
                match decoded.map(|r| r.action) {
                    Ok(CharacterLifecycleAction::EnterWorldRequest) => {
                        if self.network_output.len() >= self.limits.messages {
                            return Err(NetworkEvent::Message { key, message });
                        }
                        let bytes = bace_wire::CharacterReply::WorldServerReady
                            .encode()
                            .expect("constant source response");
                        self.network_output.push_back(NetworkCommand::Send {
                            key,
                            queue: 9,
                            bytes,
                        });
                        Ok(())
                    }
                    Ok(CharacterLifecycleAction::EnterWorld {
                        character_id,
                        account,
                    }) => {
                        let loading = self
                            .sessions
                            .values()
                            .filter(|s| {
                                s.loading
                                    .as_ref()
                                    .is_some_and(|l| l.phase != lifecycle::Phase::Entered)
                            })
                            .count();
                        if !self.shard.accepts(session.account.access_level) {
                            if self.network_output.len() >= self.limits.messages {
                                return Err(NetworkEvent::Message { key, message });
                            }
                            let bytes = bace_wire::CharacterReply::Error(
                                bace_wire::opcode::CharacterError::LogonServerFull,
                            )
                            .encode()
                            .expect("constant source response");
                            self.network_output
                                .push_back(NetworkCommand::TerminateAfterFlush {
                                    key,
                                    queue: 9,
                                    bytes,
                                });
                            let session = self.sessions.get_mut(&key).expect("matched session");
                            session.terminated = true;
                            session.closing = true;
                            return Ok(());
                        }
                        if self.draining
                            || loading
                                + self
                                    .login_queue
                                    .iter()
                                    .filter(|(_, action)| matches!(action, PlayerIoAction::Load(_)))
                                    .count()
                                + usize::from(self.login_key.is_some())
                                >= self.limits.loading
                            || session.loading.is_some()
                            || self.login_queue.iter().any(|(k, _)| *k == key)
                            || self.login_key == Some(key)
                        {
                            return Err(NetworkEvent::Message { key, message });
                        }
                        if account != session.account.name.as_str() {
                            self.sessions.get_mut(&key).expect("session").failure =
                                Some("character account mismatch".into());
                            return Ok(());
                        }
                        self.login_queue
                            .push_back((key, PlayerIoAction::Load(character_id)));
                        Ok(())
                    }
                    Ok(CharacterLifecycleAction::LogOff) => {
                        self.sessions.get_mut(&key).expect("session").terminated = true;
                        Ok(())
                    }
                    _ => match self.handle_progression_message(key, &message) {
                        Ok(progression::ProgressionIngress::Accepted) => Ok(()),
                        Ok(progression::ProgressionIngress::Blocked) => {
                            Err(NetworkEvent::Message { key, message })
                        }
                        Err(error) => {
                            self.sessions.get_mut(&key).expect("session").failure = Some(error);
                            Err(NetworkEvent::Message { key, message })
                        }
                        Ok(progression::ProgressionIngress::Unsupported) => match self
                            .handle_social_message(key, &message)
                        {
                            Ok(social::SocialIngress::Accepted) => Ok(()),
                            Ok(social::SocialIngress::Blocked) => {
                                Err(NetworkEvent::Message { key, message })
                            }
                            Ok(social::SocialIngress::Unsupported) => {
                                match self.handle_pet_message(key, &message) {
                                    Ok(inventory::InventoryIngress::Accepted) => return Ok(()),
                                    Ok(inventory::InventoryIngress::Blocked) => {
                                        return Err(NetworkEvent::Message { key, message });
                                    }
                                    Ok(inventory::InventoryIngress::Unsupported) => {}
                                    Err(error) => {
                                        self.sessions.get_mut(&key).expect("session").failure =
                                            Some(error);
                                        return Err(NetworkEvent::Message { key, message });
                                    }
                                }
                                match self.handle_attribute_transfer_message(key, &message) {
                                    Ok(inventory::InventoryIngress::Accepted) => return Ok(()),
                                    Ok(inventory::InventoryIngress::Blocked) => {
                                        return Err(NetworkEvent::Message { key, message });
                                    }
                                    Ok(inventory::InventoryIngress::Unsupported) => {}
                                    Err(error) => {
                                        self.sessions.get_mut(&key).expect("session").failure =
                                            Some(error);
                                        return Err(NetworkEvent::Message { key, message });
                                    }
                                }
                                match self.handle_inventory_message(key, &message) {
                                    Ok(inventory::InventoryIngress::Accepted) => Ok(()),
                                    Ok(inventory::InventoryIngress::Blocked) => {
                                        Err(NetworkEvent::Message { key, message })
                                    }
                                    Ok(inventory::InventoryIngress::Unsupported) => match self
                                        .handle_skill_device_message(key, &message)
                                    {
                                        Ok(inventory::InventoryIngress::Accepted) => Ok(()),
                                        Ok(inventory::InventoryIngress::Blocked) => {
                                            Err(NetworkEvent::Message { key, message })
                                        }
                                        Err(error) => {
                                            if let Some(s) = self.sessions.get_mut(&key) {
                                                s.failure = Some(error);
                                            }
                                            Err(NetworkEvent::Message { key, message })
                                        }
                                        Ok(inventory::InventoryIngress::Unsupported) => {
                                            match self.handle_crafting_message(key, &message) {
                                                Ok(crafting::CraftingIngress::Accepted) => Ok(()),
                                                Ok(crafting::CraftingIngress::Blocked) => {
                                                    Err(NetworkEvent::Message { key, message })
                                                }
                                                Ok(crafting::CraftingIngress::Unsupported) => {
                                                    self.sessions
                                                        .get_mut(&key)
                                                        .expect("session")
                                                        .terminated = true;
                                                    Ok(())
                                                }
                                                Err(error) => {
                                                    self.sessions
                                                        .get_mut(&key)
                                                        .expect("session")
                                                        .failure = Some(error);
                                                    Err(NetworkEvent::Message { key, message })
                                                }
                                            }
                                        }
                                    },
                                    Err(error) => {
                                        self.sessions.get_mut(&key).expect("session").failure =
                                            Some(error);
                                        Err(NetworkEvent::Message { key, message })
                                    }
                                }
                            }
                            Err(error) => {
                                self.sessions.get_mut(&key).expect("session").failure = Some(error);
                                Err(NetworkEvent::Message { key, message })
                            }
                        },
                    },
                }
            }
            NetworkEvent::Terminated { key, reason } => {
                if let Some(session) = self.sessions.get_mut(&key) {
                    session.terminated = true;
                    session.disconnected = true;
                    if let Some(dat) = self.dat.as_mut() {
                        dat.forget(key);
                    }
                    Ok(())
                } else if self.network_output.len() < self.limits.messages {
                    if self.auth_inflight.contains(&key) {
                        self.auth_cancelled.insert(key);
                    }
                    self.network_output
                        .push_back(NetworkCommand::DrainCompleted { key });
                    Ok(())
                } else {
                    Err(NetworkEvent::Terminated { key, reason })
                }
            }
            NetworkEvent::CommandRejected { key } => {
                if let Some(session) = self.sessions.get_mut(&key)
                    && !session.disconnected
                    && !session.closing
                {
                    session.failure = Some("network rejected trusted lifecycle command".into());
                }
                Ok(())
            }
        }
    }
    pub(super) fn flush_output(&mut self) {
        for _ in 0..self.limits.work_per_poll {
            let Some(command) = self.network_output.pop_front() else {
                break;
            };
            if let Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) =
                self.network.try_send(command)
            {
                self.network_output.push_front(command);
                break;
            }
        }
    }
}
fn event_bytes(event: &NetworkEvent) -> usize {
    match event {
        NetworkEvent::Message { message, .. } => message.bytes.len(),
        NetworkEvent::Login { request, .. } => {
            request.account.len()
                + request.client_version.len()
                + request.account_to_login_as.len()
                + match &request.credential {
                    bace_wire::LoginCredential::Password(p)
                    | bace_wire::LoginCredential::GlsTicket(p) => p.len(),
                    _ => 0,
                }
        }
        _ => 0,
    }
}

fn event_key(event: &NetworkEvent) -> SessionKey {
    match event {
        NetworkEvent::ReliableBatchAdmission { key, .. }
        | NetworkEvent::CommandRejected { key }
        | NetworkEvent::Connected { key }
        | NetworkEvent::Login { key, .. }
        | NetworkEvent::Message { key, .. }
        | NetworkEvent::Terminated { key, .. } => *key,
    }
}

/// Source movement/cancellation can interrupt an active action even when an
/// earlier unaccepted ordinary request from the same peer is waiting. Kernel
/// authorization still rejects any older action that this control supersedes.
fn priority_control(event: &NetworkEvent, limit: usize) -> bool {
    let NetworkEvent::Message { message, .. } = event else {
        return false;
    };
    use bace_wire::opcode::GameActionType as A;
    bace_wire::GameActionEnvelope::decode(&message.bytes, limit).is_ok_and(|input| {
        matches!(
            input.action,
            A::Jump
                | A::JumpNonAutonomous
                | A::MoveToState
                | A::AutonomousPosition
                | A::CancelAttack
        )
    })
}
