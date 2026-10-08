//! Authenticated PetDevice Use keeps a fresh owner snapshot, allocated identity,
//! authored DAT closure and durable reservation under one bounded runtime owner.
use super::*;
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    Command, PetAction, PetCommand, PetDecision, PetError, PlayerReadSnapshot,
    PlayerSnapshotOutcome, PlayerSnapshotRequest,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use std::sync::mpsc::TrySendError;
use std::{
    sync::Arc,
    task::{Context, Poll, Waker},
};

pub(super) struct PendingPetUse {
    pub(super) key: SessionKey,
    pub(super) context: ActionContext,
    pub(super) device: EntityId,
    pub(super) device_name: String,
    pub(super) phase: Phase,
    pub(super) prepared: Option<preparation::PreparedPetSource>,
    pub(super) pet: Option<EntityId>,
    pub(super) identity: PetOperationId,
    pub(super) item_output: bool,
}

type ColdPet = Result<
    (
        EntityId,
        preparation::PreparedPetSource,
        u64,
        bace_simulation::PetUser,
        bace_inventory::ActivationRequirements,
    ),
    String,
>;

pub(super) enum Phase {
    CaptureReady,
    Capturing(u64),
    Captured(Arc<PlayerReadSnapshot>),
    Cold(Job<ColdPet>),
    Ready {
        correlation: u64,
        command: Option<Box<Command>>,
    },
    Submitted(u64),
    Proposed(Option<Box<PetWork>>),
    WaitingStow(u64),
    StowRetired(u64),
    Saving,
    Published,
    Rejected(PetError),
    Blocked(String),
}

impl PetRuntime {
    pub(in crate::game_runtime) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.use_pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capturing(c) if c == outcome.correlation))
    }

    pub(in crate::game_runtime) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if !self.owns_capture(&outcome) {
            return Err(outcome);
        }
        let pending = self.use_pending.as_mut().expect("matched pet capture");
        match &outcome.result {
            Ok(snapshot) if snapshot.binding() == binding(pending.context) => {
                pending.phase = Phase::Captured(snapshot.clone());
                Ok(())
            }
            Err(_) => {
                pending.phase = Phase::CaptureReady;
                Ok(())
            }
            _ => Err(outcome),
        }
    }

    pub(super) fn accept_use_outcome(
        &mut self,
        outcome: PetOutcome,
    ) -> Result<(), Box<PetOutcome>> {
        let Some(pending) = self.use_pending.as_mut() else {
            return Err(Box::new(outcome));
        };
        let Phase::Submitted(correlation) = pending.phase else {
            return Err(Box::new(outcome));
        };
        if correlation != outcome.correlation {
            return Err(Box::new(outcome));
        }
        match outcome.result {
            Ok(PetDecision::Proposed {
                ticket,
                actor_revision,
                registry_revision,
                registry_after,
            }) if ticket.actor == pending.context.actor => {
                let cooldown = pending
                    .prepared
                    .as_ref()
                    .is_some_and(|source| source.profile.cooldown_group().is_some());
                let work = PetWork {
                    binding: binding(pending.context),
                    ticket,
                    actor_revision,
                    registry_after: cooldown.then_some((registry_revision, registry_after)),
                    summon: true,
                    identity: pending.identity,
                };
                pending.phase = match self.service.stage(work) {
                    Ok(()) => Phase::Saving,
                    Err(work) => Phase::Proposed(Some(work)),
                };
                Ok(())
            }
            Err(error) => {
                pending.phase = Phase::Rejected(error);
                Ok(())
            }
            Ok(PetDecision::Stowing {
                operation: Some(operation),
            }) if operation != 0 => {
                pending.pet = None;
                pending.prepared = None;
                pending.phase = Phase::WaitingStow(operation);
                Ok(())
            }
            result => Err(Box::new(PetOutcome {
                correlation,
                result,
            })),
        }
    }
}

