use super::*;
use crate::player_preparation_worker::{
    PlayerPreparationRequest, PlayerPreparationResult, PreparedPlayerCold,
};
use crate::player_service::PlayerIoResult;
use bace_gameplay_api::social::SocialIdentity;
use bace_persistence::{CharacterLease, OnlineLoginReceipt};
use bace_simulation::{Command, SocialControl, SocialControlAction};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Metadata,
    Region,
    Friends,
    Cold,
    Admission,
    Online,
    RegisterSaves,
    Receipt,
    Snapshot,
    Entry,
    QueuedEntry,
    Entered,
}
pub(super) struct Loading {
    pub loaded: Arc<LoadedPlayer>,
    pub phase: Phase,
    pub account_created: Option<i64>,
    pub friends: Vec<SocialIdentity>,
    pub friends_next: usize,
    pub cold: Option<PreparedPlayerCold>,
    pub appearance: Option<crate::player_entry::PreparedEntryAppearanceAssets>,
    pub character_assets: Option<Arc<crate::character_assets::PreparedCharacterAssets>>,
    pub enchantments: Vec<bace_gameplay_api::EnchantmentProjection>,
    pub spell_table: Option<Arc<bace_dat::SpellTable>>,
    pub online: Option<CharacterLease>,
    pub receipt: Option<OnlineLoginReceipt>,
    pub snapshot: Option<Arc<bace_simulation::PlayerReadSnapshot>>,
    pub cold_token: Option<u64>,
    pub region_requested: bool,
    pub settle_attempts: u16,
    pub first_settle_tick: Option<u64>,
}
pub(super) struct MetadataCompletion {
    key: SessionKey,
    result: Result<(Option<i64>, Vec<SocialIdentity>), String>,
}
pub(super) struct ReceiptCompletion {
    key: SessionKey,
    result: Result<OnlineLoginReceipt, String>,
}
impl GameRuntime {
    pub(super) fn poll_lifecycle(&mut self, _elapsed: Duration, unix: u64) -> Result<(), String> {
        if let Some(completion) = ready(&mut self.login_job) {
            let key = self
                .login_key
                .take()
                .ok_or("login completion without owner")?;
            let result = self.players.accept_io(completion).map_err(|completion| {
                self.login_job = Some(Box::pin(async { *completion }));
                "mismatched retained login completion"
            })?;
            if !self.accept_creation_io(key, &result)? {
                match result {
                    Ok(PlayerIoResult::Loaded(loaded)) => {
                        let session = self
                            .sessions
                            .get_mut(&key)
                            .ok_or("loaded session missing")?;
                        session.loading = Some(Loading {
                            loaded,
                            phase: Phase::Metadata,
                            account_created: None,
                            friends: vec![],
                            friends_next: 0,
                            cold: None,
                            appearance: None,
                            character_assets: None,
                            enchantments: vec![],
                            spell_table: None,
                            online: None,
                            receipt: None,
                            snapshot: None,
                            cold_token: None,
                            region_requested: false,
                            settle_attempts: 0,
                            first_settle_tick: None,
                        });
                    }
                    Ok(PlayerIoResult::Online(lease)) => {
                        let loading = self
                            .sessions
                            .get_mut(&key)
                            .and_then(|s| s.loading.as_mut())
                            .ok_or("online loading owner missing")?;
                        loading.online = Some(lease);
                        loading.phase = Phase::RegisterSaves;
                    }
                    Ok(PlayerIoResult::LoggedOut(_)) => {
                        self.logout
                            .get_mut(&key)
                            .ok_or("logout owner missing")?
                            .adopted = true;
                    }
                    Ok(PlayerIoResult::Retired) => {
                        self.forget_social_session(key)?;
                        self.forget_staff_session(key)?;
                        self.forget_progression_session(key)?;
                        self.forget_inventory_session(key)?;
                        self.forget_crafting_session(key)?;
                        self.forget_skill_device_session(key)?;
                        self.forget_attribute_transfer_session(key)?;
                        self.forget_pet_session(key)?;
                        self.forget_magic_session(key)?;
                        self.sessions.remove(&key);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.sessions
                            .get_mut(&key)
                            .ok_or("failed login session missing")?
                            .failure = Some(error);
                    }
                }
            }
        }
        if let Some(done) = ready(&mut self.metadata_job) {
            let session = self
                .sessions
                .get_mut(&done.key)
                .ok_or("metadata session missing")?;
            match done.result {
                Ok((created, friends)) => {
                    let loading = session.loading.as_mut().ok_or("metadata owner missing")?;
                    loading.account_created = created;
                    loading.friends = friends;
                    loading.phase = Phase::Region;
                }
                Err(error) => session.failure = Some(error),
            }
        }
        if let Some(done) = ready(&mut self.receipt_job) {
            let session = self
                .sessions
                .get_mut(&done.key)
                .ok_or("receipt session missing")?;
            match done.result {
                Ok(receipt) => {
                    let loading = session.loading.as_mut().ok_or("receipt owner missing")?;
                    if loading.online != Some(receipt.lease) {
                        return Err("online login receipt lease mismatch".into());
                    }
                    self.players.install_login_receipt(done.key, receipt)?;
                    loading.receipt = Some(receipt);
                    loading.phase = Phase::Snapshot;
                }
                Err(error) => session.failure = Some(error),
            }
        }
        if self.unexpected_cold.is_some() {
            return Err("unrelated cold preparation retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(result) = self.preparation.try_recv()? else {
                break;
            };
            let done = match result {
                PlayerPreparationResult::Admission(done) => done,
                PlayerPreparationResult::BindingMotion(done) => {
                    self.accept_binding_motion_completion(done)?;
                    continue;
                }
            };
            let valid = self
                .sessions
                .get(&done.key)
                .and_then(|s| s.loading.as_ref())
                .is_some_and(|l| {
                    l.cold_token == Some(done.correlation) && l.loaded.binding == done.binding
                });
            if !valid {
                self.unexpected_cold = Some(*done);
                return Err("cold player completion fence mismatch; preparation retained".into());
            }
            let session = self
                .sessions
                .get_mut(&done.key)
                .expect("validated cold session");
            let loading = session.loading.as_mut().expect("validated cold owner");
            loading.cold_token = None;
            match done.result {
                Ok(prepared) => {
                    loading.cold = Some(prepared);
                    loading.phase = Phase::Admission;
                }
                Err(error) => session.failure = Some(error),
            }
        }
        self.accept_lifecycle_outputs()?;
        // At most four cold player owners exist. Rotate the bounded selection so
        // a waiting first region cannot starve a later prepared character.
        let mut keys: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                s.failure.is_none()
                    && s.loading
                        .as_ref()
                        .is_some_and(|l| l.phase != Phase::Entered)
            })
            .map(|(k, _)| *k)
            .collect();
        if let Some(last) = self.lifecycle_cursor {
            let offset = keys.partition_point(|key| *key <= last);
            keys.rotate_left(offset);
        }
        for key in keys.into_iter().take(self.limits.work_per_poll) {
            self.lifecycle_cursor = Some(key);
            if let Err(error) = self.advance_player(key, unix) {
                self.sessions
                    .get_mut(&key)
                    .expect("retained session")
                    .failure = Some(error);
            }
        }
        if self.login_job.is_none()
            && let Some((key, action)) = self.login_queue.pop_front()
        {
            match self.players.begin_io(key, action) {
                Ok(work) => {
                    self.login_key = Some(key);
                    self.login_job = Some(Box::pin(work.execute()));
                }
                Err(error) => {
                    self.login_queue.push_front((key, action));
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    fn advance_player(&mut self, key: SessionKey, unix: u64) -> Result<(), String> {
        let phase = self.sessions[&key]
            .loading
            .as_ref()
            .expect("selected loading")
            .phase;
        match phase {
            Phase::Metadata if self.metadata_job.is_none() => {
                let loaded = self.sessions[&key]
                    .loading
                    .as_ref()
                    .expect("loading")
                    .loaded
                    .clone();
                let store = self.bootstrap.store.clone();
                self.metadata_job = Some(Box::pin(async move {
                    let result = async {
                        let created = store
                            .account_creation_time(loaded.binding.account)
                            .await
                            .map_err(|e| e.to_string())?;
                        let ids = &loaded.player.social.friends;
                        let rows = store
                            .lookup_player_identities(ids)
                            .await
                            .map_err(|e| e.to_string())?;
                        if rows.len() != ids.len() {
                            return Err(
                                "saved friend identity missing from accepted database".into()
                            );
                        }
                        let friends = rows
                            .into_iter()
                            .map(|p| SocialIdentity {
                                character: bace_types::EntityId(p.object_id),
                                account: bace_types::AccountId(p.account_id),
                                name: p.name,
                            })
                            .collect();
                        Ok((created, friends))
                    }
                    .await;
                    MetadataCompletion { key, result }
                }));
            }
            Phase::Region => self.prepare_region_for_player(key)?,
            Phase::Friends => {
                if self.social_controls.values().any(|k| *k == key) {
                    return Ok(());
                }
                let loading = self.sessions[&key].loading.as_ref().expect("loading");
                if loading.friends_next == loading.friends.len() {
                    self.sessions
                        .get_mut(&key)
                        .expect("session")
                        .loading
                        .as_mut()
                        .expect("loading")
                        .phase = Phase::Cold;
                    return Ok(());
                }
                let identity = loading.friends[loading.friends_next].clone();
                let token = self.token()?;
                if self
                    .simulation
                    .input()
                    .try_submit(Command::SocialControl(SocialControl {
                        sequence: token,
                        action: SocialControlAction::CacheIdentity(identity),
                    }))
                    .is_ok()
                {
                    self.social_controls.insert(token, key);
                }
            }
            Phase::Cold => {
                let loading = self.sessions[&key].loading.as_ref().expect("loading");
                if loading.cold_token.is_some() {
                    return Ok(());
                }
                let Some(world) = &self.world else {
                    return Ok(());
                };
                let block = landblock(&loading.loaded)?;
                let Some(region) = world.regions.prepared_region(block) else {
                    return Ok(());
                };
                let geometry = region.geometry.clone();
                let policy = self.assets.policy.prepare(
                    &loading.loaded,
                    &self.sessions[&key].account,
                    loading.account_created,
                    unix,
                )?;
                let loaded = loading.loaded.clone();
                let token = self.token()?;
                let request = PlayerPreparationRequest {
                    generation: self.bootstrap.pack.generation.clone(),
                    correlation: token,
                    loaded,
                    geometry,
                    spell_rows: self.assets.spell_rows.clone(),
                    component_templates: self.assets.component_templates.clone(),
                    projectile_shapes: self.assets.projectile_shapes.clone(),
                    policy,
                    now_unix_millis: unix,
                };
                if self.preparation.try_submit(request).is_ok() {
                    self.sessions
                        .get_mut(&key)
                        .expect("session")
                        .loading
                        .as_mut()
                        .expect("loading")
                        .cold_token = Some(token);
                }
            }
            Phase::Admission => {
                let loading = self
                    .sessions
                    .get_mut(&key)
                    .expect("session")
                    .loading
                    .as_mut()
                    .expect("loading");
                let Some(cold) = loading.cold.take() else {
                    return Ok(());
                };
                let PreparedPlayerCold {
                    admission,
                    appearance,
                    character_assets,
                    enchantments,
                    spell_table,
                } = cold;
                match self.players.stage_admission(key, admission) {
                    Ok(_) => {
                        loading.appearance = Some(appearance);
                        loading.character_assets = Some(character_assets);
                        loading.enchantments = enchantments;
                        loading.spell_table = Some(spell_table);
                    }
                    Err((error, admission)) => {
                        loading.cold = Some(PreparedPlayerCold {
                            admission: *admission,
                            appearance,
                            character_assets,
                            enchantments,
                            spell_table,
                        });
                        return Err(error);
                    }
                }
            }
            Phase::Online => {
                if !self.login_queue.iter().any(|(k, _)| *k == key) && self.login_key != Some(key) {
                    self.login_queue.push_back((key, PlayerIoAction::Online));
                }
            }
            Phase::RegisterSaves => {
                let loading = self
                    .sessions
                    .get_mut(&key)
                    .expect("session")
                    .loading
                    .as_mut()
                    .expect("loading");
                self.online_saves.register_loaded(
                    &loading.loaded,
                    loading.online.ok_or("online lease missing")?,
                    self.last_elapsed,
                )?;
                loading.phase = Phase::Receipt;
            }
            Phase::Receipt if self.receipt_job.is_none() => {
                let lease = self.sessions[&key]
                    .loading
                    .as_ref()
                    .expect("loading")
                    .online
                    .ok_or("online lease missing")?;
                let store = self.bootstrap.store.clone();
                self.receipt_job = Some(Box::pin(async move {
                    ReceiptCompletion {
                        key,
                        result: store
                            .online_login_receipt(lease)
                            .await
                            .map_err(|e| e.to_string()),
                    }
                }));
            }
            Phase::Snapshot if self.snapshot.is_none() => {
                let binding = self.sessions[&key]
                    .loading
                    .as_ref()
                    .expect("loading")
                    .loaded
                    .binding;
                let token = self.token()?;
                if self
                    .simulation
                    .input()
                    .try_submit(Command::PlayerSnapshot(
                        bace_simulation::PlayerSnapshotRequest {
                            correlation: token,
                            binding,
                            operation: None,
                        },
                    ))
                    .is_ok()
                {
                    self.snapshot = Some((token, key));
                }
            }
            Phase::Entry => self.prepare_entry(key, unix)?,
            _ => {}
        }
        Ok(())
    }
    fn accept_lifecycle_outputs(&mut self) -> Result<(), String> {
        if self.unexpected_admission.is_some()
            || self.unexpected_entered.is_some()
            || self.unexpected_social_control.is_some()
        {
            return Err("unrelated lifecycle output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.player_admission_outcomes().try_recv() else {
                break;
            };
            let accepted = outcome.result.is_ok();
            let rejection = outcome
                .result
                .as_ref()
                .err()
                .map(|(reason, _)| format!("{reason:?}"));
            match self.players.accept_admission(outcome) {
                Ok(key) => {
                    if accepted {
                        self.sessions
                            .get_mut(&key)
                            .ok_or("admitted session missing")?
                            .loading
                            .as_mut()
                            .ok_or("admitted loading missing")?
                            .phase = Phase::Online;
                    } else {
                        self.sessions
                            .get_mut(&key)
                            .ok_or("rejected session missing")?
                            .failure = Some(format!(
                            "world rejected preparation: {}; original admission retained",
                            rejection.as_deref().unwrap_or("missing owner error")
                        ));
                    }
                }
                Err(outcome) => {
                    self.unexpected_admission = Some(outcome);
                    return Err("unrelated player admission output retained".into());
                }
            }
        }
        for _ in 0..self.limits.work_per_poll {
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            let Ok(outcome) = self.simulation.player_entered_outcomes().try_recv() else {
                break;
            };
            match self.players.accept_entered(outcome) {
                Ok(key) => {
                    if self
                        .players
                        .binding_for(key)
                        .is_some_and(|b| self.players.entered(b.actor))
                    {
                        if !self.sessions[&key].disconnected {
                            self.network_output
                                .push_back(NetworkCommand::EnterWorldCommitted { key });
                        }
                        let loading = self
                            .sessions
                            .get_mut(&key)
                            .ok_or("entered session missing")?
                            .loading
                            .as_mut()
                            .ok_or("entered loading missing")?;
                        loading.phase = Phase::Entered;
                        loading.appearance = None;
                        loading.character_assets = None;
                        loading.snapshot = None;
                        loading.enchantments.clear();
                    }
                }
                Err((error, outcome)) => {
                    self.unexpected_entered = Some(outcome);
                    return Err(error);
                }
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.social_control_outcomes().try_recv() else {
                break;
            };
            if self.allegiance_owns_control(&outcome) {
                self.accept_allegiance_control(outcome)?;
                continue;
            }
            let Some(key) = self.social_controls.remove(&outcome.sequence) else {
                self.unexpected_social_control = Some(outcome);
                return Err("unrelated social control output retained".into());
            };
            let session = self
                .sessions
                .get_mut(&key)
                .ok_or("friend cache session missing")?;
            match outcome.result {
                Ok(_) => {
                    session
                        .loading
                        .as_mut()
                        .ok_or("friend loading missing")?
                        .friends_next += 1
                }
                Err(e) => session.failure = Some(format!("friend cache rejected: {e:?}")),
            }
        }
        Ok(())
    }
}
pub(super) fn landblock(loaded: &LoadedPlayer) -> Result<u16, String> {
    let position = loaded
        .player
        .player
        .entity
        .state
        .properties
        .positions
        .iter()
        .find(|p| p.id == 1)
        .ok_or("saved authoritative location missing")?;
    Ok((position.value.obj_cell_id >> 16) as u16)
}
