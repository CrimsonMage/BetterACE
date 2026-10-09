//! Retained character creation: authenticated request -> durable identities ->
//! verified cold assets/geometry -> exact frozen database transaction -> reply.
use super::*;
use crate::creation_preparation_worker::{
    CreationPreparationCompletion, CreationPreparationRequest, CreationPreparationWorker,
    PreparedCharacterCreation,
};
use crate::player_service::PlayerIoResult;
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Character,
    Items,
    Cold,
    Prepared,
    Commit,
    Rejected,
}
struct Work {
    wire: bace_wire::CharacterCreateRequest,
    account: Arc<bace_auth::AccountRecord>,
    generation: Arc<bace_storage_codec::PackGeneration>,
    unix: u64,
    correlation: u64,
    phase: Phase,
    entity: Option<EntityId>,
    items: Option<Vec<u32>>,
    cold_pending: bool,
    prepared: Option<PreparedCharacterCreation>,
    response: Option<u32>,
    retry_at: u64,
    failure: Option<String>,
}
struct ItemCompletion {
    key: SessionKey,
    result: Result<Vec<u32>, String>,
}
pub(super) struct CreationRuntime {
    worker: Option<CreationPreparationWorker>,
    profile: Arc<bace_content::CharacterStartProfileV1>,
    work: BTreeMap<SessionKey, Work>,
    item_job: Option<Job<ItemCompletion>>,
    unexpected: Option<CreationPreparationCompletion>,
    capacity: usize,
    maximum_slots: u16,
}
pub(super) enum CreationIngress {
    Accepted,
    Blocked,
    Unsupported,
}
impl CreationRuntime {
    pub(super) fn new(
        manifest: crate::region_activation::RegionAssetManifest,
        capacity: usize,
        maximum_slots: u16,
    ) -> Result<Self, String> {
        if !(1..=100).contains(&maximum_slots) {
            return Err("creation slot capacity".into());
        }
        let profile = Arc::new(crate::creation_profile::builtin_creation_profile()?);
        Ok(Self {
            worker: Some(CreationPreparationWorker::start(manifest, capacity)?),
            profile,
            work: BTreeMap::new(),
            item_job: None,
            unexpected: None,
            capacity,
            maximum_slots,
        })
    }
    pub(super) fn pending(&self) -> bool {
        !self.work.is_empty()
            || self.item_job.is_some()
            || self.unexpected.is_some()
            || self.worker.as_ref().is_some_and(|w| w.pending() != 0)
    }
    pub(super) fn shutdown(&mut self) -> Result<Option<std::thread::JoinHandle<()>>, String> {
        if self.pending() {
            return Err("creation preparation requires drain".into());
        }
        let Some(worker) = self.worker.take() else {
            return Ok(None);
        };
        match worker.try_shutdown() {
            Ok(thread) => Ok(Some(thread)),
            Err(worker) => {
                self.worker = Some(*worker);
                Err("creation worker still owns receipts".into())
            }
        }
    }
}
impl GameRuntime {
    /// Join the returned cold worker only on blocking capacity after drain.
    pub fn shutdown_creation_worker(
        &mut self,
    ) -> Result<Option<std::thread::JoinHandle<()>>, String> {
        self.creation.shutdown()
    }
    pub(super) fn creation_ingress_blocked(&self, key: SessionKey) -> bool {
        self.creation.work.contains_key(&key)
    }
    pub(super) fn handle_creation_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<CreationIngress, String> {
        if message.bytes.get(..4) != Some(&0xF656u32.to_le_bytes()) {
            return Ok(CreationIngress::Unsupported);
        }
        if self.creation.work.contains_key(&key)
            || self.creation.work.len() >= self.creation.capacity
            || self.login_key == Some(key)
            || self.login_queue.iter().any(|(k, _)| *k == key)
        {
            return Ok(CreationIngress::Blocked);
        }
        let session = self.sessions.get(&key).ok_or("creation session missing")?;
        if self.draining || session.terminated || session.loading.is_some() {
            return Ok(CreationIngress::Blocked);
        }
        let wire = match bace_wire::CharacterCreateRequest::decode(
            &message.bytes,
            self.limits.message_bytes,
            1024,
            55,
        ) {
            Ok(wire) => wire,
            Err(_) => {
                return self.reject_creation_ingress(key, 5);
            }
        };
        if wire.character_slot >= u32::from(self.creation.maximum_slots)
            || crate::character_preparation::creation_request(&wire, &session.account, 0x50000001)
                .is_err()
        {
            return self.reject_creation_ingress(key, 5);
        }
        let unix = self
            .clock
            .unix_millis
            .checked_add(
                u64::try_from(self.last_elapsed.as_millis()).map_err(|_| "creation clock")?,
            )
            .ok_or("creation clock overflow")?;
        crate::creation_assets::creation_birth_date(unix)?;
        self.next = self
            .next
            .checked_add(1)
            .ok_or("creation correlation exhausted")?;
        self.creation.work.insert(
            key,
            Work {
                wire,
                account: Arc::new(session.account.clone()),
                generation: self.bootstrap.pack.generation.clone(),
                unix,
                correlation: self.next,
                phase: Phase::Character,
                entity: None,
                items: None,
                cold_pending: false,
                prepared: None,
                response: None,
                retry_at: 0,
                failure: None,
            },
        );
        self.login_queue.push_back((key, PlayerIoAction::Allocate));
        Ok(CreationIngress::Accepted)
    }
    fn reject_creation_ingress(
        &mut self,
        key: SessionKey,
        code: u32,
    ) -> Result<CreationIngress, String> {
        if self.network_output.len() >= self.limits.messages {
            return Ok(CreationIngress::Blocked);
        }
        self.network_output.push_back(NetworkCommand::Send {
            key,
            queue: 9,
            bytes: bace_wire::CharacterReply::CreateFailed(code)
                .encode()
                .map_err(|e| e.to_string())?,
        });
        Ok(CreationIngress::Accepted)
    }
    /// Called before generic lifecycle error handling. Uncertain commit retries
    /// retain the staged PlayerService proposal and original allocated identity.
    pub(super) fn accept_creation_io(
        &mut self,
        key: SessionKey,
        result: &Result<PlayerIoResult, String>,
    ) -> Result<bool, String> {
        let Some(work) = self.creation.work.get_mut(&key) else {
            return Ok(false);
        };
        match result {
            Ok(PlayerIoResult::Allocated(id)) if work.phase == Phase::Character => {
                if !(0x50000001..=0x5fffffff).contains(id) {
                    return Err("allocated character identity range".into());
                }
                work.entity = Some(EntityId(*id));
                work.phase = Phase::Items;
            }
            Ok(PlayerIoResult::Created(id)) if work.phase == Phase::Commit => {
                if work.entity != Some(EntityId(*id)) {
                    return Err("created identity fence".into());
                }
                self.creation.work.remove(&key);
            }
            Ok(PlayerIoResult::CreationRejected(code)) if work.phase == Phase::Commit => {
                work.phase = Phase::Rejected;
                work.response = Some(*code);
            }
            Err(error) if matches!(work.phase, Phase::Character | Phase::Commit) => {
                work.failure = Some(error.clone());
                work.retry_at = self
                    .clock
                    .unix_millis
                    .checked_add(self.last_elapsed.as_millis() as u64)
                    .and_then(|t| t.checked_add(1000))
                    .ok_or("creation retry clock")?;
            }
            _ => return Err("unexpected creation lifecycle outcome".into()),
        }
        Ok(true)
    }
    pub(super) fn poll_creation(&mut self, unix: u64) -> Result<(), String> {
        if self.creation.unexpected.is_some() {
            return Err("unrelated creation completion retained".into());
        }
        if let Some(done) = ready(&mut self.creation.item_job) {
            let work = self
                .creation
                .work
                .get_mut(&done.key)
                .ok_or("creation item allocation owner missing")?;
            match done.result {
                Ok(ids) => {
                    work.items = Some(ids);
                    work.phase = Phase::Cold;
                    work.failure = None;
                }
                Err(error) => {
                    work.failure = Some(error);
                    work.retry_at = unix.checked_add(1000).ok_or("creation retry clock")?;
                }
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(done) = self
                .creation
                .worker
                .as_mut()
                .ok_or("creation worker stopped")?
                .try_recv()?
            else {
                break;
            };
            let valid = self.creation.work.get(&done.key).is_some_and(|w| {
                w.phase == Phase::Cold
                    && w.cold_pending
                    && w.correlation == done.correlation
                    && w.account.id == done.account
                    && w.entity == Some(done.entity)
                    && u32::from(done.slot) == w.wire.character_slot
                    && Arc::ptr_eq(&w.generation, &done.generation)
            });
            if !valid {
                self.creation.unexpected = Some(done);
                return Err("creation completion identity fence".into());
            }
            let work = self.creation.work.get_mut(&done.key).expect("checked work");
            work.cold_pending = false;
            match done.result {
                Ok(prepared) => {
                    work.prepared = Some(prepared);
                    work.phase = Phase::Prepared;
                }
                Err(error) => {
                    work.phase = Phase::Rejected;
                    work.response = Some(error.response);
                    work.failure = Some(error.message);
                }
            }
        }
        let keys: Vec<_> = self
            .creation
            .work
            .keys()
            .copied()
            .take(self.limits.work_per_poll)
            .collect();
        for key in keys {
            self.advance_creation(key, unix)?;
        }
        Ok(())
    }
    fn advance_creation(&mut self, key: SessionKey, unix: u64) -> Result<(), String> {
        let work = self
            .creation
            .work
            .get_mut(&key)
            .ok_or("creation owner missing")?;
        if unix < work.retry_at {
            return Ok(());
        }
        let login_pending =
            self.login_key == Some(key) || self.login_queue.iter().any(|(k, _)| *k == key);
        match work.phase {
            Phase::Character | Phase::Commit => {
                if !login_pending {
                    self.login_queue.push_back((
                        key,
                        if work.phase == Phase::Character {
                            PlayerIoAction::Allocate
                        } else {
                            PlayerIoAction::Create
                        },
                    ));
                }
            }
            Phase::Items => {
                if self.creation.item_job.is_none() {
                    let store = self.bootstrap.store.clone();
                    self.creation.item_job = Some(Box::pin(async move {
                        ItemCompletion {
                            key,
                            result: store
                                .allocate_dynamic_ids(1023)
                                .await
                                .map_err(|e| e.to_string()),
                        }
                    }));
                }
            }
            Phase::Cold => {
                if !work.cold_pending {
                    let request = CreationPreparationRequest {
                        correlation: work.correlation,
                        key,
                        account: work.account.clone(),
                        wire: work.wire.clone(),
                        entity: work.entity.ok_or("creation character ID missing")?,
                        item_ids: work.items.take().ok_or("creation item IDs missing")?,
                        slot: work.wire.character_slot as u16,
                        maximum_slots: self.creation.maximum_slots,
                        generation: work.generation.clone(),
                        profile: self.creation.profile.clone(),
                        now_unix_millis: work.unix,
                    };
                    match self
                        .creation
                        .worker
                        .as_mut()
                        .ok_or("creation worker stopped")?
                        .try_submit(request)
                    {
                        Ok(()) => work.cold_pending = true,
                        Err(request) => {
                            work.items = Some(request.item_ids);
                        }
                    }
                }
            }
            Phase::Prepared => {
                let prepared = work.prepared.take().ok_or("creation proposal missing")?;
                let PreparedCharacterCreation {
                    frozen,
                    geometry,
                    unused_item_ids,
                } = prepared;
                match self
                    .players
                    .stage_creation(key, frozen, work.wire.character_slot as u16)
                {
                    Ok(()) => {
                        work.phase = Phase::Commit;
                        self.login_queue.push_back((key, PlayerIoAction::Create));
                    }
                    Err((error, frozen)) => {
                        work.prepared = Some(PreparedCharacterCreation {
                            frozen,
                            geometry,
                            unused_item_ids,
                        });
                        work.failure = Some(error);
                    }
                }
            }
            Phase::Rejected => {
                let disconnected = self
                    .sessions
                    .get(&key)
                    .ok_or("creation session disappeared")?
                    .disconnected;
                if disconnected || self.network_output.len() < self.limits.messages {
                    if !disconnected {
                        self.network_output.push_back(NetworkCommand::Send {
                            key,
                            queue: 9,
                            bytes: bace_wire::CharacterReply::CreateFailed(
                                work.response.ok_or("creation failure response missing")?,
                            )
                            .encode()
                            .map_err(|e| e.to_string())?,
                        });
                    }
                    self.creation.work.remove(&key);
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
