//! Bounded live tinkering route. SQL and source preparation remain outside the
//! simulation; only accepted snapshots can become an owner proposal.
mod catalog;
mod inputs;
mod output;
use super::*;
use crate::crafting_service::{CraftingCompletion, CraftingService, CraftingWork};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    Command, CraftingCommand, CraftingCommandKind, CraftingOutcome, CraftingResult, CraftingTicket,
    PlayerSnapshotOutcome, PlayerSnapshotRequest, TinkerCommandInput,
};
use bace_transport::ReceivedMessage;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;
#[derive(Clone)]
struct Quote {
    token: u32,
    expires_at: Duration,
    input: TinkerCommandInput,
    native: Arc<bace_crafting::NativeRecipe>,
    assets: Arc<ColdAssets>,
}
enum Phase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Preparing,
    Ready(Box<CraftingCommand>),
    Submitted,
    Saving,
    Quoted(bace_crafting::TinkerChance),
    Failed(String),
    Done,
}
struct Pending {
    key: SessionKey,
    context: ActionContext,
    binding: CharacterBinding,
    source: u32,
    target: u32,
    operation: [u8; 16],
    confirm: bool,
    motion: Option<(u32, u32)>,
    phase: Phase,
    submitted: Option<u64>,
    pending_operation: Option<u64>,
    rejection: Option<bace_crafting::CraftError>,
    input: Option<TinkerCommandInput>,
    native: Option<Arc<bace_crafting::NativeRecipe>>,
    ticket: Option<CraftingTicket>,
    assets: Option<Arc<ColdAssets>>,
    completion: Option<CraftingCompletion>,
}
struct ColdAssets {
    appearance: crate::player_entry::PreparedEntryAppearanceAssets,
    materials: BTreeMap<u32, String>,
    clap: Arc<bace_motion::PreparedMotionChain>,
}
struct Prepared {
    catalog: Arc<catalog::Catalog>,
    native: Arc<bace_crafting::NativeRecipe>,
    input: TinkerCommandInput,
    assets: Arc<ColdAssets>,
}
pub(super) struct CraftingRuntime {
    pub(super) service: CraftingService,
    pending: Option<Pending>,
    quotes: BTreeMap<SessionKey, Quote>,
    catalog: Option<Arc<catalog::Catalog>>,
    cold: Option<Job<Result<Prepared, String>>>,
    unexpected: Option<CraftingOutcome>,
    unexpected_ticket: Option<CraftingTicket>,
    failures: BTreeMap<SessionKey, String>,
}
impl CraftingRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: CraftingService::new(),
            pending: None,
            quotes: BTreeMap::new(),
            catalog: None,
            cold: None,
            unexpected: None,
            unexpected_ticket: None,
            failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.cold.is_some()
            || self.service.requires_drain()
            || self.unexpected.is_some()
            || self.unexpected_ticket.is_some()
    }
    fn accept_owner(&mut self, outcome: CraftingOutcome) -> Result<(), Box<CraftingOutcome>> {
        let Some(p) = self.pending.as_mut().filter(|p| {
            matches!(p.phase, Phase::Submitted) && p.submitted == Some(outcome.correlation)
        }) else {
            return Err(Box::new(outcome));
        };
        match outcome.result {
            Ok(CraftingResult::Animating {
                actor,
                style,
                command,
            }) if actor == p.binding.actor && p.motion.is_none() => {
                p.motion = Some((style, command));
            }
            Ok(CraftingResult::Quoted(chance)) => p.phase = Phase::Quoted(chance),
            Ok(CraftingResult::Pending(op))
                if op != 0 && p.ticket.as_ref().is_none_or(|t| t.operation == op) =>
            {
                p.pending_operation = Some(op);
                p.phase = Phase::Saving
            }
            Ok(CraftingResult::Cancelled) => p.phase = Phase::Done,
            Err(e) => {
                p.rejection = Some(e);
                p.phase = Phase::Failed(format!("crafting rejected: {e:?}"));
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.service.owns_capture(outcome)
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.service.owns_capture(&outcome) {
            return self.service.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
        else {
            return Err(outcome);
        };
        p.phase = match outcome.result {
            Ok(s) if s.binding() == p.binding => Phase::Captured(s, unix),
            Ok(_) => Phase::Failed("crafting capture binding mismatch".into()),
            Err(e) => Phase::Failed(format!("crafting capture: {e:?}")),
        };
        Ok(())
    }
}
pub(super) enum CraftingIngress {
    Accepted,
    Blocked,
    Unsupported,
}
impl GameRuntime {
    pub fn crafting_failure(&self, key: SessionKey) -> Option<&str> {
        self.crafting
            .failures
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.crafting_ingress_blocked(key)
                    .then(|| self.crafting.service.blocked())
                    .flatten()
            })
    }
    pub(super) fn crafting_ingress_blocked(&self, key: SessionKey) -> bool {
        self.crafting.pending.as_ref().is_some_and(|p| p.key == key)
    }
    pub(super) fn retry_crafting(&mut self, key: SessionKey) -> bool {
        if self.crafting_ingress_blocked(key) && self.crafting.service.blocked().is_some() {
            self.crafting.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn forget_crafting_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.crafting_ingress_blocked(key) {
            return Err("crafting remains pending".into());
        }
        self.crafting.quotes.remove(&key);
        self.crafting.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_crafting_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<CraftingIngress, String> {
        if message.bytes.get(..4)
            != Some(
                &bace_wire::opcode::GameMessageOpcode::GameAction
                    .0
                    .to_le_bytes(),
            )
        {
            return Ok(CraftingIngress::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(CraftingIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(CraftingIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let decoded = match bace_session::decode_crafting(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(v) => v,
            Err(bace_session::DispatchError::UnsupportedAction(_)) => {
                return Ok(CraftingIngress::Unsupported);
            }
            Err(e) => return Err(format!("crafting input: {e:?}")),
        };
        if matches!(decoded.request, bace_wire::CraftingAction::Salvage { .. }) {
            return Ok(CraftingIngress::Unsupported);
        }
        if self.crafting.pending.is_some()
            || self.crafting.cold.is_some()
            || self.crafting.service.requires_drain()
            || self.crafting.unexpected.is_some()
            || self.crafting.unexpected_ticket.is_some()
        {
            return Ok(CraftingIngress::Blocked);
        }
        if !self.players.entered(binding.actor) || session.terminated || session.disconnected {
            return Ok(CraftingIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("crafting authenticated binding mismatch".into());
        }
        let (source, target, operation, confirm, phase, native) = match decoded.request {
            bace_wire::CraftingAction::UseWithTarget {
                source_id,
                target_id,
            } => {
                let mut operation = [0; 16];
                OsRng
                    .try_fill_bytes(&mut operation)
                    .map_err(|e| e.to_string())?;
                (source_id, target_id, operation, false, Phase::Capture, None)
            }
            bace_wire::CraftingAction::Confirmation {
                context: token,
                accepted,
                ..
            } => {
                let Some(q) = self.crafting.quotes.get(&key).filter(|q| q.token == token) else {
                    self.crafting
                        .failures
                        .insert(key, "stale crafting confirmation".into());
                    return Ok(CraftingIngress::Accepted);
                };
                (
                    q.input.source.id,
                    q.input.target.id,
                    q.input.context.operation_id,
                    true,
                    if accepted {
                        Phase::Capture
                    } else {
                        Phase::Ready(Box::new(CraftingCommand {
                            correlation: self.next,
                            action: CraftingCommandKind::Cancel { context },
                        }))
                    },
                    Some(q.native.clone()),
                )
            }
            _ => return Ok(CraftingIngress::Unsupported),
        };
        if !confirm && self.crafting.quotes.len() >= 64 && !self.crafting.quotes.contains_key(&key)
        {
            return Ok(CraftingIngress::Blocked);
        }
        if source == 0 || target == 0 || source == target {
            return Err("invalid crafting identities".into());
        }
        self.crafting.pending = Some(Pending {
            key,
            context,
            binding,
            source,
            target,
            operation,
            confirm,
            motion: None,
            phase,
            submitted: None,
            pending_operation: None,
            rejection: None,
            input: None,
            native,
            ticket: None,
            assets: self
                .crafting
                .quotes
                .get(&key)
                .filter(|_| confirm)
                .map(|q| q.assets.clone()),
            completion: None,
        });
        Ok(CraftingIngress::Accepted)
    }
    pub(super) fn poll_crafting(&mut self) -> Result<(), String> {
        self.crafting
            .quotes
            .retain(|_, q| q.expires_at > self.last_elapsed);
        if self.crafting.unexpected.is_some() || self.crafting.unexpected_ticket.is_some() {
            return Err("unrelated crafting output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.crafting_outcomes().try_recv() else {
                break;
            };
            let outcome = match self.crafting.service.accept_outcome(outcome) {
                Ok(()) => continue,
                Err(outcome) => *outcome,
            };
            if let Err(outcome) = self.crafting.accept_owner(outcome) {
                self.crafting.unexpected = Some(*outcome);
                return Err("unrelated crafting outcome retained".into());
            }
        }
        if let Ok(ticket) = self.simulation.crafting_proposals().try_recv() {
            let Some(p) = self.crafting.pending.as_mut() else {
                self.crafting.unexpected_ticket = Some(ticket);
                return Err("unrelated crafting proposal".into());
            };
            let matches = matches!(&ticket.decision,bace_simulation::CraftingDecision::Tinker(t) if t.operation_id==p.operation&&t.actor==p.binding.actor.0);
            if !matches
                || p.ticket.is_some()
                || p.pending_operation.is_some_and(|op| op != ticket.operation)
            {
                self.crafting.unexpected_ticket = Some(ticket);
                return Err("crafting proposal mismatch".into());
            }
            p.ticket = Some(ticket);
        }
        if let Some(result) = ready(&mut self.crafting.cold) {
            let p = self
                .crafting
                .pending
                .as_mut()
                .ok_or("crafting cold owner missing")?;
            match result {
                Ok(prepared) => {
                    self.crafting.catalog = Some(prepared.catalog);
                    p.native = Some(prepared.native);
                    p.input = Some(prepared.input);
                    p.assets = Some(prepared.assets);
                    p.phase = Phase::Capture;
                }
                Err(e) => p.phase = Phase::Failed(e),
            }
        }
        let token = self.token()?;
        if let Some(p) = self.crafting.pending.as_mut() {
            match &mut p.phase {
                Phase::Capture => {
                    let command = Command::PlayerSnapshot(PlayerSnapshotRequest {
                        correlation: token,
                        binding: p.binding,
                        operation: None,
                    });
                    match self.simulation.input().try_submit(command) {
                        Ok(()) => p.phase = Phase::Capturing(token),
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err("crafting capture ingress closed".into());
                        }
                    }
                }
                Phase::Captured(snapshot, unix) => {
                    let baseline = self
                        .online_saves
                        .baseline(p.binding.actor.0)
                        .ok_or("crafting player baseline missing")?
                        .0;
                    let saved =
                        crate::player_saves::freeze_player_snapshot(baseline, snapshot, *unix)
                            .map_err(|e| e.to_string())?;
                    let (source, target) =
                        inputs::items(&self.online_saves, snapshot, p.source, p.target)?;
                    if let Some(native) = p.native.as_ref().filter(|_| p.assets.is_some()) {
                        let input =
                            inputs::build(snapshot, &saved, &source, &target, native, p.operation)?;
                        if p.input.as_ref().is_some_and(|old| {
                            old.source != input.source || old.target != input.target
                        }) {
                            p.phase = Phase::Failed(
                                "crafting items changed during cold preparation".into(),
                            );
                        } else {
                            let dialog = snapshot
                                .character()
                                .ui()
                                .ok_or("crafting UI state missing")?
                                .state
                                .options1
                                & 0x80000000
                                != 0;
                            let action = if p.confirm {
                                CraftingCommandKind::Confirm {
                                    context: p.context,
                                    input: Box::new(input.clone()),
                                }
                            } else {
                                CraftingCommandKind::BeginUse {
                                    context: p.context,
                                    input: Box::new(input.clone()),
                                    motion: p
                                        .assets
                                        .as_ref()
                                        .ok_or("crafting motion assets missing")?
                                        .clap
                                        .clone(),
                                    quote: dialog,
                                    lifetime: 1800,
                                }
                            };
                            p.input = Some(input);
                            p.phase = Phase::Ready(Box::new(CraftingCommand {
                                correlation: 0,
                                action,
                            }));
                        }
                    } else {
                        let generation = self.bootstrap.pack.generation.clone();
                        let manifest = self.bootstrap.assets.clone();
                        let cached = self.crafting.catalog.clone();
                        let operation = p.operation;
                        let snapshot = snapshot.clone();
                        self.crafting.cold = Some(Box::pin(async move {
                            tokio::task::spawn_blocking(move || {
                                let catalog = match cached {
                                    Some(v) if v.matches_generation(&generation) => v,
                                    _ => Arc::new(catalog::Catalog::load(&generation)?),
                                };
                                let native = Arc::new(
                                    catalog.prepare(&source.entity.state, &target.entity.state)?,
                                );
                                let input = inputs::build(
                                    &snapshot, &saved, &source, &target, &native, operation,
                                )?;
                                let mut verified =
                                    crate::region_activation::VerifiedRegionAssets::open(
                                        &manifest,
                                    )?;
                                let assets = Arc::new(ColdAssets {
                                    appearance: verified.prepare_entry_appearance(&[
                                        &source.entity.state,
                                        &target.entity.state,
                                    ])?,
                                    materials: verified.prepare_material_names()?,
                                    clap: verified.prepare_crafting_motion(
                                        &saved.player.entity.state,
                                        snapshot
                                            .entry_motion()
                                            .ok_or("accepted crafting motion state missing")?,
                                    )?,
                                });
                                Ok(Prepared {
                                    catalog,
                                    native,
                                    input,
                                    assets,
                                })
                            })
                            .await
                            .map_err(|e| e.to_string())?
                        }));
                        p.phase = Phase::Preparing;
                    }
                }
                Phase::Saving => {
                    if let Some(ticket) = p.ticket.take() {
                        self.crafting
                            .service
                            .stage(CraftingWork {
                                binding: p.binding,
                                ticket,
                                generated: vec![],
                            })
                            .map_err(|work| {
                                p.ticket = Some(work.ticket);
                                "crafting durable lane busy".to_string()
                            })?;
                    }
                }
                _ => {}
            }
        }
        if let Some(p) = &mut self.crafting.pending
            && matches!(p.phase, Phase::Ready(_))
        {
            let Phase::Ready(mut command) = std::mem::replace(&mut p.phase, Phase::Submitted)
            else {
                unreachable!("ready phase")
            };
            command.correlation = token;
            match self
                .simulation
                .input()
                .try_submit(Command::Crafting(*command))
            {
                Ok(()) => p.submitted = Some(token),
                Err(TrySendError::Full(Command::Crafting(command))) => {
                    p.phase = Phase::Ready(Box::new(command))
                }
                Err(TrySendError::Disconnected(Command::Crafting(command))) => {
                    p.phase = Phase::Ready(Box::new(command));
                    return Err("crafting command ingress closed".into());
                }
                Err(_) => unreachable!("exact crafting command returned"),
            }
        }
        let token = self.token()?;
        self.crafting.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            token,
        )?;
        if let Some(completion) = self.crafting.service.take_completion() {
            let p = self
                .crafting
                .pending
                .as_mut()
                .ok_or("crafting completion owner missing")?;
            p.completion = Some(completion);
        }
        self.project_crafting_output()
    }
}

#[cfg(test)]
mod tests;
