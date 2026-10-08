//! One live allegiance controller shares the driver's snapshot/control pumps.
use super::*;
use crate::allegiance_service::AllegianceService;
use bace_simulation::{
    AllegianceTicket, Command, SocialControl, SocialControlAction, SocialControlOutcome,
};
use std::sync::atomic::Ordering;
mod login;

pub(super) struct AllegianceRuntime {
    pub(super) service: AllegianceService,
    allocation: Option<Job<Result<Vec<u32>, String>>>,
    preparing: Option<Box<AllegianceTicket>>,
    lease_read: Option<Job<Result<Vec<bace_persistence::CharacterLease>, String>>>,
    leases: Option<Vec<bace_persistence::CharacterLease>>,
    allocated: Option<u32>,
    seed: Option<(u64, u32)>,
    available: Option<u32>,
    retained: Option<Box<AllegianceTicket>>,
    failure: Option<String>,
    login: login::LoginRedemptions,
}
impl AllegianceRuntime {
    pub(super) fn new(bootstrap: &GameBootstrap) -> Result<Self, String> {
        Ok(Self {
            service: AllegianceService::new(
                bootstrap.world_owner.epoch(),
                &bootstrap.allegiance_nodes,
                &bootstrap.allegiance_metadata,
            )?,
            allocation: None,
            preparing: None,
            lease_read: None,
            leases: None,
            allocated: None,
            seed: None,
            available: None,
            retained: None,
            failure: None,
            login: login::LoginRedemptions::default(),
        })
    }
    pub(super) fn has_pending(&self) -> bool {
        self.service.requires_drain()
            || self.allocation.is_some()
            || self.preparing.is_some()
            || self.lease_read.is_some()
            || self.allocated.is_some()
            || self.seed.is_some()
            || self.retained.is_some()
            || self.login.has_pending()
    }
}
impl GameRuntime {
    pub fn source_allegiance_ticket(&self) -> Option<&AllegianceTicket> {
        self.allegiance
            .retained
            .as_deref()
            .filter(|t| t.npc.is_some() || t.rare.is_some())
    }
    pub fn allegiance_before_images(
        &self,
        ticket: &AllegianceTicket,
    ) -> Result<
        (
            Vec<bace_persistence::StoredAllegiance>,
            Vec<bace_persistence::StoredAllegiance>,
        ),
        String,
    > {
        self.allegiance.service.external_before_images(ticket)
    }
    /// Called only after a source workflow's durable composite and exact owner
    /// receipt. A queued write or owner command is not a committed handoff.
    pub fn accept_source_allegiance_commit(
        &mut self,
        ticket: &AllegianceTicket,
        operation: &bace_persistence::AllegianceOperation,
    ) -> Result<(), String> {
        if self
            .allegiance
            .retained
            .as_deref()
            .is_some_and(|t| t != ticket)
        {
            return Err("source allegiance ticket mismatch".into());
        }
        self.allegiance
            .service
            .accept_external_commit(ticket, operation)?;
        self.allegiance.retained = None;
        Ok(())
    }
    pub fn allegiance_failure(&self) -> Option<&str> {
        self.allegiance
            .failure
            .as_deref()
            .or_else(|| self.allegiance.service.blocked())
    }
    pub fn retry_allegiance(&mut self) {
        self.allegiance.failure = None;
        self.allegiance.service.retry();
    }
    pub(super) fn allegiance_seed_ready(&self) -> bool {
        self.allegiance.available.is_some()
    }
    pub(super) fn allegiance_owns_control(&self, outcome: &SocialControlOutcome) -> bool {
        self.allegiance
            .seed
            .is_some_and(|(sequence, _)| sequence == outcome.sequence)
            || self.allegiance.login.owns(outcome.sequence)
            || self.allegiance.service.owns_control(outcome)
    }
    pub(super) fn accept_allegiance_control(
        &mut self,
        outcome: SocialControlOutcome,
    ) -> Result<(), String> {
        if self.allegiance.login.owns(outcome.sequence) {
            let (key, binding, result) =
                self.allegiance.login.accept(outcome).map_err(|error| {
                    let failure = error.to_owned();
                    self.allegiance.failure = Some(failure.clone());
                    failure
                })?;
            match result {
                Ok(_) | Err(bace_gameplay_api::social::SocialError::Busy) => return Ok(()),
                Err(
                    bace_gameplay_api::social::SocialError::Missing
                    | bace_gameplay_api::social::SocialError::Offline,
                ) if self.players.binding_for(key) != Some(binding)
                    || !self.players.entered(binding.actor) =>
                {
                    return Ok(());
                }
                Err(error) => {
                    let failure = format!("allegiance login redemption rejected: {error:?}");
                    self.allegiance.failure = Some(failure.clone());
                    return Err(failure);
                }
            }
        }
        if let Some((sequence, id)) = self.allegiance.seed
            && sequence == outcome.sequence
        {
            if outcome.result == Ok(None) {
                self.allegiance.seed = None;
                self.allegiance.available = Some(id);
                return Ok(());
            }
            self.allegiance.failure = Some(format!(
                "allegiance ID owner rejected: {:?}",
                outcome.result
            ));
            return Err(self.allegiance.failure.clone().expect("set failure"));
        }
        self.allegiance
            .service
            .accept_control(outcome)
            .map_err(|_| "allegiance control correlation".into())
    }
    pub(super) fn poll_allegiance(&mut self) -> Result<(), String> {
        if let Some(result) = ready(&mut self.allegiance.allocation) {
            match result {
                Ok(ids) if ids.len() == 1 => self.allegiance.allocated = Some(ids[0]),
                Ok(_) => {
                    self.allegiance.failure = Some("allegiance allocator response bounds".into())
                }
                Err(error) => self.allegiance.failure = Some(error),
            }
        }
        if let Some(error) = &self.allegiance.failure {
            return Err(error.clone());
        }
        if self.allegiance.retained.is_some() {
            return Err(
                "source workflow allegiance ticket retained for its composite owner".into(),
            );
        }
        if !self.allegiance.service.requires_drain()
            && self.allegiance.preparing.is_none()
            && let Ok(ticket) = self.simulation.allegiance_proposals().try_recv()
        {
            if ticket.patch.metadata.iter().any(|(before, after)| {
                after.as_ref().is_some_and(|m| {
                    Some(m.chat_room) == self.allegiance.available
                        && before
                            .as_ref()
                            .is_none_or(|old| old.chat_room != m.chat_room)
                })
            }) {
                self.allegiance.available = None;
            }
            self.allegiance.preparing = Some(Box::new(ticket));
        }
        if let Some(result) = ready(&mut self.allegiance.lease_read) {
            match result {
                Ok(leases) => self.allegiance.leases = Some(leases),
                Err(error) => {
                    self.allegiance.failure = Some(error.clone());
                    return Err(error);
                }
            }
        }
        if let Some(ticket) = self.allegiance.preparing.as_deref().cloned() {
            if let Some(leases) = self.allegiance.leases.take() {
                let expected = AllegianceService::participants(&ticket);
                if leases.iter().map(|l| l.character_id).collect::<Vec<_>>() != expected
                    || leases.iter().any(|l| {
                        !matches!(
                            l.state,
                            bace_persistence::OwnershipState::Online
                                | bace_persistence::OwnershipState::Offline
                        )
                    })
                {
                    self.allegiance.failure =
                        Some("allegiance participant ownership is missing or in transition".into());
                    return Err(self.allegiance.failure.clone().expect("set failure"));
                }
                let mut bindings = vec![];
                for (actor, _) in &ticket.player_changes {
                    let Some(replication) = self.players.replication(*actor) else {
                        return Err("allegiance reward player binding missing".into());
                    };
                    bindings.push(replication.binding);
                }
                let npc = if ticket.npc.is_some() {
                    match self.prepare_npc_shared(&ticket)? {
                        Some(npc) => Some(npc),
                        None => {
                            self.allegiance.leases = Some(leases);
                            return Ok(());
                        }
                    }
                } else {
                    None
                };
                let ticket = *self.allegiance.preparing.take().expect("prepared ticket");
                let result = if let Some(npc) = npc {
                    self.allegiance
                        .service
                        .stage_npc(ticket, bindings, leases, npc)
                        .map_err(|rejected| {
                            let (ticket, npc) = *rejected;
                            self.finish_npc_shared(npc.token, false)
                                .expect("unsubmitted exact source lock");
                            Box::new(ticket)
                        })
                } else {
                    self.allegiance.service.stage(ticket, bindings, leases)
                };
                if let Err(ticket) = result {
                    self.allegiance.retained = Some(ticket);
                    return Err("allegiance proposal requires source composite owner".into());
                }
            } else if self.allegiance.lease_read.is_none()
                && !self.bootstrap.save_pressure.load(Ordering::Acquire)
            {
                let ids = AllegianceService::participants(&ticket);
                let store = self.bootstrap.store.clone();
                self.allegiance.lease_read = Some(Box::pin(async move {
                    store
                        .character_leases(&ids)
                        .await
                        .map_err(|e| e.to_string())
                }));
            }
        }
        let token = self.token()?;
        self.allegiance.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            token,
        )?;
        if let Some(completion) = self.allegiance.service.take_completion() {
            self.record_binding_allegiance_completion(completion.operation, completion.committed)?;
            if let Some(npc) = completion.npc {
                self.finish_npc_shared(npc, completion.committed)?;
            }
            self.allegiance
                .login
                .finish(completion.operation, completion.committed);
        }
        if let Some(id) = self.allegiance.allocated {
            let token = self.token()?;
            match self
                .simulation
                .input()
                .try_submit(Command::SocialControl(SocialControl {
                    sequence: token,
                    action: SocialControlAction::SupplyAllegianceId(id),
                })) {
                Ok(()) => {
                    self.allegiance.allocated = None;
                    self.allegiance.seed = Some((token, id));
                }
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("allegiance ID ingress closed".into());
                }
            }
        }
        if !self.draining
            && self.allegiance.available.is_none()
            && self.allegiance.allocated.is_none()
            && self.allegiance.seed.is_none()
            && self.allegiance.allocation.is_none()
            && !self.bootstrap.save_pressure.load(Ordering::Acquire)
        {
            let store = self.bootstrap.store.clone();
            self.allegiance.allocation = Some(Box::pin(async move {
                store
                    .allocate_dynamic_ids(1)
                    .await
                    .map_err(|e| e.to_string())
            }));
        }
        self.poll_allegiance_login_redemption()?;
        Ok(())
    }

    fn poll_allegiance_login_redemption(&mut self) -> Result<(), String> {
        let entered: Vec<_> = self
            .players
            .entered_bindings()
            .filter(|(key, _)| {
                self.sessions.get(key).is_some_and(|session| {
                    !session.terminated && !session.disconnected && !session.closing
                })
            })
            .collect();
        self.allegiance.login.retain_entered(&entered);
        if self.draining
            || self.allegiance.service.requires_drain()
            || self.allegiance.preparing.is_some()
            || self.allegiance.retained.is_some()
            || self.bootstrap.save_pressure.load(Ordering::Acquire)
        {
            return Ok(());
        }
        let Some((key, binding)) = self.allegiance.login.candidate(&entered) else {
            return Ok(());
        };
        let sequence = self.token()?;
        match self
            .simulation
            .input()
            .try_submit(Command::SocialControl(SocialControl {
                sequence,
                action: SocialControlAction::RedeemAllegiance(binding.actor),
            })) {
            Ok(()) => self
                .allegiance
                .login
                .start(sequence, key, binding)
                .map_err(str::to_owned),
            Err(std::sync::mpsc::TrySendError::Full(_)) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("allegiance login redemption ingress closed".into())
            }
        }
    }
}
