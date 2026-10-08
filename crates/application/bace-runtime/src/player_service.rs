//! Bounded character lifecycle composition. All accepted world ownership and
//! database leases survive output pressure and uncertain online completion.
use crate::{
    game_login::{GameLoginService, LoadedPlayer},
    network::{NetworkCommand, NetworkThread},
    simulation::SimulationInput,
};
use bace_gameplay_api::{CharacterBinding, social::SocialEvent};
use bace_persistence::CharacterLease;
use bace_replication::{
    BatchLimits, EventSequencer, LoginProjection, LoginProjectionLimits, Sequences,
};
use bace_session::SessionKey;
use bace_simulation::{
    Command, PlayerAdmissionOutcome, PlayerAdmissionRequest, PreparedPlayerAdmission,
};
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, mpsc::TrySendError},
};
pub struct SessionReplication {
    pub key: SessionKey,
    pub binding: CharacterBinding,
    pub events: EventSequencer,
    pub properties: Sequences,
    pub item_properties: BTreeMap<EntityId, Sequences>,
    /// Last canonical public presentation flags; never collision authority.
    pub public_physics_state: Option<u32>,
    /// Accepted per-vital world revisions; independent output lanes cannot
    /// overwrite a newer health/mana/stamina observation with an older one.
    pub vital_revisions: [Option<u64>; 3],
}
struct Session {
    loaded: Option<Arc<LoadedPlayer>>,
    creation: Option<(crate::character_creation::FrozenCharacterCreation, u16)>,
    admission: Option<PlayerAdmissionRequest>,
    outstanding: Option<u64>,
    held: bool,
    accepted: bool,
    online: Option<CharacterLease>,
    replication: Option<SessionReplication>,
    entry: Option<bace_simulation::PlayerEnteredRequest>,
    entry_outstanding: bool,
    entry_error: Option<bace_gameplay_api::UiError>,
    entered: bool,
}
pub struct PlayerService {
    turbine_chat: bool,
    login: Option<GameLoginService>,
    maximum_slots: u16,
    io_pending: Option<(u64, SessionKey)>,
    io_network_slot: bool,
    sessions: BTreeMap<SessionKey, Session>,
    actors: BTreeMap<EntityId, SessionKey>,
    network: VecDeque<NetworkCommand>,
    capacity: usize,
    next: u64,
}
impl PlayerService {
    /// Synthetic output-state-machine fixture only. It requires a previously
    /// authenticated session and a save/lease/binding with the same identity;
    /// production world admission and DAT gates are not bypassed by this API.
    #[cfg(test)]
    pub(crate) fn test_admit_replication(
        &mut self,
        key: SessionKey,
        loaded: Arc<LoadedPlayer>,
    ) -> Result<(), String> {
        let binding = loaded.binding;
        if loaded.key != key
            || binding.actor.0 != loaded.player.player.entity.object_id
            || binding.account.0 != loaded.player.player.account_id
            || loaded.lease.character_id != binding.actor.0
            || self.actors.contains_key(&binding.actor)
        {
            return Err("test replication fixture identity mismatch".into());
        }
        let session = self
            .sessions
            .get_mut(&key)
            .ok_or("test session not authenticated")?;
        if session.loaded.is_some() || session.replication.is_some() {
            return Err("test session already loaded".into());
        }
        let item_properties = loaded
            .inventory
            .iter()
            .map(|item| {
                Ok((
                    EntityId(item.entity.object_id),
                    Sequences::new(256)
                        .map_err(|error| format!("test item sequence: {error:?}"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        session.loaded = Some(loaded);
        session.accepted = true;
        session.online = session.loaded.as_ref().map(|loaded| loaded.lease);
        session.replication = Some(SessionReplication {
            key,
            binding,
            events: EventSequencer::new(binding, 0),
            properties: Sequences::new(65536)
                .map_err(|error| format!("test actor sequence: {error:?}"))?,
            item_properties,
            public_physics_state: None,
            vital_revisions: [None; 3],
        });
        self.actors.insert(binding.actor, key);
        Ok(())
    }

    /// Test-only accepted entry receipt for an exact authenticated and durably
    /// online binding. Production entry still waits for `accept_entered`.
    #[cfg(test)]
    pub(crate) fn test_mark_entered(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
    ) -> Result<(), String> {
        if binding.session.0 != key.generation || self.actors.get(&binding.actor) != Some(&key) {
            return Err("test entered owner identity mismatch".into());
        }
        let session = self.sessions.get_mut(&key).ok_or("test session missing")?;
        if !session.accepted
            || session.entered
            || session.entry.is_some()
            || session.entry_outstanding
            || session.entry_error.is_some()
            || session
                .loaded
                .as_ref()
                .is_none_or(|loaded| loaded.binding != binding)
            || session
                .replication
                .as_ref()
                .is_none_or(|owner| owner.binding != binding)
            || session.online.is_none_or(|lease| {
                lease.state != bace_persistence::OwnershipState::Online
                    || lease.character_id != binding.actor.0
            })
        {
            return Err("test entered receipt lacks exact online binding".into());
        }
        session.entered = true;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_clear_replication(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
    ) -> Result<(), String> {
        let session = self.sessions.get_mut(&key).ok_or("test session missing")?;
        if session
            .replication
            .as_ref()
            .is_none_or(|owner| owner.binding != binding)
            || self.actors.get(&binding.actor) != Some(&key)
        {
            return Err("test replication owner mismatch".into());
        }
        session.loaded = None;
        session.accepted = false;
        session.online = None;
        session.replication = None;
        self.actors.remove(&binding.actor);
        Ok(())
    }

    pub fn new(login: GameLoginService, capacity: usize) -> Result<Self, String> {
        if !(1..=4096).contains(&capacity) {
            return Err("player service capacity".into());
        }
        let maximum_slots = login.maximum_slots();
        Ok(Self {
            turbine_chat: false,
            login: Some(login),
            maximum_slots,
            io_pending: None,
            io_network_slot: false,
            sessions: BTreeMap::new(),
            actors: BTreeMap::new(),
            network: VecDeque::new(),
            capacity,
            next: 0,
        })
    }
    /// Enable only after the composed driver installs authenticated Turbine ingress.
    pub(crate) fn enable_turbine_chat(&mut self) {
        self.turbine_chat = true;
    }
    /// Only call for the exact authenticated generation admitted by SessionRegistry.
    pub fn authenticated(
        &mut self,
        key: SessionKey,
        account: &bace_auth::AccountRecord,
    ) -> Result<(), String> {
        if self.sessions.len() >= self.capacity || self.sessions.contains_key(&key) {
            return Err("player lifecycle full/duplicate".into());
        }
        self.login
            .as_mut()
            .ok_or("lifecycle I/O pending")?
            .register_authenticated(key, account)
            .map_err(|e| e.to_string())?;
        self.sessions.insert(
            key,
            Session {
                loaded: None,
                creation: None,
                admission: None,
                outstanding: None,
                held: false,
                accepted: false,
                online: None,
                replication: None,
                entry: None,
                entry_outstanding: false,
                entry_error: None,
                entered: false,
            },
        );
        Ok(())
    }
    pub async fn roster(&mut self, key: SessionKey) -> Result<(), String> {
        if self.network.len() + usize::from(self.io_network_slot) >= self.capacity {
            return Err("player network backpressure".into());
        }
        let packet = crate::game_messages::roster_command(
            self.login.as_ref().ok_or("lifecycle I/O pending")?,
            key,
        )
        .await
        .map_err(|e| e.to_string())?;
        self.network.push_back(packet);
        Ok(())
    }
    pub async fn allocate_character_id(&self, key: SessionKey) -> Result<u32, String> {
        self.login
            .as_ref()
            .ok_or("lifecycle I/O pending")?
            .allocate_character_id(key)
            .await
            .map_err(|e| e.to_string())
    }
    /// Retains the exact creation bytes before database work. Retrying never allocates another identity.
    #[expect(
        clippy::result_large_err,
        reason = "returns retained immutable creation ownership without an allocation on rejected admission"
    )]
    pub fn stage_creation(
        &mut self,
        key: SessionKey,
        creation: crate::character_creation::FrozenCharacterCreation,
        slot: u16,
    ) -> Result<(), (String, crate::character_creation::FrozenCharacterCreation)> {
        let Some(s) = self.sessions.get_mut(&key) else {
            return Err(("unknown player session".into(), creation));
        };
        if self.io_pending.is_some_and(|(_, pending)| pending == key)
            || s.creation.is_some()
            || s.loaded.is_some()
            || slot >= self.maximum_slots
        {
            return Err(("player creation already pending".into(), creation));
        }
        s.creation = Some((creation, slot));
        Ok(())
    }
    pub async fn commit_creation(&mut self, key: SessionKey) -> Result<(), String> {
        if self.network.len() + usize::from(self.io_network_slot) >= self.capacity {
            return Err("player network backpressure".into());
        }
        let (creation, slot) = self
            .sessions
            .get(&key)
            .and_then(|s| s.creation.as_ref())
            .ok_or("missing exact creation")?;
        let input = crate::game_login::PreparedCharacter {
            player: &creation.player,
            items: &creation.items,
            slot: *slot,
        };
        let phase = self
            .login
            .as_ref()
            .ok_or("lifecycle I/O pending")?
            .phase(key)
            .map_err(|e| e.to_string())?;
        let result = if phase == crate::game_lifecycle::GameLoginPhase::Roster {
            self.login
                .as_mut()
                .ok_or("lifecycle I/O pending")?
                .create_prepared(key, input)
                .await
        } else {
            self.login
                .as_mut()
                .ok_or("lifecycle I/O pending")?
                .retry_creation(key, input)
                .await
        }
        .map_err(|e| e.to_string())?;
        let command = crate::game_messages::creation_command(&result).map_err(|e| e.to_string())?;
        self.network.push_back(command);
        self.sessions
            .get_mut(&key)
            .ok_or("missing creation session")?
            .creation = None;
        Ok(())
    }
    pub async fn load(&mut self, key: SessionKey, actor: u32) -> Result<Arc<LoadedPlayer>, String> {
        let s = self.sessions.get(&key).ok_or("unknown player session")?;
        if s.creation.is_some() || s.loaded.is_some() {
            return Err("player lifecycle pending".into());
        }
        let loaded = Arc::new(
            self.login
                .as_mut()
                .ok_or("lifecycle I/O pending")?
                .load_character(key, actor)
                .await
                .map_err(|e| e.to_string())?,
        );
        self.sessions
            .get_mut(&key)
            .ok_or("missing loading session")?
            .loaded = Some(loaded.clone());
        Ok(loaded)
    }
    pub fn retry_admission(&mut self, key: SessionKey) -> Result<(), String> {
        let s = self
            .sessions
            .get_mut(&key)
            .ok_or("unknown player session")?;
        if !s.held || s.admission.is_none() {
            return Err("no held admission".into());
        }
        s.held = false;
        Ok(())
    }
    pub fn loaded(&self, key: SessionKey) -> Option<&Arc<LoadedPlayer>> {
        self.sessions.get(&key)?.loaded.as_ref()
    }
    pub fn stage_admission(
        &mut self,
        key: SessionKey,
        prepared: PreparedPlayerAdmission,
    ) -> Result<u64, (String, Box<PreparedPlayerAdmission>)> {
        let check = (|| {
            let s = self.sessions.get(&key).ok_or("unknown player session")?;
            let loaded = s.loaded.as_ref().ok_or("player is not loaded")?;
            if s.accepted
                || s.outstanding.is_some()
                || s.admission.is_some()
                || prepared.binding != loaded.binding
                || self.actors.contains_key(&prepared.binding.actor)
            {
                return Err("stale/duplicate admission");
            }
            self.next
                .checked_add(1)
                .ok_or("player correlation overflow")
        })();
        let token = match check {
            Ok(token) => token,
            Err(e) => return Err((e.into(), Box::new(prepared))),
        };
        self.next = token;
        self.sessions
            .get_mut(&key)
            .expect("validated session")
            .admission = Some(PlayerAdmissionRequest {
            correlation: token,
            prepared: Box::new(prepared),
        });
        Ok(token)
    }
    pub fn flush_admissions(&mut self, input: &SimulationInput, budget: usize) -> usize {
        let mut sent = 0;
        for s in self
            .sessions
            .values_mut()
            .filter(|s| s.admission.is_some() && !s.held)
            .take(budget)
        {
            let request = s.admission.take().expect("filtered request");
            let token = request.correlation;
            match input.try_submit(Command::AdmitPlayer(request)) {
                Ok(()) => {
                    s.outstanding = Some(token);
                    sent += 1;
                }
                Err(
                    TrySendError::Full(Command::AdmitPlayer(request))
                    | TrySendError::Disconnected(Command::AdmitPlayer(request)),
                ) => {
                    s.admission = Some(request);
                    break;
                }
                Err(_) => unreachable!("input returns exact submitted variant"),
            }
        }
        sent
    }
    /// A failed owner admission is retained for explicit retry after prerequisites change.
    pub fn accept_admission(
        &mut self,
        outcome: PlayerAdmissionOutcome,
    ) -> Result<SessionKey, Box<PlayerAdmissionOutcome>> {
        let Some((&key, s)) = self.sessions.iter_mut().find(|(_, s)| {
            s.outstanding == Some(outcome.correlation)
                && s.loaded
                    .as_ref()
                    .is_some_and(|l| l.binding == outcome.binding)
        }) else {
            return Err(Box::new(outcome));
        };
        s.outstanding = None;
        match outcome.result {
            Ok(()) => {
                s.accepted = true;
                self.actors.insert(outcome.binding.actor, key);
            }
            Err((_, prepared)) => {
                s.held = true;
                s.admission = Some(PlayerAdmissionRequest {
                    correlation: outcome.correlation,
                    prepared,
                });
            }
        }
        Ok(key)
    }
    /// World acceptance alone is insufficient: Online's durable lease CAS must also resolve.
    pub async fn finish_online(&mut self, key: SessionKey) -> Result<CharacterLease, String> {
        let s = self.sessions.get(&key).ok_or("unknown player session")?;
        if !s.accepted {
            return Err("world admission not accepted".into());
        }
        if let Some(lease) = s.online {
            return Ok(lease);
        }
        let binding = s.loaded.as_ref().ok_or("missing loaded identity")?.binding;
        let lease = self
            .login
            .as_mut()
            .ok_or("lifecycle I/O pending")?
            .world_admitted(key, binding)
            .await
            .map_err(|e| e.to_string())?;
        self.adopt_online(key, lease)?;
        Ok(lease)
    }
    fn adopt_online(&mut self, key: SessionKey, lease: CharacterLease) -> Result<(), String> {
        let s = self
            .sessions
            .get_mut(&key)
            .ok_or("retained online session missing")?;
        let binding = s
            .loaded
            .as_ref()
            .ok_or("loaded online session missing")?
            .binding;
        s.online = Some(lease);
        let item_properties = s
            .loaded
            .as_ref()
            .expect("loaded online session")
            .inventory
            .iter()
            .map(|i| {
                Ok((
                    EntityId(i.entity.object_id),
                    Sequences::new(256).map_err(|e| format!("item sequence capacity: {e:?}"))?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        s.replication = Some(SessionReplication {
            public_physics_state: None,
            vital_revisions: [None; 3],
            key,
            binding,
            events: EventSequencer::new(binding, 0),
            properties: Sequences::new(65536).map_err(|e| format!("sequence capacity: {e:?}"))?,
            item_properties,
        });
        Ok(())
    }
    /// Readiness only; the eventual enqueue rechecks this same bounded owner.
    pub fn network_ready(&self) -> bool {
        self.network.len() + usize::from(self.io_network_slot) < self.capacity
    }
    pub fn queue_entry(
        &mut self,
        key: SessionKey,
        projection: LoginProjection<'_>,
        limits: LoginProjectionLimits,
    ) -> Result<(), String> {
        if self.network.len() + usize::from(self.io_network_slot) >= self.capacity {
            return Err("player network backpressure".into());
        }
        let token = self
            .next
            .checked_add(1)
            .ok_or("player correlation overflow")?;
        let session = self
            .sessions
            .get_mut(&key)
            .ok_or("unknown player session")?;
        if session.entry.is_some() || session.entered {
            return Err("player entry already queued".into());
        }
        let r = session
            .replication
            .as_mut()
            .ok_or("player not durably online")?;
        let public_physics_state = projection.self_object.physics.state;
        let batch = r
            .events
            .project_login(r.binding, projection, limits)
            .map_err(|e| format!("player entry projection: {e:?}"))?;
        let command =
            crate::game_messages::session_batch_command(key, batch).map_err(|e| e.to_string())?;
        self.network.push_back(command);
        r.public_physics_state = Some(public_physics_state);
        session.entry = Some(bace_simulation::PlayerEnteredRequest {
            correlation: token,
            binding: r.binding,
        });
        self.next = token;
        Ok(())
    }
    /// Trusted completion input is retained independently of the already encoded
    /// entry batch. Queue pressure cannot re-encode or resend the login sequence.
    pub fn flush_entered(&mut self, input: &SimulationInput, budget: usize) -> usize {
        if !self.network.is_empty() {
            return 0;
        }
        let mut sent = 0;
        for session in self
            .sessions
            .values_mut()
            .filter(|s| s.entry.is_some() && !s.entry_outstanding && s.entry_error.is_none())
            .take(budget)
        {
            let request = session.entry.expect("filtered entry");
            if input.try_submit(Command::PlayerEntered(request)).is_err() {
                break;
            }
            session.entry_outstanding = true;
            sent += 1;
        }
        sent
    }
    pub fn accept_entered(
        &mut self,
        outcome: bace_simulation::PlayerEnteredOutcome,
    ) -> Result<SessionKey, (String, bace_simulation::PlayerEnteredOutcome)> {
        let Some((&key, session)) = self.sessions.iter_mut().find(|(_, s)| {
            s.entry_outstanding
                && s.entry.is_some_and(|r| {
                    r.correlation == outcome.correlation && r.binding == outcome.binding
                })
        }) else {
            return Err(("unrelated player entered receipt".into(), outcome));
        };
        session.entry_outstanding = false;
        if let Err(error) = outcome.result {
            session.entry_error = Some(error);
            return Err((format!("player entered rejected: {error:?}"), outcome));
        }
        session.entry = None;
        session.entered = true;
        Ok(key)
    }
    pub fn entered(&self, actor: EntityId) -> bool {
        self.actors
            .get(&actor)
            .and_then(|key| self.sessions.get(key))
            .is_some_and(|s| s.entered)
    }
    /// Authenticated, durably online owners whose entry transition completed.
    /// The iterator borrows metadata only; it does not copy world state.
    pub fn entered_bindings(&self) -> impl Iterator<Item = (SessionKey, CharacterBinding)> + '_ {
        self.sessions
            .iter()
            .filter(|(_, session)| session.entered)
            .filter_map(|(key, session)| session.replication.as_ref().map(|r| (*key, r.binding)))
    }
    /// Called only after the simulation's exact detach outcome. The online lease
    /// and frozen owner remain retained until final persistence completes.
    pub fn stop_world_replication(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&key)
            .ok_or("detached replication session missing")?;
        if session
            .replication
            .as_ref()
            .is_none_or(|r| r.binding != binding)
            || session.entry_outstanding
            || session.entry.is_some()
        {
            return Err("detached replication binding mismatch".into());
        }
        session.entered = false;
        Ok(())
    }
    pub fn project_social(
        &mut self,
        recipient: EntityId,
        event: &SocialEvent,
        limits: BatchLimits,
    ) -> Result<Option<NetworkCommand>, String> {
        let Some(key) = self.actors.get(&recipient).copied() else {
            return Ok(None);
        };
        let session = self
            .sessions
            .get_mut(&key)
            .ok_or("accepted session missing")?;
        if !session.entered {
            return Err("accepted player awaiting entry completion".into());
        }
        let r = session
            .replication
            .as_mut()
            .ok_or("world actor awaiting durable online lease")?;
        crate::social_service::project_social_for_session(
            key,
            r.binding,
            event,
            &mut r.events,
            &mut r.properties,
            limits,
        )
        .map(Some)
    }
    pub fn project_experience(
        &mut self,
        event: &bace_gameplay_api::experience::ExperienceEvent,
        limits: BatchLimits,
    ) -> Result<Option<bace_replication::experience::ExperienceBatch>, String> {
        if !self.actors.contains_key(&event.actor) {
            return Ok(None);
        }
        if !self.entered(event.actor) {
            return Err("accepted player awaiting entry completion".into());
        }
        let r = self
            .replication(event.actor)
            .ok_or("accepted player awaiting durable online admission")?;
        bace_replication::experience::project_experience(
            r.binding,
            event,
            &mut r.properties,
            limits,
        )
        .map(Some)
        .map_err(|e| format!("XP projection: {e:?}"))
    }
    pub fn project_item_experience(
        &mut self,
        event: &bace_gameplay_api::item_experience::ItemExperienceEvent,
        limits: BatchLimits,
    ) -> Result<Option<bace_replication::item_experience::ItemExperienceBatch>, String> {
        if !self.actors.contains_key(&event.actor) {
            return Ok(None);
        }
        if !self.entered(event.actor) {
            return Err("accepted player awaiting entry completion".into());
        }
        let r = self
            .replication(event.actor)
            .ok_or("accepted player awaiting durable online admission")?;
        let seq = r
            .item_properties
            .get_mut(&event.item)
            .ok_or("unregistered item sequence owner")?;
        bace_replication::item_experience::project_item_experience(
            r.binding, event, event.item, seq, limits,
        )
        .map(Some)
        .map_err(|e| format!("item XP projection: {e:?}"))
    }
    /// Register a newly accepted item's independent property sequence owner only
    /// after its inventory admission has committed.
    pub fn admit_item_sequences(&mut self, actor: EntityId, item: EntityId) -> Result<(), String> {
        let r = self
            .replication(actor)
            .ok_or("player replication not admitted")?;
        if item.0 == 0 || r.item_properties.contains_key(&item) || r.item_properties.len() >= 1023 {
            return Err("item sequence identity/capacity".into());
        }
        r.item_properties.insert(
            item,
            Sequences::new(256).map_err(|e| format!("item sequence capacity: {e:?}"))?,
        );
        Ok(())
    }
    pub fn retire_item_sequences(&mut self, actor: EntityId, item: EntityId) -> Result<(), String> {
        let r = self
            .replication(actor)
            .ok_or("player replication not admitted")?;
        r.item_properties
            .remove(&item)
            .ok_or("item sequence owner missing")?;
        Ok(())
    }
    pub fn replication(&mut self, actor: EntityId) -> Option<&mut SessionReplication> {
        let key = *self.actors.get(&actor)?;
        self.sessions.get_mut(&key)?.replication.as_mut()
    }
    pub fn flush_network(&mut self, network: &NetworkThread, budget: usize) -> usize {
        let mut sent = 0;
        for _ in 0..budget {
            let Some(command) = self.network.pop_front() else {
                break;
            };
            match network.try_send(command) {
                Ok(()) => sent += 1,
                Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) => {
                    self.network.push_front(command);
                    break;
                }
            }
        }
        sent
    }
    pub async fn abort_loading(&mut self, key: SessionKey) -> Result<CharacterLease, String> {
        let s = self.sessions.get(&key).ok_or("unknown player session")?;
        if s.accepted || s.outstanding.is_some() {
            return Err("world owner must drain before abort".into());
        }
        let lease = self
            .login
            .as_mut()
            .ok_or("lifecycle I/O pending")?
            .abort_loading(key)
            .await
            .map_err(|e| e.to_string())?;
        let s = self.sessions.get_mut(&key).expect("retained abort session");
        s.loaded = None;
        s.admission = None;
        Ok(lease)
    }
    pub async fn logout_drained(
        &mut self,
        key: SessionKey,
        offline: CharacterLease,
    ) -> Result<(), String> {
        self.login
            .as_mut()
            .ok_or("lifecycle I/O pending")?
            .logout_drained(key, offline)
            .await
            .map_err(|e| e.to_string())?;
        let s = self
            .sessions
            .get_mut(&key)
            .ok_or("unknown player session")?;
        if let Some(loaded) = s.loaded.take() {
            self.actors.remove(&loaded.binding.actor);
        }
        s.accepted = false;
        s.online = None;
        s.replication = None;
        s.entry = None;
        s.entry_outstanding = false;
        s.entry_error = None;
        s.entered = false;
        Ok(())
    }
    /// Keep the service itself in recovery; dropping it would discard exact creation/admission/output ownership.
    pub fn requires_drain(&self) -> bool {
        !self.network.is_empty()
            || self.io_pending.is_some()
            || self.sessions.values().any(|s| {
                s.loaded.is_some()
                    || s.creation.is_some()
                    || s.admission.is_some()
                    || s.outstanding.is_some()
                    || s.accepted
            })
    }
}

#[cfg(test)]
pub(crate) mod tests;

mod io;
pub use io::{PlayerIoAction, PlayerIoCompletion, PlayerIoResult, PlayerIoWork};
