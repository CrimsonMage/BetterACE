//! One bounded cold creation lane. This thread prepares immutable proposals;
//! PostgreSQL identity/name uniqueness and simulation entry remain separate gates.
use crate::{
    character_creation::FrozenCharacterCreation,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
};
use bace_session::SessionKey;
use bace_storage_codec::PackGeneration;
use bace_types::{AccountId, EntityId};
use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc},
    thread,
};
pub struct CreationPreparationRequest {
    pub correlation: u64,
    pub key: SessionKey,
    pub account: Arc<bace_auth::AccountRecord>,
    pub wire: bace_wire::CharacterCreateRequest,
    pub entity: EntityId,
    pub item_ids: Vec<u32>,
    pub slot: u16,
    pub maximum_slots: u16,
    pub generation: Arc<PackGeneration>,
    pub profile: Arc<bace_content::CharacterStartProfileV1>,
    pub now_unix_millis: u64,
}
pub struct PreparedCharacterCreation {
    pub frozen: FrozenCharacterCreation,
    pub geometry: Arc<bace_physics::GeometryRegion>,
    pub unused_item_ids: Vec<u32>,
}
pub struct CreationPreparationCompletion {
    pub correlation: u64,
    pub key: SessionKey,
    pub account: AccountId,
    pub entity: EntityId,
    pub slot: u16,
    /// Retains the accepted immutable input generation through caller adoption.
    pub generation: Arc<PackGeneration>,
    pub result: Result<PreparedCharacterCreation, crate::creation_assets::CreationFailure>,
    /// Retains exact allocated IDs, trusted clock and immutable inputs for retry
    /// or explicit allocator-gap accounting; failure never fabricates a new job.
    pub request: Box<CreationPreparationRequest>,
}
pub struct CreationPreparationWorker {
    jobs: mpsc::SyncSender<CreationPreparationRequest>,
    results: mpsc::Receiver<CreationPreparationCompletion>,
    thread: thread::JoinHandle<()>,
    outstanding: BTreeSet<u64>,
    capacity: usize,
}
impl CreationPreparationWorker {
    pub fn start(manifest: RegionAssetManifest, capacity: usize) -> Result<Self, String> {
        let mut assets = None;
        Self::start_with(capacity, move |request| {
            let correlation = request.correlation;
            let key = request.key;
            let account = request.account.id;
            let entity = request.entity;
            let slot = request.slot;
            let generation = request.generation.clone();
            let result = assets
                .get_or_insert_with(|| VerifiedRegionAssets::open(&manifest))
                .as_mut()
                .map_err(|e| crate::creation_assets::CreationFailure::unavailable(e.clone()))
                .and_then(|assets| prepare(assets, &request));
            CreationPreparationCompletion {
                correlation,
                key,
                account,
                entity,
                slot,
                generation,
                result,
                request: Box::new(request),
            }
        })
    }
    fn start_with(
        capacity: usize,
        mut prepare: impl FnMut(CreationPreparationRequest) -> CreationPreparationCompletion
        + Send
        + 'static,
    ) -> Result<Self, String> {
        if !(1..=4).contains(&capacity) {
            return Err("creation preparation capacity must be1..4".into());
        }
        let (jobs, inbox) = mpsc::sync_channel::<CreationPreparationRequest>(capacity);
        let (outbox, results) = mpsc::sync_channel(capacity);
        let thread = thread::Builder::new()
            .name("bace-creation-preparation".into())
            .spawn(move || {
                while let Ok(job) = inbox.recv() {
                    if outbox.send(prepare(job)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            jobs,
            results,
            thread,
            outstanding: BTreeSet::new(),
            capacity,
        })
    }
    pub fn try_submit(
        &mut self,
        request: CreationPreparationRequest,
    ) -> Result<(), Box<CreationPreparationRequest>> {
        if request.correlation == 0
            || request.key.generation == 0
            || self.outstanding.len() >= self.capacity
            || self.outstanding.contains(&request.correlation)
            || request.account.disabled
            || request.account.id.0 == 0
            || request.generation.revision() == 0
            || request.maximum_slots == 0
            || request.maximum_slots > 100
            || request.slot >= request.maximum_slots
            || u32::from(request.slot) != request.wire.character_slot
            || request.wire.skill_advancement_classes.len() != 55
            || request.item_ids.len() > 1023
            || request
                .item_ids
                .iter()
                .any(|id| *id == 0 || *id == u32::MAX)
            || request.profile.validate().is_err()
            || crate::character_preparation::creation_request(
                &request.wire,
                &request.account,
                request.entity.0,
            )
            .is_err()
            || crate::creation_assets::creation_birth_date(request.now_unix_millis).is_err()
        {
            return Err(Box::new(request));
        }
        let correlation = request.correlation;
        match self.jobs.try_send(request) {
            Ok(()) => {
                self.outstanding.insert(correlation);
                Ok(())
            }
            Err(mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request)) => {
                Err(Box::new(request))
            }
        }
    }
    pub fn try_recv(&mut self) -> Result<Option<CreationPreparationCompletion>, String> {
        match self.results.try_recv() {
            Ok(value) => {
                if !self.outstanding.remove(&value.correlation) {
                    return Err("creation completion fence mismatch".into());
                }
                Ok(Some(value))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("creation worker disconnected; outstanding correlations remain owned".into())
            }
        }
    }
    pub fn pending(&self) -> usize {
        self.outstanding.len()
    }
    pub fn try_shutdown(self) -> Result<thread::JoinHandle<()>, Box<Self>> {
        if !self.outstanding.is_empty() {
            return Err(Box::new(self));
        }
        drop(self.jobs);
        drop(self.results);
        Ok(self.thread)
    }
}
fn prepare(
    assets: &mut VerifiedRegionAssets,
    request: &CreationPreparationRequest,
) -> Result<PreparedCharacterCreation, crate::creation_assets::CreationFailure> {
    use crate::creation_assets::CreationFailure as Failure;
    let intent = crate::character_preparation::creation_request(
        &request.wire,
        &request.account,
        request.entity.0,
    )
    .map_err(|e| Failure::corrupt(e.to_string()))?;
    let closure = assets
        .prepare_creation_assets(&request.generation, intent.heritage, &request.profile)
        .map_err(Failure::unavailable)?;
    if closure.content_generation != request.generation.revision() {
        return Err(Failure::unavailable("creation asset generation changed"));
    }
    let approval = closure.names.approve(&intent.name).map_err(|e| Failure {
        response: 4,
        message: format!("creation name rejected: {e:?}"),
    })?;
    let mut character =
        bace_character::prepare_character(&intent, &closure.creation, &approval, &request.item_ids)
            .map_err(|e| Failure::corrupt(format!("character factory: {e:?}")))?;
    crate::creation_assets::stamp_creation_birth(&mut character, request.now_unix_millis)
        .map_err(Failure::corrupt)?;
    let geometry = assets
        .validate_creation_geometry(&character)
        .map_err(Failure::unavailable)?;
    let used: BTreeSet<_> = character.possessions.iter().map(|p| p.entity).collect();
    let unused_item_ids = request
        .item_ids
        .iter()
        .copied()
        .filter(|id| !used.contains(id))
        .collect();
    let frozen = crate::character_creation::freeze_creation(character)
        .map_err(|e| Failure::corrupt(e.to_string()))?;
    Ok(PreparedCharacterCreation {
        frozen,
        geometry,
        unused_item_ids,
    })
}
#[cfg(test)]
mod tests;
