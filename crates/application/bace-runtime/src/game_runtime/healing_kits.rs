//! ACE Healer UseWithTarget route. One retained action survives cold content,
//! simulation output, exact durable receipt and canonical client publication.
mod output;
mod preparation;
use super::*;
use crate::healing_kit_service::{HealingKitCompletion, HealingKitService};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    Command, HealingKitCommand, HealingKitDecision, HealingKitOutcome, HealingKitTicket,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;

type PreparedHealingResult = Result<
    (
        bace_simulation::PreparedHealingKit,
        Arc<bace_motion::PreparedMotionChain>,
    ),
    String,
>;

enum Phase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Preparing,
    Ready(Option<HealingKitCommand>),
    Submitted,
    Animating,
    Saving,
    Finished,
    Rejected(bace_simulation::HealingKitError),
}
struct Pending {
    key: SessionKey,
    context: ActionContext,
    item: EntityId,
    name: String,
    target_name: String,
    vital: Option<bace_entity::EntityVital>,
    revision: u64,
    target: EntityId,
    correlation: u64,
    motion: Option<(u32, u32)>,
    phase: Phase,
    ticket: Option<HealingKitTicket>,
    completion: Option<HealingKitCompletion>,
}
pub(super) struct HealingKitRuntime {
    service: HealingKitService,
    pending: Option<Pending>,
    cold: Option<Job<PreparedHealingResult>>,
    unexpected: Option<Box<HealingKitOutcome>>,
    failures: BTreeMap<SessionKey, String>,
}
impl HealingKitRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: HealingKitService::new(),
            pending: None,
            cold: None,
            unexpected: None,
            failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.cold.is_some()
            || self.service.has_pending()
            || self.unexpected.is_some()
    }
    pub(super) fn owns_capture(&self, outcome: &bace_simulation::PlayerSnapshotOutcome) -> bool {
        self.service.owns_capture(outcome)
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase, Phase::Capturing(c) if c == outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: bace_simulation::PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), bace_simulation::PlayerSnapshotOutcome> {
        if self.service.owns_capture(&outcome) {
            return self.service.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| matches!(p.phase, Phase::Capturing(c) if c == outcome.correlation))
        else {
            return Err(outcome);
        };
        match outcome.result {
            Ok(snapshot) if snapshot.binding().actor == p.context.actor => {
                p.phase = Phase::Captured(snapshot, unix)
            }
            Err(error) => {
                p.phase = Phase::Capture;
                self.failures
                    .insert(p.key, format!("healing preparation capture: {error:?}"));
            }
            other => {
                return Err(bace_simulation::PlayerSnapshotOutcome {
                    correlation: outcome.correlation,
                    result: other,
                });
            }
        }
        Ok(())
    }
}
impl GameRuntime {
    pub fn healing_kit_failure(&self, key: SessionKey) -> Option<&str> {
        self.healing_kits
            .failures
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.healing_kit_ingress_blocked(key)
                    .then(|| self.healing_kits.service.blocked())
                    .flatten()
            })
    }
    pub(super) fn healing_kit_ingress_blocked(&self, key: SessionKey) -> bool {
        self.healing_kits
            .pending
            .as_ref()
            .is_some_and(|p| p.key == key)
    }
    pub(super) fn retry_healing_kit(&mut self, key: SessionKey) -> bool {
        if self.healing_kit_ingress_blocked(key) && self.healing_kits.service.blocked().is_some() {
            self.healing_kits.service.retry();
            self.healing_kits.failures.remove(&key);
            true
        } else {
            false
        }
    }
    pub(super) fn forget_healing_kit_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.healing_kit_ingress_blocked(key) {
            return Err("healing kit durable work remains pending".into());
        }
        self.healing_kits.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_healing_kit_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(session) = self.sessions.get(&key) else {
            return Ok(I::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(I::Unsupported);
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
            Ok(value) => value,
            Err(bace_session::DispatchError::UnsupportedAction(_)) => return Ok(I::Unsupported),
            Err(error) => return Err(format!("healing-kit input: {error:?}")),
        };
        let bace_wire::CraftingAction::UseWithTarget {
            source_id,
            target_id,
        } = decoded.request
        else {
            return Ok(I::Unsupported);
        };
        let item = EntityId(source_id);
        let Some(saved) = self
            .online_saves
            .inventory_baseline(binding.actor.0, source_id)
        else {
            return Ok(I::Unsupported);
        };
        if saved.entity.state.weenie_type != 28 {
            return Ok(I::Unsupported);
        }
        if self.healing_kits.has_pending()
            || self.inventory.has_pending()
            || self.crafting.has_pending()
        {
            return Ok(I::Blocked);
        }
        if !self.players.entered(binding.actor) || session.terminated || session.disconnected {
            return Ok(I::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("healing-kit authenticated binding mismatch".into());
        }
        let name = saved
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 1)
            .map_or("Healing Kit".to_owned(), |p| p.value.clone());
        let target_name = loading.loaded.player.player.name.clone();
        let revision = saved.entity.mutation_revision;
        let correlation = self.token()?;
        self.healing_kits.pending = Some(Pending {
            key,
            context,
            item,
            name,
            target_name,
            vital: None,
            revision,
            target: EntityId(target_id),
            correlation,
            motion: None,
            phase: Phase::Capture,
            ticket: None,
            completion: None,
        });
        self.healing_kits.failures.remove(&key);
        self.diagnostic(
            "info",
            "game.healing_kit_use",
            &format!(
                "session {} generation {} item {source_id:#010x} target {target_id:#010x}",
                key.id, key.generation
            ),
        );
        Ok(I::Accepted)
    }
    pub(super) fn poll_healing_kits(&mut self) -> Result<(), String> {
        if self.healing_kits.unexpected.is_some() {
            return Err("uncorrelated healing owner output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.healing_kit_outcomes().try_recv() else {
                break;
            };
            let Some(pending) = self.healing_kits.pending.as_mut() else {
                self.healing_kits.unexpected = Some(Box::new(outcome));
                return Err("healing-kit outcome without request".into());
            };
            if matches!(pending.phase, Phase::Saving) {
                self.healing_kits
                    .service
                    .accept_owner(outcome)
                    .map_err(|outcome| {
                        self.healing_kits.unexpected = Some(outcome);
                        "healing-kit durable resolution mismatch".to_owned()
                    })?;
                continue;
            }
            if !matches!(pending.phase, Phase::Submitted | Phase::Animating)
                || outcome.correlation != pending.correlation
                || outcome.context != Some(pending.context)
            {
                self.healing_kits.unexpected = Some(Box::new(outcome));
                return Err("healing-kit proposal correlation mismatch".into());
            }
            match outcome.result {
                Ok(HealingKitDecision::MotionStarted {
                    actor,
                    style,
                    command,
                }) if actor == pending.context.actor && pending.motion.is_none() => {
                    pending.motion = Some((style, command));
                    pending.phase = Phase::Animating;
                }
                Ok(HealingKitDecision::Proposed(ticket)) if ticket.context == pending.context => {
                    let mut identity = [0; 16];
                    OsRng
                        .try_fill_bytes(&mut identity)
                        .map_err(|error| error.to_string())?;
                    let binding = CharacterBinding {
                        actor: pending.context.actor,
                        account: pending.context.account,
                        session: pending.context.session,
                    };
                    self.healing_kits
                        .service
                        .stage(identity, binding, ticket.clone())
                        .map_err(|_| "healing-kit service busy".to_owned())?;
                    pending.ticket = Some(ticket);
                    pending.phase = Phase::Saving;
                }
                Err(error) => {
                    self.healing_kits
                        .failures
                        .insert(pending.key, format!("healing-kit: {error:?}"));
                    pending.phase = Phase::Rejected(error);
                }
                _ => {
                    self.healing_kits.unexpected = Some(Box::new(outcome));
                    return Err("healing-kit unexpected owner decision".into());
                }
            }
        }
        if self
            .healing_kits
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capture))
        {
            let correlation = self.token()?;
            let p = self
                .healing_kits
                .pending
                .as_mut()
                .expect("healing capture owner");
            let binding = CharacterBinding {
                actor: p.context.actor,
                account: p.context.account,
                session: p.context.session,
            };
            match self.simulation.input().try_submit(Command::PlayerSnapshot(
                bace_simulation::PlayerSnapshotRequest {
                    correlation,
                    binding,
                    operation: None,
                },
            )) {
                Ok(()) => p.phase = Phase::Capturing(correlation),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("healing preparation snapshot input closed".into());
                }
            }
        }
        if self.healing_kits.cold.is_none() {
            let captured = self
                .healing_kits
                .pending
                .as_ref()
                .and_then(|p| match &p.phase {
                    Phase::Captured(snapshot, unix) => {
                        Some((p.context.actor, p.item, snapshot.clone(), *unix))
                    }
                    _ => None,
                });
            if let Some((actor, item, snapshot, unix)) = captured {
                let baseline = self
                    .online_saves
                    .baseline(actor.0)
                    .ok_or("healing player baseline missing")?
                    .0;
                let player = crate::player_saves::freeze_player_snapshot(baseline, &snapshot, unix)
                    .map_err(|error| error.to_string())?;
                let source = self
                    .online_saves
                    .inventory_baseline(actor.0, item.0)
                    .ok_or("healing item baseline missing")?
                    .entity
                    .state
                    .clone();
                let current = snapshot
                    .entry_motion()
                    .ok_or("healing accepted motion state missing")?;
                let generation = self.bootstrap.pack.generation.clone();
                let manifest = self.bootstrap.assets.clone();
                self.healing_kits.cold = Some(Box::pin(async move {
                    tokio::task::spawn_blocking(move || {
                        let bace_storage_codec::PackLookup::Record(record) = generation
                            .lookup(bace_storage_codec::PackKey {
                                namespace: 1,
                                id: u64::from(source.weenie_id),
                            })
                            .map_err(|error| error.to_string())?
                        else {
                            return Err("healing-kit template missing".into());
                        };
                        let template = bace_content_tools::decode(record.bytes())
                            .map_err(|error| error.to_string())?;
                        let profile = preparation::profile(&source, &template)?;
                        let mut assets =
                            crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                        let motion =
                            assets.prepare_healing_motion(&player.player.entity.state, current)?;
                        Ok((profile, motion))
                    })
                    .await
                    .map_err(|error| error.to_string())?
                }));
                self.healing_kits
                    .pending
                    .as_mut()
                    .expect("healing cold owner")
                    .phase = Phase::Preparing;
            }
        }
        if let Some(result) = ready(&mut self.healing_kits.cold) {
            let p = self
                .healing_kits
                .pending
                .as_mut()
                .ok_or("healing cold request missing")?;
            match result {
                Ok((profile, motion)) => {
                    p.vital = Some(profile.vital);
                    p.phase = Phase::Ready(Some(HealingKitCommand::Use {
                        correlation: p.correlation,
                        context: p.context,
                        item: p.item,
                        item_revision: p.revision,
                        target: p.target,
                        profile,
                        motion,
                    }));
                }
                Err(error) => {
                    self.healing_kits.failures.insert(p.key, error);
                    p.phase = Phase::Rejected(bace_simulation::HealingKitError::Content);
                }
            }
        }
        if let Some(p) = self.healing_kits.pending.as_mut()
            && let Phase::Ready(command) = &mut p.phase
            && let Some(value) = command.take()
        {
            match self
                .simulation
                .input()
                .try_submit(Command::HealingKit(value))
            {
                Ok(()) => p.phase = Phase::Submitted,
                Err(TrySendError::Full(Command::HealingKit(value))) => *command = Some(value),
                Err(TrySendError::Disconnected(Command::HealingKit(value))) => {
                    *command = Some(value);
                    return Err("healing owner input closed".into());
                }
                Err(_) => return Err("healing command ownership".into()),
            }
        }
        let correlation = self.token()?;
        if let Err(error) = self.healing_kits.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        ) {
            let key = self
                .healing_kits
                .pending
                .as_ref()
                .map(|p| p.key)
                .ok_or("healing service blocked without request")?;
            if self
                .healing_kits
                .failures
                .get(&key)
                .is_none_or(|prior| prior != &error)
            {
                self.diagnostic(
                    "error",
                    "game.healing_kit_durability",
                    &format!("session {} generation {}: {error}", key.id, key.generation),
                );
            }
            self.healing_kits.failures.insert(key, error);
            return Ok(());
        }
        if let Some(completion) = self.healing_kits.service.take_completion() {
            let p = self
                .healing_kits
                .pending
                .as_mut()
                .ok_or("healing completion without request")?;
            if p.ticket
                .as_ref()
                .is_none_or(|ticket| ticket.operation != completion.ticket.operation)
            {
                return Err("healing completion operation mismatch".into());
            }
            p.completion = Some(completion);
            p.phase = Phase::Finished;
        }
        self.project_healing_kit_output()?;
        Ok(())
    }
}
