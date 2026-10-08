//! Atomic detach freezes the last live state before final writes. No network
//! logout success or lease release occurs until every owned row is durable.
use super::*;
use bace_persistence::{CharacterLease, OwnershipState, SaveSnapshot};
use bace_simulation::{Command, PlayerDetachRequest};
pub(super) struct Work {
    pub adopted: bool,
    request: Option<u64>,
    detached: Option<bace_simulation::DetachedPlayer>,
    staged: bool,
    frozen: Option<SaveSnapshot>,
    lease: CharacterLease,
    database_pending: bool,
    offline: Option<CharacterLease>,
}
pub(super) struct Completion {
    key: SessionKey,
    result: Result<CharacterLease, String>,
}
impl GameRuntime {
    pub(super) fn poll_logout(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        if let Some(done) = ready(&mut self.logout_job) {
            let work = self
                .logout
                .get_mut(&done.key)
                .ok_or("logout completion owner missing")?;
            work.database_pending = false;
            match done.result {
                Ok(lease) => work.offline = Some(lease),
                Err(error) => {
                    self.sessions
                        .get_mut(&done.key)
                        .ok_or("logout session missing")?
                        .failure = Some(error)
                }
            }
        }
        if self.unexpected_detach.is_some() {
            return Err("unrelated detached owner retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            if self.visibility.retirements.len() >= 65536 {
                break;
            }
            let Ok(outcome) = self.simulation.player_detach_outcomes().try_recv() else {
                break;
            };
            let key = self
                .logout
                .iter()
                .find(|(_, w)| w.request == Some(outcome.correlation))
                .map(|(k, _)| *k);
            let Some(key) = key else {
                self.unexpected_detach = Some(Box::new(outcome));
                return Err("unrelated detached owner retained".into());
            };
            if self.players.binding_for(key) != Some(outcome.binding) {
                self.unexpected_detach = Some(Box::new(outcome));
                return Err("detached binding mismatch".into());
            }
            if let Err(error) = self.finish_portal_disconnect(
                key,
                outcome.correlation,
                outcome.binding,
                outcome.result.is_ok(),
            ) {
                self.unexpected_detach = Some(Box::new(outcome));
                return Err(error);
            }
            let work = self.logout.get_mut(&key).expect("matched logout");
            work.request = None;
            match outcome.result {
                Ok(detached) => {
                    let actor = detached.actor.id;
                    let tick = detached.snapshot.tick();
                    work.detached = Some(detached);
                    self.players.stop_world_replication(key, outcome.binding)?;
                    self.visibility.retirements.insert(actor, tick);
                }
                Err(bace_simulation::CharacterRegistrationError::DurabilityPending) => {}
                Err(error) => {
                    self.sessions
                        .get_mut(&key)
                        .expect("retained session")
                        .failure = Some(format!("detach rejected: {error:?}"))
                }
            }
        }
        let keys: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, s)| (s.terminated || self.draining) && s.failure.is_none())
            .take(self.limits.work_per_poll)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            if let Err(error) = self.advance_logout(key, elapsed, unix) {
                self.sessions
                    .get_mut(&key)
                    .expect("retained logout session")
                    .failure = Some(error);
            }
        }
        Ok(())
    }
    fn advance_logout(
        &mut self,
        key: SessionKey,
        elapsed: Duration,
        unix: u64,
    ) -> Result<(), String> {
        self.cancel_staff_unsubmitted_for_logout(key);
        if (self.social_ingress_blocked(key) || self.staff_ingress_blocked(key))
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
            || self.movement_pending(key)
            || self.portal_session_pending(key)
            || self.death_session_pending(key)
            || self.recall_session_pending(key)
        {
            return Ok(());
        }
        let session = self.sessions.get(&key).ok_or("logout session missing")?;
        let Some(loading) = &session.loading else {
            if !session.disconnected {
                if !session.closing && self.network_output.len() < self.limits.messages {
                    self.network_output
                        .push_back(NetworkCommand::Terminate { key });
                    self.sessions
                        .get_mut(&key)
                        .expect("retained session")
                        .closing = true;
                }
                return Ok(());
            }
            if self.login_key != Some(key)
                && !self.login_queue.iter().any(|(k, _)| *k == key)
                && self.network_output.len() < self.limits.messages
            {
                self.network_output
                    .push_back(NetworkCommand::DrainCompleted { key });
                self.login_queue.push_back((key, PlayerIoAction::Retire));
            }
            return Ok(());
        };
        // Cold/admission ownership may be in flight; finish its exact transition
        // rather than racing an abort against an accepted body.
        let Some(lease) = loading.online else {
            return Ok(());
        };
        // Resolve the exact entered receipt before detach; otherwise an already
        // submitted entry/snapshot could race the retired world owner. A closed
        // peer's packets are cancelled by the network owner, not replayed.
        if loading.phase != lifecycle::Phase::Entered {
            return Ok(());
        }
        let binding = loading.loaded.binding;
        let actor = binding.actor.0;
        if !self.logout.contains_key(&key) {
            self.online_saves.drain_player(actor, elapsed)?;
            self.logout.insert(
                key,
                Work {
                    adopted: false,
                    request: None,
                    detached: None,
                    staged: false,
                    frozen: None,
                    lease,
                    database_pending: false,
                    offline: None,
                },
            );
        }
        let work = &self.logout[&key];
        if work.adopted {
            if (self.draining || session.closing) && !session.disconnected {
                if !session.closing && self.network_output.len() < self.limits.messages {
                    self.network_output
                        .push_back(NetworkCommand::Terminate { key });
                    self.sessions
                        .get_mut(&key)
                        .expect("retained session")
                        .closing = true;
                }
                return Ok(());
            }
            if self.network_output.len() + 2 <= self.limits.messages {
                self.logout_completed(key)?;
            }
            return Ok(());
        }
        if work.detached.is_none() {
            if work.request.is_some() || !self.online_saves.critical_ready(&[actor])? {
                return Ok(());
            }
            if !self.queue_corpse_viewer_detach(key)? {
                return Ok(());
            }
            let (saved, _, _) = self
                .online_saves
                .baseline(actor)
                .ok_or("logout baseline missing")?;
            let expected_revision = saved.player.entity.mutation_revision;
            let expected_items = self
                .online_saves
                .inventory_baselines(actor)
                .iter()
                .map(|i| {
                    (
                        bace_types::EntityId(i.entity.object_id),
                        i.entity.mutation_revision,
                    )
                })
                .collect();
            let token = self.token()?;
            if self
                .simulation
                .input()
                .try_submit(Command::DetachPlayer(PlayerDetachRequest {
                    correlation: token,
                    binding,
                    expected_revision,
                    expected_items,
                    capture_final: true,
                }))
                .is_ok()
            {
                self.logout.get_mut(&key).expect("logout").request = Some(token);
                self.begin_portal_disconnect(key, token)?;
            }
            return Ok(());
        }
        if !work.staged {
            let snapshot = work
                .detached
                .as_ref()
                .expect("detached owner")
                .snapshot
                .clone();
            self.online_saves
                .stage_detached_snapshot(snapshot, elapsed, unix)?;
            self.logout.get_mut(&key).expect("logout").staged = true;
        }
        self.online_saves.submit_due(
            &self.saves.handle,
            elapsed,
            true,
            self.limits.work_per_poll,
        )?;
        if !self.online_saves.player_clean(actor) {
            return Ok(());
        }
        let work = self.logout.get_mut(&key).expect("logout");
        if let Some(offline) = work.offline {
            if self.login_key != Some(key) && !self.login_queue.iter().any(|(k, _)| *k == key) {
                self.login_queue
                    .push_back((key, PlayerIoAction::Logout(offline)));
            }
            return Ok(());
        }
        if work.database_pending || self.logout_job.is_some() {
            return Ok(());
        }
        if work.frozen.is_none() {
            let (saved, version, _) = self
                .online_saves
                .baseline(actor)
                .ok_or("final logout baseline missing")?;
            work.frozen = Some(SaveSnapshot {
                object_id: actor,
                mutation_revision: saved.player.entity.mutation_revision,
                expected_version: version,
                bytes: saved.encode().map_err(|e| e.to_string())?,
            });
        }
        let snapshot = work
            .frozen
            .as_ref()
            .expect("retained final payload")
            .clone();
        let lease = work.lease;
        work.database_pending = true;
        let store = self.bootstrap.store.clone();
        self.logout_job = Some(Box::pin(async move {
            Completion {
                key,
                result: finish(&store, lease, &snapshot).await,
            }
        }));
        Ok(())
    }
    pub(super) fn logout_completed(&mut self, key: SessionKey) -> Result<(), String> {
        let work = self
            .logout
            .get(&key)
            .ok_or("offline logout owner missing")?;
        let actor = work.lease.character_id;
        if work.offline.is_none() || !work.staged || work.detached.is_none() {
            return Err("logout receipt before final ownership drain".into());
        }
        // Reserve output before forgetting the last durable owner.
        if self.network_output.len() + 2 > self.limits.messages {
            return Err("logout output backpressure".into());
        }
        self.online_saves.forget_clean(actor)?;
        self.logout.remove(&key);
        let session = self.sessions.get_mut(&key).expect("retained session");
        session.loading = None;
        if session.disconnected || self.draining {
            self.network_output
                .push_back(NetworkCommand::DrainCompleted { key });
            self.login_queue.push_back((key, PlayerIoAction::Retire));
        } else {
            session.terminated = false;
            self.network_output.push_back(NetworkCommand::Send {
                key,
                queue: 9,
                bytes: bace_wire::CharacterReply::LoggedOff
                    .encode()
                    .expect("constant logout response"),
            });
            self.network_output
                .push_back(NetworkCommand::LogoutCommitted { key });
            self.login_queue.push_back((key, PlayerIoAction::Roster));
        }
        Ok(())
    }
    pub fn retry_player(&mut self, key: SessionKey) -> Result<(), String> {
        if !self.sessions.contains_key(&key) {
            return Err("unknown retry session".into());
        }
        let progression = self.retry_progression(key);
        let inventory = self.retry_inventory(key);
        let crafting = self.retry_crafting(key);
        let recall = self.retry_recall(key);
        let staff = self.retry_staff_session(key);
        let portal = self.retry_portal_session(key);
        let death = self.retry_death_session(key);
        let skill_device = self.retry_skill_device(key);
        let attribute_transfer = self.retry_attribute_transfer_session(key);
        let pet = self.retry_pet_session(key);
        let subsystem = progression
            || inventory
            || crafting
            || portal
            || death
            || skill_device
            || attribute_transfer
            || pet
            || recall;
        let session = self.sessions.get_mut(&key).expect("checked session");
        if session.failure.take().is_none() && !subsystem && !staff {
            return Err("player has no failed work".into());
        }
        Ok(())
    }
}
pub(crate) async fn finish(
    store: &bace_db_postgres::PgStore,
    online: CharacterLease,
    snapshot: &SaveSnapshot,
) -> Result<CharacterLease, String> {
    let logging = CharacterLease {
        epoch: online
            .epoch
            .checked_add(1)
            .ok_or("logout epoch exhausted")?,
        state: OwnershipState::LoggingOut,
        ..online
    };
    let offline = CharacterLease {
        state: OwnershipState::Offline,
        ..logging
    };
    // Each transition itself serializes on the row lock. On an uncertain result,
    // retain exact bytes and call this bounded resolver again; never new payloads.
    match store
        .character_lease(online.character_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(actual) if actual == online => {
            store
                .begin_logout(online)
                .await
                .map_err(|e| e.to_string())?;
        }
        Some(actual) if actual == logging => {}
        Some(actual) if actual == offline => {
            let stored = store
                .load(snapshot.object_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("offline logout snapshot missing")?;
            if stored.persisted_version
                != snapshot
                    .expected_version
                    .checked_add(1)
                    .ok_or("logout snapshot version exhausted")?
                || stored.bytes != snapshot.bytes
            {
                return Err("offline logout payload mismatch".into());
            }
            return Ok(offline);
        }
        _ => return Err("logout ownership fence changed".into()),
    }
    let (receipt, ack) = store
        .finish_logout(logging, snapshot)
        .await
        .map_err(|e| e.to_string())?;
    if receipt != offline
        || ack.object_id != snapshot.object_id
        || ack.mutation_revision != snapshot.mutation_revision
        || ack.persisted_version != snapshot.expected_version + 1
    {
        return Err("logout durable acknowledgment mismatch".into());
    }
    Ok(receipt)
}