impl GameRuntime {
    pub(in crate::game_runtime) fn handle_pet_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(loaded) = self.sessions.get(&key).and_then(|s| s.loading.as_ref()) else {
            return Ok(I::Unsupported);
        };
        let binding = loaded.loaded.binding;
        let decoded = match bace_session::decode_skill_device(
            bace_session::SessionState::WorldConnected,
            ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: message.sequence,
            },
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(decoded) => decoded,
            Err(
                bace_session::DispatchError::UnsupportedAction(_)
                | bace_session::DispatchError::InvalidTarget(_),
            ) => return Ok(I::Unsupported),
            Err(error) => return Err(format!("pet Use input: {error:?}")),
        };
        let bace_session::SkillDeviceAction::Use { item } = decoded.request else {
            return Ok(I::Unsupported);
        };
        let device = EntityId(item);
        let Some(saved) = self.online_saves.inventory_baseline(binding.actor.0, item) else {
            return Ok(I::Unsupported);
        };
        if saved.entity.state.weenie_type != 70 {
            return Ok(I::Unsupported);
        }
        if self.pets.has_pending() {
            return Ok(I::Blocked);
        }
        let mut bytes = [0; 16];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|error| error.to_string())?;
        self.pets.use_pending = Some(PendingPetUse {
            key,
            context: decoded.context,
            device,
            device_name: String::new(),
            phase: Phase::CaptureReady,
            prepared: None,
            pet: None,
            identity: PetOperationId::new(bytes)?,
            item_output: false,
        });
        Ok(I::Accepted)
    }

    pub(in crate::game_runtime) fn pet_ingress_blocked(&self, key: SessionKey) -> bool {
        self.pets.use_pending.as_ref().is_some_and(|p| p.key == key)
            || self.pets.service.requires_drain()
                && self
                    .sessions
                    .get(&key)
                    .and_then(|s| s.loading.as_ref())
                    .is_some_and(|l| self.pets.service.actor() == Some(l.loaded.binding.actor))
    }

    pub(in crate::game_runtime) fn retry_pet_session(&mut self, key: SessionKey) -> bool {
        if !self.pet_ingress_blocked(key) {
            return false;
        }
        self.pets.service.retry();
        if let Some(pending) = self.pets.use_pending.as_mut().filter(|p| p.key == key)
            && matches!(pending.phase, Phase::Blocked(_))
        {
            pending.phase = Phase::CaptureReady;
        }
        true
    }

    pub(in crate::game_runtime) fn forget_pet_session(
        &mut self,
        key: SessionKey,
    ) -> Result<(), String> {
        if self.pet_ingress_blocked(key) {
            Err("pet Use or release remains pending".into())
        } else {
            Ok(())
        }
    }

    pub(super) fn poll_pet_use(&mut self) -> Result<(), String> {
        let Some(mut pending) = self.pets.use_pending.take() else {
            return Ok(());
        };
        let disconnected = self
            .sessions
            .get(&pending.key)
            .is_none_or(|session| session.disconnected);
        if disconnected
            && matches!(
                pending.phase,
                Phase::CaptureReady
                    | Phase::Captured(_)
                    | Phase::Cold(_)
                    | Phase::Ready { .. }
                    | Phase::Blocked(_)
            )
        {
            return Ok(());
        }
        let result = self.advance_pet_use(&mut pending);
        if let Err(error) = &result
            && matches!(pending.phase, Phase::Captured(_) | Phase::Cold(_))
        {
            pending.phase = Phase::Blocked(error.clone());
        }
        self.pets.use_pending = Some(pending);
        result
    }

    fn advance_pet_use(&mut self, pending: &mut PendingPetUse) -> Result<(), String> {
        match &mut pending.phase {
            Phase::CaptureReady => {
                let correlation = self.token()?;
                let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                    correlation,
                    binding: binding(pending.context),
                    operation: None,
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => pending.phase = Phase::Capturing(correlation),
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("pet snapshot ingress closed".into());
                    }
                }
            }
            Phase::Captured(snapshot) => {
                let baselines = self.online_saves.captured_inventory_baselines(snapshot)?;
                let row = baselines
                    .iter()
                    .find(|row| row.entity.object_id == pending.device.0)
                    .ok_or("pet device missing from captured inventory")?;
                if row.entity.state.weenie_type != 70
                    || snapshot
                        .items()
                        .iter()
                        .find(|item| item.id == pending.device)
                        .is_none_or(|item| item.revision != row.entity.mutation_revision)
                {
                    return Err("pet device captured identity/revision mismatch".into());
                }
                let source = row.entity.state.clone();
                pending.device_name = source
                    .properties
                    .strings
                    .iter()
                    .find(|property| property.id == 1)
                    .map_or_else(
                        || source.class_name.clone(),
                        |property| property.value.clone(),
                    );
                let activation =
                    crate::player_assets::prepare_item_activation_requirements(&source)?;
                let skill = snapshot
                    .skill_values()
                    .iter()
                    .find(|s| s.skill == 54)
                    .ok_or("summoning skill projection missing")?;
                let level = snapshot
                    .character()
                    .native_services()
                    .ok_or("pet owner native level missing")?
                    .level;
                let (player, _, _) = self
                    .online_saves
                    .baseline(pending.context.actor.0)
                    .ok_or("pet owner source missing")?;
                let mastery = player
                    .player
                    .entity
                    .state
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == 362)
                    .map_or(0, |p| p.value);
                let user = bace_simulation::PetUser {
                    portal_space: false,
                    owns_device: true,
                    advancement: skill.advancement as u32,
                    skill: skill.current,
                    level,
                    mastery: u32::try_from(mastery).map_err(|_| "negative pet mastery")?,
                    charges: row
                        .entity
                        .state
                        .properties
                        .ints
                        .iter()
                        .find(|p| p.id == 92)
                        .and_then(|p| u32::try_from(p.value).ok())
                        .unwrap_or(0),
                    active_combat_pet: false,
                    cooldown_active: false,
                };
                let store = self.bootstrap.store.clone();
                let generation = self.bootstrap.pack.generation.clone();
                let manifest = self.bootstrap.assets.clone();
                let policy = self.assets.creatures;
                let revision = row.entity.mutation_revision;
                pending.phase = Phase::Cold(Box::pin(async move {
                    let ids = store
                        .allocate_dynamic_ids(1)
                        .await
                        .map_err(|e| e.to_string())?;
                    let pet = EntityId(*ids.first().ok_or("pet identity allocation empty")?);
                    if ids.len() != 1 || pet.0 < 0x8000_0000 {
                        return Err("pet dynamic identity allocation".into());
                    }
                    let prepared = tokio::task::spawn_blocking(move || {
                        preparation::prepare(generation, manifest, policy, source, pet)
                    })
                    .await
                    .map_err(|e| e.to_string())??;
                    Ok((pet, prepared, revision, user, activation))
                }));
            }
            Phase::Cold(job) => {
                if let Poll::Ready(result) =
                    job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
                {
                    let (pet, prepared, device_revision, user, activation) = result?;
                    let correlation = self.token()?;
                    let action = match prepared.profile.clone() {
                        preparation::PreparedPetProfile::Combat(profile) => PetAction::Summon {
                            context: pending.context,
                            device: pending.device,
                            device_revision,
                            pet,
                            user,
                            activation: Some(activation),
                            profile,
                        },
                        preparation::PreparedPetProfile::Passive(profile) => {
                            PetAction::SummonPassive {
                                context: pending.context,
                                device: pending.device,
                                device_revision,
                                pet,
                                user,
                                activation: Some(activation),
                                profile,
                            }
                        }
                    };
                    let command = Command::Pet(PetCommand {
                        correlation,
                        action,
                    });
                    pending.pet = Some(pet);
                    pending.prepared = Some(prepared);
                    pending.phase = Phase::Ready {
                        correlation,
                        command: Some(Box::new(command)),
                    };
                }
            }
            Phase::Ready {
                correlation,
                command,
            } => {
                if let Some(value) = command.take() {
                    match self.simulation.input().try_submit(*value) {
                        Ok(()) => pending.phase = Phase::Submitted(*correlation),
                        Err(TrySendError::Full(value)) => *command = Some(Box::new(value)),
                        Err(TrySendError::Disconnected(value)) => {
                            *command = Some(Box::new(value));
                            return Err("pet summon ingress closed".into());
                        }
                    }
                }
            }
            Phase::Proposed(work) => {
                if let Some(value) = work.take() {
                    match self.pets.service.stage(*value) {
                        Ok(()) => pending.phase = Phase::Saving,
                        Err(value) => *work = Some(value),
                    }
                }
            }
            Phase::Capturing(_)
            | Phase::Submitted(_)
            | Phase::WaitingStow(_)
            | Phase::StowRetired(_)
            | Phase::Saving
            | Phase::Published
            | Phase::Rejected(_)
            | Phase::Blocked(_) => {}
        }
        Ok(())
    }
}

fn binding(context: ActionContext) -> CharacterBinding {
    CharacterBinding {
        actor: context.actor,
        account: context.account,
        session: context.session,
    }
}
