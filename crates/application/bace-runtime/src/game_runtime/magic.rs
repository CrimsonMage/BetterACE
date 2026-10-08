//! Ordinary authenticated spell ingress. Cold account formula/motion preparation
//! must be acknowledged by the actor owner before the original cast is submitted.
mod expiry;
mod identity_pools;
mod output;
mod portals;
mod projectiles;
pub use output::MagicObserverWork;
mod resources;
use super::*;
use crate::staff_native_magic::{StaffNativeMagicWork, StaffNativeMagicWorker};
use bace_gameplay_api::{
    ActionContext, CastChange, CastOutcome, CastRejection, CastRequest, CharacterBinding,
};
use bace_simulation::{Command, PlayerSnapshotOutcome, PlayerSnapshotRequest};
use std::sync::mpsc::TrySendError;
struct Pending {
    context: ActionContext,
    request: CastRequest,
    token: u64,
    phase: Phase,
}
enum Phase {
    Capture,
    Capturing,
    Prepare(Arc<StaffNativeMagicWork>),
    Preparing(Arc<StaffNativeMagicWork>),
    Program(Option<Box<bace_simulation::PreparedActorMagicProgram>>),
    AwaitingProgram,
    Cast,
    AwaitingCast,
    Active(u64),
    Terminal(CastOutcome),
}
pub(super) enum MagicIngress {
    Accepted,
    Blocked,
    Unsupported,
}
pub(super) struct MagicRuntime {
    pending: BTreeMap<SessionKey, Pending>,
    cancellations: BTreeMap<SessionKey, ActionContext>,
    native: Option<StaffNativeMagicWorker>,
    unexpected: Option<CastOutcome>,
    event: Option<bace_simulation::MagicEvent>,
    resources: resources::Resources,
    identity_pools: identity_pools::IdentityPools,
    failures: BTreeMap<SessionKey, String>,
    observers: VecDeque<MagicObserverWork>,
    next_observer: u64,
    observer_bytes: usize,
    portals: VecDeque<bace_simulation::MagicEvent>,
    regions: BTreeMap<u64, SessionKey>,
    projectiles: BTreeMap<bace_types::EntityId, projectiles::ProjectilePresentation>,
}
impl MagicRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            cancellations: BTreeMap::new(),
            native: None,
            unexpected: None,
            event: None,
            resources: resources::Resources::new(),
            identity_pools: identity_pools::IdentityPools::new(),
            failures: BTreeMap::new(),
            observers: VecDeque::new(),
            next_observer: 0,
            observer_bytes: 0,
            portals: VecDeque::new(),
            regions: BTreeMap::new(),
            projectiles: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
            || !self.cancellations.is_empty()
            || self.native.as_ref().is_some_and(|w| w.has_pending())
            || self.unexpected.is_some()
            || self.event.is_some()
            || self.resources.has_pending()
            || self.identity_pools.pending()
            || !self.observers.is_empty()
            || !self.portals.is_empty()
            || !self.regions.is_empty()
            || !self.projectiles.is_empty()
    }
}
impl GameRuntime {
    pub(super) fn magic_ingress_blocked(&self, key: SessionKey) -> bool {
        self.magic.pending.contains_key(&key) || self.magic.resources.owns_session(key)
    }
    pub(super) fn forget_magic_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.magic_ingress_blocked(key) {
            return Err("spell still owns session".into());
        }
        self.magic.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_magic_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<MagicIngress, String> {
        if message.bytes.get(..4)
            != Some(
                &bace_wire::opcode::GameMessageOpcode::GameAction
                    .0
                    .to_le_bytes(),
            )
        {
            return Ok(MagicIngress::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(MagicIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(MagicIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let request = match bace_session::decode_magic(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value.request,
            Err(bace_session::DispatchError::UnsupportedAction(_)) => {
                return Ok(MagicIngress::Unsupported);
            }
            Err(_) => {
                // Unaccepted malformed ingress cannot poison durable drain.
                // Existing accepted casts/resources remain owned and finish.
                self.sessions
                    .get_mut(&key)
                    .ok_or("magic session disappeared")?
                    .terminated = true;
                return Ok(MagicIngress::Accepted);
            }
        };
        if self.magic.pending.contains_key(&key)
            || self
                .magic
                .pending
                .values()
                .filter(|p| !matches!(p.phase, Phase::Active(_) | Phase::Terminal(_)))
                .count()
                >= self.limits.loading
            || !self.players.entered(binding.actor)
            || session.terminated
            || session.disconnected
        {
            return Ok(MagicIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("magic authenticated binding mismatch".into());
        }
        let token = self.token()?;
        self.magic.pending.insert(
            key,
            Pending {
                context,
                request,
                token,
                phase: Phase::Capture,
            },
        );
        Ok(MagicIngress::Accepted)
    }
    pub(super) fn magic_owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.magic
            .pending
            .values()
            .any(|p| p.token == outcome.correlation && matches!(p.phase, Phase::Capturing))
    }
    pub(super) fn accept_magic_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), String> {
        let key = self
            .magic
            .pending
            .iter()
            .find(|(_, p)| p.token == outcome.correlation && matches!(p.phase, Phase::Capturing))
            .map(|(k, _)| *k)
            .ok_or("magic snapshot correlation")?;
        let snapshot = match outcome.result {
            Ok(value) => value,
            Err(bace_simulation::CharacterRegistrationError::DurabilityPending) => {
                self.magic.pending.get_mut(&key).expect("matched").phase = Phase::Capture;
                return Ok(());
            }
            Err(error) => {
                return self.fail_magic_preparation(key, format!("magic snapshot: {error:?}"));
            }
        };
        let p = &self.magic.pending[&key];
        if snapshot.binding() != binding(p.context) {
            return Err("magic snapshot actor mismatch".into());
        }
        let spell = match p.request {
            CastRequest::Targeted { spell, .. } | CastRequest::Untargeted { spell } => spell,
            CastRequest::Cancel => {
                self.magic.pending.get_mut(&key).expect("matched").phase = Phase::Cast;
                return Ok(());
            }
        };
        let Some(row) = self.assets.spell_rows.get(&spell) else {
            return self.fail_magic_preparation(key, "native spell unavailable".into());
        };
        let (baseline, _, _) = self
            .online_saves
            .baseline(p.context.actor.0)
            .ok_or("magic source baseline unavailable")?;
        let saved = crate::player_saves::freeze_player_snapshot(baseline, &snapshot, unix)
            .map_err(|e| e.to_string())?;
        let session = &self.sessions[&key];
        let mut w = bace_wire::Writer::new();
        w.string16(session.account.name.as_str())
            .map_err(|e| e.to_string())?;
        let bytes = w.into_bytes();
        let len = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
        let work=Arc::new(StaffNativeMagicWork{token:p.token,binding:snapshot.binding(),expected_character_revision:snapshot.character().progression().revision(),source:Arc::new(saved.player.entity.state.clone()),account_cp1252:bytes[2..2+len].to_vec(),top_level_templates:snapshot.items().iter().filter(|i|matches!(i.place,bace_inventory::ItemPlace::Contained{container,..} if container==p.context.actor)).map(|i|i.template).collect(),row:Arc::new(row.clone())});
        self.magic.pending.get_mut(&key).expect("matched").phase = Phase::Prepare(work);
        Ok(())
    }
    fn fail_magic_preparation(&mut self, key: SessionKey, error: String) -> Result<(), String> {
        let p = self
            .magic
            .pending
            .get_mut(&key)
            .ok_or("magic pending missing")?;
        p.phase = Phase::Terminal(CastOutcome {
            context: p.context,
            result: Err(CastRejection::MissingAssets),
        });
        self.magic.failures.insert(key, error);
        Ok(())
    }
    pub(super) fn accept_player_magic_program_outcome(
        &mut self,
        event: &bace_gameplay_api::staff::StaffEvent,
    ) -> Result<bool, String> {
        let bace_gameplay_api::staff::StaffEvent::Outcome {
            token,
            actor,
            result,
        } = event
        else {
            return Ok(false);
        };
        let Some((_, p)) = self
            .magic
            .pending
            .iter_mut()
            .find(|(_, p)| p.token == *token && matches!(p.phase, Phase::AwaitingProgram))
        else {
            return Ok(false);
        };
        if *actor != Some(p.context.actor) {
            return Err("player magic program actor fence".into());
        }
        p.phase = if result.is_ok() {
            Phase::Cast
        } else if matches!(
            result,
            Err(bace_gameplay_api::staff::StaffError::Stale
                | bace_gameplay_api::staff::StaffError::Busy
                | bace_gameplay_api::staff::StaffError::Capacity)
        ) {
            Phase::Capture
        } else {
            Phase::Terminal(CastOutcome {
                context: p.context,
                result: Err(CastRejection::MissingAssets),
            })
        };
        Ok(true)
    }
    pub(super) fn poll_magic(&mut self, unix: u64) -> Result<(), String> {
        self.poll_magic_identity_pools(unix)?;
        self.poll_magic_resources(unix)?;
        if self.magic.unexpected.is_some() {
            return Err("unrelated client cast outcome retained".into());
        }
        if let Some(native) = &mut self.magic.native
            && let Some(done) = native.poll()?
        {
            let key=self.magic.pending.iter().find(|(_,p)|matches!(&p.phase,Phase::Preparing(work) if Arc::ptr_eq(work,&done.work))).map(|(k,_)|*k).ok_or("player magic cold correlation")?;
            match done.result {
                Ok(program) => {
                    self.magic.pending.get_mut(&key).expect("matched").phase =
                        Phase::Program(Some(Box::new(program)))
                }
                Err(error) => self.fail_magic_preparation(key, error)?,
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.cast_outcomes().try_recv() else {
                break;
            };
            if let Some(key) = self
                .magic
                .cancellations
                .iter()
                .find_map(|(key, c)| (*c == outcome.context).then_some(*key))
            {
                self.magic.cancellations.remove(&key);
                if let Ok(CastChange::Cancelled { .. }) = outcome.result {
                    let p = self
                        .magic
                        .pending
                        .get_mut(&key)
                        .ok_or("cancelled cast owner missing")?;
                    p.context = outcome.context;
                    p.phase = Phase::Terminal(outcome);
                } else if let Err(error) = outcome.result {
                    self.magic
                        .failures
                        .insert(key, format!("cast cancellation: {error:?}"));
                }
                continue;
            }
            let Some((_, p)) = self.magic.pending.iter_mut().find(|(_, p)| {
                p.context == outcome.context
                    && matches!(p.phase, Phase::AwaitingCast | Phase::Active(_))
            }) else {
                self.magic.unexpected = Some(outcome);
                return Err("cast outcome has no retained request".into());
            };
            if matches!((&p.phase,&outcome.result),(Phase::Active(expected),Ok(CastChange::Completed{cast}|CastChange::Cancelled{cast})) if expected!=cast)
            {
                self.magic.unexpected = Some(outcome);
                return Err("terminal cast identity mismatch".into());
            }
            p.phase = match outcome.result {
                Ok(CastChange::Started { cast }) => Phase::Active(cast),
                _ => Phase::Terminal(outcome),
            };
        }
        let keys: Vec<_> = self
            .magic
            .pending
            .iter()
            .filter(|(_, p)| {
                !matches!(
                    p.phase,
                    Phase::Active(_)
                        | Phase::AwaitingCast
                        | Phase::Capturing
                        | Phase::Preparing(_)
                        | Phase::AwaitingProgram
                        | Phase::Terminal(_)
                )
            })
            .take(self.limits.work_per_poll)
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            self.advance_magic_ingress(key)?;
        }
        self.poll_magic_output(unix)?;
        self.poll_magic_observers()
    }
    fn advance_magic_ingress(&mut self, key: SessionKey) -> Result<(), String> {
        if !self.prepare_magic_regions(key)? {
            return Ok(());
        }
        let p = self
            .magic
            .pending
            .get_mut(&key)
            .ok_or("magic request owner missing")?;
        match &mut p.phase {
            Phase::Capture => {
                let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                    correlation: p.token,
                    binding: binding(p.context),
                    operation: None,
                });
                if self.simulation.input().try_submit(command).is_ok() {
                    p.phase = Phase::Capturing;
                }
            }
            Phase::Prepare(work) => {
                if self.magic.native.is_none() {
                    self.magic.native = Some(StaffNativeMagicWorker::start(
                        self.bootstrap.assets.clone(),
                        self.bootstrap.pack.generation.clone(),
                    )?);
                }
                if self
                    .magic
                    .native
                    .as_mut()
                    .expect("worker")
                    .submit_generation(work.clone(), self.bootstrap.pack.generation.clone())
                    .is_ok()
                {
                    p.phase = Phase::Preparing(work.clone());
                }
            }
            Phase::Program(program) => {
                let command = Command::ActorMagicProgram {
                    correlation: p.token,
                    program: program.take().ok_or("native magic program missing")?,
                };
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::AwaitingProgram,
                    Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) => {
                        let Command::ActorMagicProgram {
                            program: retained, ..
                        } = command
                        else {
                            unreachable!()
                        };
                        *program = Some(retained);
                    }
                }
            }
            Phase::Cast
                if self
                    .simulation
                    .input()
                    .try_submit(Command::Cast {
                        context: p.context,
                        request: p.request,
                    })
                    .is_ok() =>
            {
                p.phase = Phase::AwaitingCast;
            }
            _ => {}
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

impl GameRuntime {
    /// Called before the general active-action gate. Only an actually submitted
    /// cast takes ownership; other cancellation remains with combat/movement.
    pub(super) fn handle_player_magic_cancel(
        &mut self,
        context: ActionContext,
    ) -> Result<MagicIngress, String> {
        let Some((&key, _)) = self.magic.pending.iter().find(|(_, p)| {
            p.context.actor == context.actor
                && p.context.account == context.account
                && p.context.session == context.session
                && matches!(p.phase, Phase::Active(_) | Phase::AwaitingCast)
        }) else {
            return Ok(MagicIngress::Unsupported);
        };
        if self.magic.cancellations.contains_key(&key) {
            return Ok(MagicIngress::Blocked);
        }
        match self.simulation.input().try_submit(Command::Cast {
            context,
            request: CastRequest::Cancel,
        }) {
            Ok(()) => {
                self.magic.cancellations.insert(key, context);
                Ok(MagicIngress::Accepted)
            }
            Err(TrySendError::Full(_)) => Ok(MagicIngress::Blocked),
            Err(TrySendError::Disconnected(_)) => {
                Err("cast cancellation owner ingress closed".into())
            }
        }
    }
}

impl GameRuntime {
    pub fn shutdown_magic_workers(&mut self) -> Result<Vec<std::thread::JoinHandle<()>>, String> {
        if self.magic.has_pending() {
            return Err("magic controller still requires drain".into());
        }
        let mut tasks = Vec::with_capacity(1);
        if let Some(worker) = &mut self.magic.native
            && let Some(task) = worker.try_shutdown()?
        {
            tasks.push(task);
        }
        self.magic.native = None;
        Ok(tasks)
    }
}
