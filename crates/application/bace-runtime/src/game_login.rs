//! Durable character lifecycle composition, executed on bounded adapter capacity.
//! Requires verified accounts and gameplay-prepared saves. It does not implement
//! character appearance, starting gear or world/geometry admission policy.
use crate::game_lifecycle::{
    AdmissionRegistry, AdmissionState, GameLoginPhase, binding, expected_loading,
};
use bace_auth::AccountRecord;
use bace_db_postgres::{PgStore, StoreError};
use bace_gameplay_api::CharacterBinding;
use bace_persistence::{CharacterLease, OwnershipState, PlayerSummary};
use bace_session::SessionKey;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, SaveCodecError};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum GameLoginError {
    #[error("invalid authenticated identity")]
    InvalidIdentity,
    #[error("character lifecycle capacity exceeded")]
    Capacity,
    #[error("stale character session")]
    StaleSession,
    #[error("character lifecycle state does not permit this operation")]
    WrongState,
    #[error("account does not own requested character")]
    NotOwned,
    #[error("character preparation does not match authenticated account")]
    InvalidPreparation,
    #[error("pending character operation or world owner requires drain")]
    DrainRequired,
    #[error("durable operation outcome remains unresolved; do not create another character")]
    Unresolved,
    #[error("loaded character does not match durable identity")]
    CorruptIdentity,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Save(#[from] SaveCodecError),
    #[error(transparent)]
    Wire(#[from] bace_wire::WireError),
}
/// Server-prepared, borrowed creation input. The gameplay owner must validate
/// name, DAT-backed appearance/allocation, template, spawn and all starting items.
/// Retain this exact input until creation succeeds or its uncertain commit resolves.
pub struct PreparedCharacter<'a> {
    pub player: &'a PlayerSaveV1,
    pub slot: u16,
    pub items: &'a [(EntitySaveV1, u32)],
}
#[derive(Debug)]
pub struct LoadedPlayer {
    pub is_plussed: bool,
    pub key: SessionKey,
    pub binding: CharacterBinding,
    pub lease: CharacterLease,
    pub persisted_version: i64,
    pub player: bace_storage_codec::PlayerSaveV6,
    pub cached_experience: u64,
    pub inventory: Vec<LoadedPlayerItem>,
}
#[derive(Debug)]
pub struct LoadedPlayerItem {
    pub construction: Option<bace_storage_codec::FrozenCreatureConstructionV1>,
    pub source_destination: Option<u8>,
    pub enchantments: Vec<bace_storage_codec::FrozenEnchantmentV1>,
    pub entity: EntitySaveV1,
    pub location: bace_persistence::ItemLocation,
    pub persisted_version: i64,
    pub depth: u16,
    pub placement: bace_storage_codec::ItemPlacementV2,
}
#[derive(Clone, Debug)]
pub struct CommittedCharacter {
    pub key: SessionKey,
    pub object_id: u32,
    pub name: String,
    pub lease: CharacterLease,
}
/// No simulation or packet sends occur here. The outer owner runs these calls
/// through bounded I/O work and fences returned keys before world/network use.
/// Pending markers are installed before awaits and survive cancellation.
pub struct GameLoginService {
    store: PgStore,
    sessions: AdmissionRegistry,
    maximum_slots: u16,
}
impl GameLoginService {
    pub fn new(
        store: PgStore,
        capacity: usize,
        maximum_slots: u16,
    ) -> Result<Self, GameLoginError> {
        if maximum_slots == 0 || maximum_slots > 100 {
            return Err(GameLoginError::Capacity);
        }
        Ok(Self {
            store,
            sessions: AdmissionRegistry::new(capacity)?,
            maximum_slots,
        })
    }
    /// Call only for a successful, current-generation AuthenticationCompletion.
    /// AccountRecord alone is not a password proof; authentication owns that gate.
    pub fn register_authenticated(
        &mut self,
        key: SessionKey,
        account: &AccountRecord,
    ) -> Result<(), GameLoginError> {
        if account.disabled {
            return Err(GameLoginError::InvalidIdentity);
        }
        self.sessions
            .register(key, account.id, account.name.clone())
    }
    pub fn phase(&self, key: SessionKey) -> Result<GameLoginPhase, GameLoginError> {
        Ok(self.sessions.get(key)?.state.phase())
    }
    pub fn retire_roster(&mut self, key: SessionKey) -> Result<(), GameLoginError> {
        self.sessions.retire(key)
    }
    pub fn maximum_slots(&self) -> u16 {
        self.maximum_slots
    }
    pub fn account_name(&self, key: SessionKey) -> Result<&str, GameLoginError> {
        Ok(self.sessions.get(key)?.name.as_str())
    }
    pub async fn roster(&self, key: SessionKey) -> Result<Vec<PlayerSummary>, GameLoginError> {
        let session = self.sessions.get(key)?;
        if !matches!(session.state, AdmissionState::Roster) {
            return Err(GameLoginError::WrongState);
        }
        Ok(self.store.players_for_account(session.account.0).await?)
    }
    pub async fn allocate_character_id(&self, key: SessionKey) -> Result<u32, GameLoginError> {
        if self.phase(key)? != GameLoginPhase::Roster {
            return Err(GameLoginError::WrongState);
        }
        Ok(self.store.allocate_player_id().await?)
    }
    pub async fn create_prepared(
        &mut self,
        key: SessionKey,
        input: PreparedCharacter<'_>,
    ) -> Result<CommittedCharacter, GameLoginError> {
        let session = self.sessions.get(key)?;
        if !matches!(session.state, AdmissionState::Roster) {
            return Err(GameLoginError::WrongState);
        }
        if input.player.account_id != session.account.0 || input.slot >= self.maximum_slots {
            return Err(GameLoginError::InvalidPreparation);
        }
        // Validate every frozen object before changing lifecycle state.
        let encoded = input.player.encode()?;
        bace_wire::CharacterReply::Created {
            object_id: input.player.entity.object_id,
            name: input.player.name.clone(),
        }
        .encode()?;
        if input.items.len() > 1023 {
            return Err(GameLoginError::Capacity);
        }
        for (item, _) in input.items {
            item.validate()?;
        }
        let player_fingerprint = Sha256::digest(&encoded).into();
        let fingerprint = creation_fingerprint(&input, self.maximum_slots)?;
        self.sessions.set(
            key,
            AdmissionState::Creating {
                character_id: input.player.entity.object_id,
                fingerprint,
                player_fingerprint,
            },
        )?;
        match self
            .store
            .create_player(input.player, input.slot, self.maximum_slots, input.items)
            .await
        {
            Ok(lease) => {
                self.sessions.set(key, AdmissionState::Roster)?;
                Ok(CommittedCharacter {
                    key,
                    object_id: input.player.entity.object_id,
                    name: input.player.name.clone(),
                    lease,
                })
            }
            Err(error) => {
                // A reported rollback is safe to release. Unknown/cancelled commit
                // retains the exact character identity until reconciliation.
                if !matches!(error, StoreError::CommitUncertain(_)) {
                    self.sessions.set(key, AdmissionState::Roster)?;
                }
                Err(error.into())
            }
        }
    }
    /// Retries only the exact retained request/identity. The database account lock
    /// and unique player/slot keys serialize any still-running original attempt.
    /// A conflict is reconciled; it never allocates a second identity.
    pub async fn retry_creation(
        &mut self,
        key: SessionKey,
        input: PreparedCharacter<'_>,
    ) -> Result<CommittedCharacter, GameLoginError> {
        let AdmissionState::Creating {
            character_id,
            fingerprint,
            ..
        } = self.sessions.get(key)?.state
        else {
            return Err(GameLoginError::WrongState);
        };
        if character_id != input.player.entity.object_id
            || creation_fingerprint(&input, self.maximum_slots)? != fingerprint
        {
            return Err(GameLoginError::InvalidPreparation);
        }
        match self
            .store
            .create_player(input.player, input.slot, self.maximum_slots, input.items)
            .await
        {
            Ok(lease) => {
                self.sessions.set(key, AdmissionState::Roster)?;
                Ok(CommittedCharacter {
                    key,
                    object_id: character_id,
                    name: input.player.name.clone(),
                    lease,
                })
            }
            Err(StoreError::CommitUncertain(error)) => {
                Err(GameLoginError::Store(StoreError::CommitUncertain(error)))
            }
            Err(_) => self.resolve_creation(key).await,
        }
    }
    /// Observes durable identity after uncertain/cancelled creation. Absence is
    /// not proof of rollback; it retains the marker and returns Unresolved.
    pub async fn resolve_creation(
        &mut self,
        key: SessionKey,
    ) -> Result<CommittedCharacter, GameLoginError> {
        let session = self.sessions.get(key)?;
        let AdmissionState::Creating {
            character_id,
            player_fingerprint,
            ..
        } = session.state
        else {
            return Err(GameLoginError::WrongState);
        };
        if !self
            .store
            .account_owns_player(session.account.0, character_id)
            .await?
        {
            return Err(GameLoginError::Unresolved);
        }
        let stored = self
            .store
            .load(character_id)
            .await?
            .ok_or(GameLoginError::Unresolved)?;
        if <[u8; 32]>::from(Sha256::digest(&stored.bytes)) != player_fingerprint {
            return Err(GameLoginError::CorruptIdentity);
        }
        let player = PlayerSaveV1::decode(&stored.bytes)?;
        let lease = self
            .store
            .character_lease(character_id)
            .await?
            .ok_or(GameLoginError::Unresolved)?;
        if lease.state != OwnershipState::Offline {
            return Err(GameLoginError::DrainRequired);
        }
        self.sessions.set(key, AdmissionState::Roster)?;
        Ok(CommittedCharacter {
            key,
            object_id: character_id,
            name: player.name,
            lease,
        })
    }
    /// Fences offline writers before returning any bytes to asset/world admission.
    pub async fn load_character(
        &mut self,
        key: SessionKey,
        character_id: u32,
    ) -> Result<LoadedPlayer, GameLoginError> {
        let session = self.sessions.get(key)?;
        if !matches!(session.state, AdmissionState::Roster) {
            return Err(GameLoginError::WrongState);
        }
        if !self
            .store
            .account_owns_player(session.account.0, character_id)
            .await?
        {
            return Err(GameLoginError::NotOwned);
        }
        let previous = self
            .store
            .character_lease(character_id)
            .await?
            .ok_or(GameLoginError::NotOwned)?;
        if previous.state != OwnershipState::Offline {
            return Err(GameLoginError::DrainRequired);
        }
        self.sessions
            .set(key, AdmissionState::Loading { previous })?;
        match self.store.begin_login(previous).await {
            Ok(load) => {
                self.sessions
                    .set(key, AdmissionState::AwaitingWorld { lease: load.lease })?;
                self.decoded_load(key, load).await
            }
            Err(error) => {
                if !matches!(error, StoreError::CommitUncertain(_)) {
                    self.sessions.set(key, AdmissionState::Roster)?;
                }
                Err(error.into())
            }
        }
    }
    async fn decoded_load(
        &self,
        key: SessionKey,
        load: bace_persistence::CharacterLoad,
    ) -> Result<LoadedPlayer, GameLoginError> {
        let session = self.sessions.get(key)?;
        let player = bace_storage_codec::PlayerSaveV6::decode_or_migrate(&load.snapshot.bytes)?;
        if player.player.account_id != session.account.0
            || player.player.entity.object_id != load.lease.character_id
            || load.snapshot.object_id != load.lease.character_id
        {
            return Err(GameLoginError::CorruptIdentity);
        }
        let loaded_inventory = self
            .store
            .load_character_inventory(load.lease, bace_persistence::InventoryLoadLimits::default())
            .await?;
        let mut inventory = Vec::with_capacity(loaded_inventory.items.len());
        for item in loaded_inventory.items {
            let expected = bace_storage_codec::ItemPlacementV2::Contained {
                container: item.location.container,
                slot: item.location.slot,
                pack_slot: item.pack_slot,
                equipped: item.equipped,
            };
            let saved = bace_storage_codec::ItemSaveV5::decode_or_migrate(
                &item.aggregate.bytes,
                Some(expected.clone()),
            )?;
            let entity = saved.previous.previous.previous.entity;
            if saved.previous.previous.previous.placement != expected
                || entity.object_id != item.aggregate.object_id
            {
                return Err(GameLoginError::CorruptIdentity);
            }
            inventory.push(LoadedPlayerItem {
                construction: saved.previous.construction,
                source_destination: saved.source_destination,
                entity,
                location: item.location,
                persisted_version: item.aggregate.persisted_version,
                depth: item.depth,
                placement: saved.previous.previous.previous.placement,
                enchantments: saved.previous.previous.enchantments,
            });
        }
        Ok(LoadedPlayer {
            is_plussed: self
                .store
                .player_is_plussed(load.lease, session.account.0)
                .await?,
            key,
            binding: binding(key, session.account, load.lease.character_id),
            lease: load.lease,
            persisted_version: load.snapshot.persisted_version,
            player,
            cached_experience: load.cached_xp,
            inventory,
        })
    }
    /// After a cancelled/uncertain begin_login, retry the same fenced attempt.
    /// A successful fence is retained; no second actor is returned on a repeat.
    pub async fn resolve_loading(
        &mut self,
        key: SessionKey,
    ) -> Result<LoadedPlayer, GameLoginError> {
        let session = self.sessions.get(key)?;
        let AdmissionState::Loading { previous } = session.state else {
            return Err(GameLoginError::WrongState);
        };
        match self.store.begin_login(previous).await {
            Ok(load) => {
                self.sessions
                    .set(key, AdmissionState::AwaitingWorld { lease: load.lease })?;
                self.decoded_load(key, load).await
            }
            Err(StoreError::OwnershipConflict) => {
                let expected = expected_loading(previous).ok_or(GameLoginError::Unresolved)?;
                if self.store.character_lease(previous.character_id).await? != Some(expected) {
                    return Err(GameLoginError::Unresolved);
                }
                // Cached offline XP must be loaded atomically with the original fence;
                // without that receipt this state is safely abortable, not playable.
                self.sessions
                    .set(key, AdmissionState::AwaitingWorld { lease: expected })?;
                Err(GameLoginError::Unresolved)
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Invoke only after the single simulation owner has committed the character
    /// with admitted DAT geometry/assets and durable admission work has completed.
    /// The explicit binding is a trusted owner receipt, never client input.
    pub async fn world_admitted(
        &mut self,
        key: SessionKey,
        receipt: CharacterBinding,
    ) -> Result<CharacterLease, GameLoginError> {
        let session = self.sessions.get(key)?;
        let lease = match session.state {
            AdmissionState::AwaitingWorld { lease } | AdmissionState::Admitting { lease } => lease,
            _ => return Err(GameLoginError::WrongState),
        };
        if receipt != binding(key, session.account, lease.character_id) {
            return Err(GameLoginError::NotOwned);
        }
        self.sessions
            .set(key, AdmissionState::Admitting { lease })?;
        match self.store.finish_login(lease).await {
            Ok(online) => {
                self.sessions
                    .set(key, AdmissionState::Online { lease: online })?;
                Ok(online)
            }
            Err(StoreError::OwnershipConflict) => {
                let expected = CharacterLease {
                    state: OwnershipState::Online,
                    ..lease
                };
                if self.store.character_lease(lease.character_id).await? == Some(expected) {
                    self.sessions
                        .set(key, AdmissionState::Online { lease: expected })?;
                    Ok(expected)
                } else {
                    Err(GameLoginError::Unresolved)
                }
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Only before world ownership was committed; callers retain returned loaded
    /// data until this abort succeeds. Failed aborts remain fenced for retry.
    pub async fn abort_loading(
        &mut self,
        key: SessionKey,
    ) -> Result<CharacterLease, GameLoginError> {
        let lease = match self.sessions.get(key)?.state {
            AdmissionState::AwaitingWorld { lease } | AdmissionState::Aborting { lease } => lease,
            _ => return Err(GameLoginError::WrongState),
        };
        self.sessions.set(key, AdmissionState::Aborting { lease })?;
        let result = self.store.abort_loading(lease).await;
        let offline = match result {
            Ok(offline) => offline,
            Err(StoreError::OwnershipConflict) => {
                let expected = CharacterLease {
                    state: OwnershipState::Offline,
                    epoch: lease
                        .epoch
                        .checked_add(1)
                        .ok_or(GameLoginError::Unresolved)?,
                    ..lease
                };
                if self.store.character_lease(lease.character_id).await? != Some(expected) {
                    return Err(GameLoginError::Unresolved);
                }
                expected
            }
            Err(error) => return Err(error.into()),
        };
        self.sessions.set(key, AdmissionState::Roster)?;
        Ok(offline)
    }
    pub fn online_lease(&self, key: SessionKey) -> Result<CharacterLease, GameLoginError> {
        match self.sessions.get(key)?.state {
            AdmissionState::Online { lease } => Ok(lease),
            _ => Err(GameLoginError::WrongState),
        }
    }
    /// Accept only an offline receipt after the save owner durably drained logout.
    pub async fn logout_drained(
        &mut self,
        key: SessionKey,
        offline: CharacterLease,
    ) -> Result<(), GameLoginError> {
        let online = self.online_lease(key)?;
        if offline.character_id != online.character_id
            || offline.state != OwnershipState::Offline
            || offline.epoch
                != online
                    .epoch
                    .checked_add(1)
                    .ok_or(GameLoginError::Unresolved)?
            || self.store.character_lease(offline.character_id).await? != Some(offline)
        {
            return Err(GameLoginError::DrainRequired);
        }
        self.sessions.set(key, AdmissionState::Roster)
    }
}

fn creation_fingerprint(
    input: &PreparedCharacter<'_>,
    maximum_slots: u16,
) -> Result<[u8; 32], GameLoginError> {
    if input.items.len() > 1023 {
        return Err(GameLoginError::Capacity);
    }
    let player = input.player.encode()?;
    let mut total = player.len();
    let mut digest = Sha256::new();
    digest.update((player.len() as u64).to_le_bytes());
    digest.update(&player);
    digest.update(input.slot.to_le_bytes());
    digest.update(maximum_slots.to_le_bytes());
    for (item, slot) in input.items {
        let bytes = item.encode_item()?;
        total = total
            .checked_add(bytes.len())
            .ok_or(GameLoginError::Capacity)?;
        if total > 64 * 1024 * 1024 {
            return Err(GameLoginError::Capacity);
        }
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
        digest.update(slot.to_le_bytes());
    }
    Ok(digest.finalize().into())
}
