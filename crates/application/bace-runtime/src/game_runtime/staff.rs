//! Authenticated staff ingress retained through cold preparation and owner output.
use super::*;
use crate::{
    gameplay_dispatch::GameplayDispatch,
    staff_map_worker::{StaffMapWork, StaffMapWorker},
};
use bace_gameplay_api::{
    ActionContext,
    staff::{
        MapTeleportRequest, PreparedMapTeleport, StaffCommand, StaffEvent, StaffSpellDefinition,
    },
};
mod account;
mod ban;
mod boot;
mod broadcast;
pub use broadcast::StaffBroadcastRecord;
mod gag;
mod help;
mod ingress;
mod listplayers;
mod native;
mod output;
mod spell;
mod time;
#[derive(Clone)]
pub(super) enum StaffRequest {
    Query(
        bace_gameplay_api::selection::TargetQueryKind,
        bace_types::EntityId,
    ),
    Line(String),
    Map(MapTeleportRequest),
}
pub(super) enum Phase {
    Command(Box<StaffCommand>),
    Native(Box<native::NativePending>),
    Awaiting,
    Account(Box<account::AccountPending>),
    Ban(Box<ban::BanPending>),
    GagLookup(gag::GagLookup),
    Capture(StaffRequest),
    Capturing {
        request: StaffRequest,
        correlation: u64,
    },
    MapPrepare(StaffMapWork),
    MapPreparing(StaffMapWork),
    MapRegion {
        request: MapTeleportRequest,
        prepared: PreparedMapTeleport,
        requested: Option<u64>,
    },
    Other {
        line: String,
        command: bace_admin::AuthorizedCommand,
        principal: bace_auth::StaffPrincipal,
    },
    BootAudit {
        sender: String,
        text: String,
    },
}
pub(super) struct Pending {
    key: SessionKey,
    context: ActionContext,
    token: u64,
    phase: Phase,
}
pub(super) struct StaffRuntime {
    broadcast: Option<broadcast::BroadcastFanout>,
    broadcast_records: VecDeque<StaffBroadcastRecord>,
    broadcast_logging: bool,
    broadcast_sink: Option<Arc<bace_observability::LogStore>>,
    pending: Option<Pending>,
    definitions: Arc<[StaffSpellDefinition]>,
    spell: Option<spell::SpellPending>,
    gag: Option<gag::GagPending>,
    map: Option<StaffMapWorker>,
    native: Option<crate::staff_native_magic::StaffNativeMagicWorker>,
    event: Option<StaffEvent>,
    output: VecDeque<NetworkCommand>,
    failure: Option<String>,
    last_password: BTreeMap<SessionKey, Duration>,
}
impl StaffRuntime {
    pub(super) fn new(definitions: Arc<[StaffSpellDefinition]>) -> Self {
        Self {
            broadcast: None,
            broadcast_records: VecDeque::new(),
            broadcast_logging: false,
            broadcast_sink: None,
            pending: None,
            definitions,
            spell: None,
            gag: None,
            map: None,
            native: None,
            event: None,
            output: VecDeque::new(),
            failure: None,
            last_password: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.broadcast.is_some()
            || !self.broadcast_records.is_empty()
            || self.pending.is_some()
            || self.spell.is_some()
            || self.gag.is_some()
            || self.event.is_some()
            || !self.output.is_empty()
            || self.map.as_ref().is_some_and(StaffMapWorker::has_pending)
            || self
                .native
                .as_ref()
                .is_some_and(crate::staff_native_magic::StaffNativeMagicWorker::has_pending)
    }
}
impl GameRuntime {
    pub fn configure_staff_definitions(
        &mut self,
        definitions: Vec<StaffSpellDefinition>,
    ) -> Result<(), String> {
        if !self.staff.definitions.is_empty()
            || self.staff.has_pending()
            || definitions.len() > 8192
            || definitions
                .iter()
                .any(|d| d.name.len() > 1024 || d.enum_name.len() > 128)
        {
            return Err("staff definition admission state/bounds".into());
        }
        self.staff.definitions = definitions.into();
        Ok(())
    }
    pub fn shutdown_staff_workers(&mut self) -> Result<(), String> {
        if self.staff.has_pending() {
            return Err("staff owner still has pending work".into());
        }
        if let Some(worker) = self.staff.map.take()
            && let Err(worker) = worker.shutdown()
        {
            self.staff.map = Some(*worker);
            return Err("staff map worker not drained".into());
        }
        if let Some(worker) = self.staff.native.take()
            && let Err(worker) = worker.shutdown()
        {
            self.staff.native = Some(*worker);
            return Err("staff native worker not drained".into());
        }
        Ok(())
    }
    pub fn staff_failure(&self) -> Option<&str> {
        self.staff.failure.as_deref()
    }
    pub(super) fn staff_region_outcome(
        &mut self,
        correlation: u64,
        result: Result<(), bace_simulation::GeneratorServiceError>,
    ) -> bool {
        let Some(p) = &mut self.staff.pending else {
            return false;
        };
        let Phase::MapRegion { requested, .. } = &mut p.phase else {
            return false;
        };
        if *requested != Some(correlation) {
            return false;
        }
        if let Err(error) = result {
            *requested = None;
            self.staff.failure = Some(format!("staff region request rejected: {error:?}"));
        }
        true
    }
    /// Only pre-submission cold failures may be abandoned. A capture, SQL write,
    /// or simulation action in flight remains owned until its exact result.
    pub fn reject_staff_unsubmitted(&mut self, key: SessionKey) -> Result<(), String> {
        let p = self.staff.pending.as_ref().ok_or("no staff request")?;
        if p.key != key
            || !matches!(
                p.phase,
                Phase::Capture(_)
                    | Phase::Other { .. }
                    | Phase::MapPrepare(_)
                    | Phase::MapRegion {
                        requested: None,
                        ..
                    }
            )
            || self.staff.spell.is_some()
        {
            return Err("staff request is already submitted".into());
        }
        self.staff.pending = None;
        self.staff.failure = None;
        Ok(())
    }
    /// Logout may abandon only an unsubmitted staff request. Captures, SQL
    /// writes, simulation commands, and their exact receipts retain ownership.
    pub(super) fn cancel_staff_unsubmitted_for_logout(&mut self, key: SessionKey) -> bool {
        if !self
            .sessions
            .get(&key)
            .is_some_and(|session| session.terminated || session.disconnected || self.draining)
        {
            return false;
        }
        self.reject_staff_unsubmitted(key).is_ok()
    }
    pub(super) fn retry_staff_session(&mut self, key: SessionKey) -> bool {
        if self.staff.pending.as_ref().is_some_and(|p| p.key == key) {
            self.staff.failure.take().is_some()
        } else {
            false
        }
    }
    pub fn retry_staff(&mut self) {
        self.staff.failure = None;
    }
    pub(super) fn staff_ingress_blocked(&self, key: SessionKey) -> bool {
        self.staff.pending.as_ref().is_some_and(|p| p.key == key)
    }
    pub(super) fn forget_staff_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.staff_ingress_blocked(key) {
            return Err("staff action still owns session".into());
        }
        self.staff.last_password.remove(&key);
        Ok(())
    }
    pub(super) fn staff_owns_capture(
        &self,
        outcome: &bace_simulation::PlayerSnapshotOutcome,
    ) -> bool {
        self.staff.gag.as_ref().is_some_and(|p|p.owns_capture(outcome)) || self.staff.spell.as_ref().is_some_and(|p|p.owns_capture(outcome)) || self.staff.pending.as_ref().is_some_and(|p|matches!(p.phase,Phase::Capturing{correlation,..} if correlation==outcome.correlation))
    }
    pub(super) fn accept_staff_capture(
        &mut self,
        outcome: bace_simulation::PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), String> {
        if self
            .staff
            .gag
            .as_ref()
            .is_some_and(|p| p.owns_capture(&outcome))
        {
            return self.accept_staff_gag_capture(outcome, unix);
        }
        if self
            .staff
            .spell
            .as_ref()
            .is_some_and(|p| p.owns_capture(&outcome))
        {
            return self.accept_staff_spell_capture(outcome, unix);
        }
        if !self.staff_owns_capture(&outcome) {
            return Err("staff capture correlation".into());
        }
        let mut p = self.staff.pending.take().expect("matched staff capture");
        let Phase::Capturing { request, .. } = &p.phase else {
            unreachable!()
        };
        let request = request.clone();
        let prepared = match outcome.result {
            Ok(snapshot)
                if snapshot.binding().actor == p.context.actor
                    && snapshot.binding().account == p.context.account
                    && snapshot.binding().session == p.context.session
                    && snapshot.operation().is_none() =>
            {
                self.prepare_staff_captured(&p, &request, &snapshot, unix)
            }
            Ok(_) => Err("staff capture binding mismatch".into()),
            Err(error) => Err(format!("staff capture: {error:?}")),
        };
        match prepared {
            Ok(phase) => p.phase = phase,
            Err(error) => {
                p.phase = Phase::Capture(request);
                self.staff.failure = Some(error);
            }
        }
        self.staff.pending = Some(p);
        Ok(())
    }
    pub(super) fn poll_staff(&mut self) -> Result<(), String> {
        self.poll_staff_output()?;
        self.poll_staff_boot_audit()?;
        if let Some(error) = &self.staff.failure {
            return Err(error.clone());
        }
        self.poll_staff_native()?;
        self.poll_staff_account()?;
        self.poll_staff_ban()?;
        self.poll_staff_gag_lookup()?;
        if let Err(error) = self.poll_staff_gag() {
            self.staff.failure = Some(error.clone());
            return Err(error);
        }
        if let Some(error) = &self.staff.failure {
            return Err(error.clone());
        }
        if let Err(error) = self.poll_staff_spell() {
            self.staff.failure = Some(error.clone());
            return Err(error);
        }
        if let Some(map) = &mut self.staff.map
            && let Some(result) = map.poll()?
        {
            let p = self
                .staff
                .pending
                .as_mut()
                .ok_or("staff map result without owner")?;
            if !matches!(p.phase,Phase::MapPreparing(work) if work==result.work) {
                return Err("staff map result identity".into());
            }
            match result.result {
                Ok(prepared) => {
                    p.phase = Phase::MapRegion {
                        request: result.work.request,
                        prepared,
                        requested: None,
                    }
                }
                Err(error) => {
                    p.phase = Phase::MapPrepare(result.work);
                    self.staff.failure = Some(error.clone());
                    return Err(error);
                }
            }
        }
        if let Some(p) = self.staff.pending.take() {
            if let Phase::Other {
                command, principal, ..
            } = &p.phase
            {
                match self
                    .apply_staff_help(p.key, p.context, command, *principal)
                    .and_then(|done| {
                        if done {
                            Ok(true)
                        } else {
                            self.apply_staff_boot(p.key, p.context, command, *principal, p.token)
                                .and_then(|done| {
                                    if done {
                                        Ok(true)
                                    } else {
                                        self.apply_staff_listplayers(
                                            p.key, p.context, command, *principal,
                                        )
                                        .and_then(|done| {
                                            if done {
                                                Ok(true)
                                            } else {
                                                self.apply_staff_time(p.key, p.context, command)
                                                    .and_then(|done| {
                                                        if done {
                                                            Ok(true)
                                                        } else {
                                                            self.apply_shard_command(
                                                                p.key, p.context, command,
                                                                *principal,
                                                            )
                                                        }
                                                    })
                                            }
                                        })
                                    }
                                })
                        }
                    }) {
                    Ok(true) => {}
                    Ok(false) => self.staff.pending = Some(p),
                    Err(error) => {
                        self.staff.pending = Some(p);
                        self.staff.failure = Some(error.clone());
                        return Err(error);
                    }
                }
            } else {
                self.staff.pending = Some(p);
            }
        }
        let token = self.token()?;
        let Some(p) = &mut self.staff.pending else {
            return Ok(());
        };
        match &mut p.phase {
            Phase::Command(command) => match self
                .simulation
                .input()
                .try_submit(bace_simulation::Command::Staff(*command.clone()))
            {
                Ok(()) => p.phase = Phase::Awaiting,
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("staff owner ingress closed".into());
                }
            },
            Phase::Capture(request) => {
                let command = bace_simulation::Command::PlayerSnapshot(
                    bace_simulation::PlayerSnapshotRequest {
                        correlation: token,
                        binding: bace_gameplay_api::CharacterBinding {
                            actor: p.context.actor,
                            account: p.context.account,
                            session: p.context.session,
                        },
                        operation: None,
                    },
                );
                if self.simulation.input().try_submit(command).is_ok() {
                    p.phase = Phase::Capturing {
                        request: request.clone(),
                        correlation: token,
                    };
                }
            }
            Phase::MapPrepare(work) => {
                if self.staff.map.is_none() {
                    self.staff.map = Some(StaffMapWorker::start(self.bootstrap.assets.clone())?);
                }
                if self
                    .staff
                    .map
                    .as_mut()
                    .expect("map worker")
                    .submit(*work)
                    .is_ok()
                {
                    p.phase = Phase::MapPreparing(*work);
                }
            }
            Phase::MapRegion {
                request,
                prepared,
                requested,
            } => {
                let block = (prepared.destination.cell >> 16) as u16;
                if self
                    .world
                    .as_ref()
                    .is_some_and(|w| w.regions.prepared_region(block).is_some())
                {
                    p.phase = Phase::Command(Box::new(StaffCommand {
                        token: p.token,
                        action: bace_gameplay_api::staff::StaffAction::MapTeleport {
                            context: p.context,
                            request: *request,
                            prepared: *prepared,
                        },
                    }));
                } else if requested.is_none()
                    && self
                        .simulation
                        .input()
                        .try_submit(bace_simulation::Command::Generator(
                            bace_simulation::GeneratorCommand {
                                correlation: token,
                                action: bace_simulation::GeneratorAction::RequestRegion {
                                    landblock: block,
                                    permanent: false,
                                },
                            },
                        ))
                        .is_ok()
                {
                    self.request_regions.insert(token, (p.key, block));
                    *requested = Some(token);
                }
            }
            Phase::Other {
                command,
                line,
                principal,
            } => {
                let _retained_authorization =
                    (command.spec.name, line.len(), principal.account_access);
                return Err("authorized staff request retained for its subsystem owner".into());
            }
            Phase::Native(_)
            | Phase::BootAudit { .. }
            | Phase::Account(_)
            | Phase::Ban(_)
            | Phase::GagLookup(_)
            | Phase::Awaiting
            | Phase::Capturing { .. }
            | Phase::MapPreparing(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
