//! One retained lifecycle SQL owner while canonical replication stays responsive.
use super::*;
#[derive(Clone, Copy, Debug)]
pub enum PlayerIoAction {
    Roster,
    Allocate,
    Create,
    Load(u32),
    ResolveLoad,
    Online,
    Abort,
    Logout(CharacterLease),
    Retire,
}
#[derive(Debug)]
pub enum PlayerIoResult {
    Roster,
    Allocated(u32),
    Created(u32),
    /// Definite rollback: exact frozen proposal released, source response code.
    CreationRejected(u32),
    Loaded(Arc<LoadedPlayer>),
    Online(CharacterLease),
    Aborted(CharacterLease),
    LoggedOut(CharacterLease),
    Retired,
}
pub struct PlayerIoWork {
    turbine_chat: bool,
    token: u64,
    key: SessionKey,
    login: GameLoginService,
    action: PlayerIoAction,
    binding: Option<CharacterBinding>,
    creation: Option<(crate::character_creation::FrozenCharacterCreation, u16)>,
}
pub struct PlayerIoCompletion {
    token: u64,
    pub key: SessionKey,
    login: GameLoginService,
    creation: Option<(crate::character_creation::FrozenCharacterCreation, u16)>,
    result: Result<PlayerIoResult, String>,
    packet: Option<NetworkCommand>,
}
impl PlayerIoWork {
    /// Keep this future until completion. It owns the original lease markers and
    /// exact creation bytes; cancellation is not permission to start another job.
    pub async fn execute(mut self) -> PlayerIoCompletion {
        let mut packet = None;
        let result = match self.action {
            PlayerIoAction::Roster => {
                match crate::game_messages::roster_command_with_chat(
                    &self.login,
                    self.key,
                    self.turbine_chat,
                )
                .await
                {
                    Ok(command) => {
                        packet = Some(command);
                        Ok(PlayerIoResult::Roster)
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            PlayerIoAction::Allocate => self
                .login
                .allocate_character_id(self.key)
                .await
                .map(PlayerIoResult::Allocated)
                .map_err(|e| e.to_string()),
            PlayerIoAction::Create => {
                let (creation, slot) = self.creation.as_ref().expect("admitted exact creation");
                let input = crate::game_login::PreparedCharacter {
                    player: &creation.player,
                    items: &creation.items,
                    slot: *slot,
                };
                let result = match self.login.phase(self.key) {
                    Ok(crate::game_lifecycle::GameLoginPhase::Roster) => {
                        self.login.create_prepared(self.key, input).await
                    }
                    Ok(_) => self.login.retry_creation(self.key, input).await,
                    Err(error) => Err(error),
                };
                match result {
                    Ok(created) => match crate::game_messages::creation_command(&created) {
                        Ok(command) => {
                            self.creation = None;
                            packet = Some(command);
                            Ok(PlayerIoResult::Created(created.object_id))
                        }
                        Err(error) => Err(error.to_string()),
                    },
                    Err(error)
                        if self.login.phase(self.key).ok()
                            == Some(crate::game_lifecycle::GameLoginPhase::Roster) =>
                    {
                        let code = match &error {
                            crate::game_login::GameLoginError::Store(error) => {
                                match error.player_creation_conflict() {
                                    Some(bace_db_postgres::PlayerCreationConflict::Name) => 3,
                                    Some(bace_db_postgres::PlayerCreationConflict::Slot) => 5,
                                    None => 6,
                                }
                            }
                            _ => 5,
                        };
                        self.creation = None;
                        Ok(PlayerIoResult::CreationRejected(code))
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
            PlayerIoAction::Load(actor) => self
                .login
                .load_character(self.key, actor)
                .await
                .map(|p| PlayerIoResult::Loaded(Arc::new(p)))
                .map_err(|e| e.to_string()),
            PlayerIoAction::ResolveLoad => self
                .login
                .resolve_loading(self.key)
                .await
                .map(|p| PlayerIoResult::Loaded(Arc::new(p)))
                .map_err(|e| e.to_string()),
            PlayerIoAction::Online => self
                .login
                .world_admitted(self.key, self.binding.expect("validated binding"))
                .await
                .map(PlayerIoResult::Online)
                .map_err(|e| e.to_string()),
            PlayerIoAction::Abort => self
                .login
                .abort_loading(self.key)
                .await
                .map(PlayerIoResult::Aborted)
                .map_err(|e| e.to_string()),
            PlayerIoAction::Logout(lease) => self
                .login
                .logout_drained(self.key, lease)
                .await
                .map(|()| PlayerIoResult::LoggedOut(lease))
                .map_err(|e| e.to_string()),
            PlayerIoAction::Retire => self
                .login
                .retire_roster(self.key)
                .map(|()| PlayerIoResult::Retired)
                .map_err(|e| e.to_string()),
        };
        PlayerIoCompletion {
            token: self.token,
            key: self.key,
            login: self.login,
            creation: self.creation,
            result,
            packet,
        }
    }
}
impl PlayerService {
    pub fn io_busy(&self) -> bool {
        self.io_pending.is_some()
    }
    pub fn maximum_slots(&self) -> u16 {
        self.maximum_slots
    }
    pub fn binding_for(&self, key: SessionKey) -> Option<CharacterBinding> {
        self.sessions.get(&key)?.loaded.as_ref().map(|p| p.binding)
    }
    pub fn begin_io(
        &mut self,
        key: SessionKey,
        action: PlayerIoAction,
    ) -> Result<PlayerIoWork, String> {
        if self.io_pending.is_some() || self.login.is_none() {
            return Err("lifecycle I/O pending".into());
        }
        let session = self.sessions.get(&key).ok_or("unknown session")?;
        let network_slot = matches!(action, PlayerIoAction::Roster | PlayerIoAction::Create);
        if network_slot && self.network.len() >= self.capacity {
            return Err("lifecycle output backpressure".into());
        }
        match action {
            PlayerIoAction::Create if session.creation.is_none() || session.loaded.is_some() => {
                return Err("exact creation missing".into());
            }
            PlayerIoAction::Load(_) | PlayerIoAction::ResolveLoad
                if session.loaded.is_some() || session.creation.is_some() =>
            {
                return Err("player already loading".into());
            }
            PlayerIoAction::Online if !session.accepted || session.online.is_some() => {
                return Err("world not awaiting Online lease".into());
            }
            PlayerIoAction::Abort if session.accepted || session.outstanding.is_some() => {
                return Err("world owner must drain before abort".into());
            }
            _ => {}
        }
        let token = self
            .next
            .checked_add(1)
            .ok_or("player correlation exhausted")?;
        let binding = session.loaded.as_ref().map(|p| p.binding);
        let creation = if matches!(action, PlayerIoAction::Create) {
            self.sessions
                .get_mut(&key)
                .expect("validated")
                .creation
                .take()
        } else {
            None
        };
        self.io_pending = Some((token, key));
        self.io_network_slot = network_slot;
        self.next = token;
        Ok(PlayerIoWork {
            turbine_chat: self.turbine_chat,
            token,
            key,
            login: self.login.take().expect("validated login owner"),
            action,
            binding,
            creation,
        })
    }
    /// Rejecting a mismatched completion returns its original SQL owner intact.
    pub fn accept_io(
        &mut self,
        completion: PlayerIoCompletion,
    ) -> Result<Result<PlayerIoResult, String>, Box<PlayerIoCompletion>> {
        if self.io_pending != Some((completion.token, completion.key)) || self.login.is_some() {
            return Err(Box::new(completion));
        }
        let PlayerIoCompletion {
            key,
            login,
            creation,
            result,
            packet,
            ..
        } = completion;
        self.login = Some(login);
        self.io_pending = None;
        self.io_network_slot = false;
        let session = self
            .sessions
            .get_mut(&key)
            .expect("pending I/O retains session");
        if let Some(creation) = creation {
            session.creation = Some(creation);
        }
        if let Some(packet) = packet {
            self.network.push_back(packet);
        }
        match &result {
            Ok(PlayerIoResult::Loaded(player)) => session.loaded = Some(player.clone()),
            Ok(PlayerIoResult::Online(lease)) => {
                if let Err(error) = self.adopt_online(key, *lease) {
                    return Ok(Err(error));
                }
            }
            Ok(PlayerIoResult::Aborted(_)) => {
                session.loaded = None;
                session.admission = None;
                session.held = false;
            }
            Ok(PlayerIoResult::LoggedOut(_)) => {
                if let Some(loaded) = session.loaded.take() {
                    self.actors.remove(&loaded.binding.actor);
                }
                session.accepted = false;
                session.online = None;
                session.replication = None;
                session.entry = None;
                session.entry_outstanding = false;
                session.entry_error = None;
                session.entered = false;
            }
            Ok(PlayerIoResult::Retired) => {
                self.sessions.remove(&key);
            }
            _ => {}
        }
        Ok(result)
    }
}
impl PlayerService {
    /// Install the durable login instance on the canonical owner before the first
    /// entry projection. This may not reset counters on an entered character.
    pub fn install_login_receipt(
        &mut self,
        key: SessionKey,
        receipt: bace_persistence::OnlineLoginReceipt,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(&key)
            .ok_or("unknown online session")?;
        if session.online != Some(receipt.lease) || session.entry.is_some() || session.entered {
            return Err("login instance receipt is stale".into());
        }
        let replica = session
            .replication
            .as_mut()
            .ok_or("missing online sequence owner")?;
        let instance = crate::player_entry::instance_from_login(receipt, replica.binding)?;
        replica.properties = Sequences::with_instance(65536, instance)
            .map_err(|e| format!("login instance sequence: {e:?}"))?;
        Ok(())
    }
    pub fn queue_prepared_entry(
        &mut self,
        key: SessionKey,
        entry: &crate::player_entry::PreparedPlayerEntry,
        limits: LoginProjectionLimits,
    ) -> Result<(), String> {
        let possessions: Vec<_> = entry
            .possessions
            .iter()
            .map(|p| match p {
                crate::player_entry::PreparedEntryPossession::Create(object) => {
                    bace_replication::LoginPossession::Create(object)
                }
                crate::player_entry::PreparedEntryPossession::Contents {
                    container_id,
                    items,
                } => bace_replication::LoginPossession::Contents {
                    container_id: *container_id,
                    items,
                },
            })
            .collect();
        self.queue_entry(
            key,
            LoginProjection {
                description: &entry.description,
                titles: &entry.titles,
                friends: &entry.friends,
                self_object: &entry.self_object,
                possessions: &possessions,
            },
            limits,
        )
    }
}
