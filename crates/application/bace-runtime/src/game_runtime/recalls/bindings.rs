//! Authenticated stone Use and the retained source action/receipt transcript.
use super::*;
use crate::game_runtime::progression::ProgressionIngress;
use bace_interactions::{BindingKind, RecallError};
use bace_replication::{BatchLimits, InventoryProjection as P};
use bace_types::EntityId;
use bace_wire::{GameActionEnvelope, InventoryAction, InventoryRequest};

enum BindingPhase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Preparing,
    Ready,
    Submitted,
    Running,
    Staged,
    Failed(String),
}
struct BindingPrepared {
    revision: u64,
    style: Option<Arc<bace_motion::PreparedMotionChain>>,
    motion: Arc<bace_motion::PreparedMotionChain>,
    seconds: f64,
}
pub(super) struct BindingPending {
    context: ActionContext,
    binding: CharacterBinding,
    object: EntityId,
    kind: BindingKind,
    phase: BindingPhase,
    prepared: Option<BindingPrepared>,
    staged: Option<(u64, bool, Arc<str>, Option<u32>)>,
    cancelling: bool,
}
pub(super) struct BindingRuntime {
    pub(super) pending: BTreeMap<SessionKey, BindingPending>,
    objects: BTreeMap<EntityId, BindingKind>,
    cold: Option<Job<(SessionKey, Result<BindingPrepared, String>)>>,
    completions: BTreeMap<u64, bool>,
    failures: BTreeMap<SessionKey, String>,
}
impl BindingRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            objects: BTreeMap::new(),
            cold: None,
            completions: BTreeMap::new(),
            failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.cold.is_some() || !self.completions.is_empty()
    }
    pub(super) fn output_pending(&self) -> bool {
        self.pending.values().any(|p| {
            matches!(p.phase, BindingPhase::Submitted)
                || p.staged
                    .as_ref()
                    .is_some_and(|(operation, ..)| self.completions.contains_key(operation))
        })
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.pending
            .values()
            .any(|p| matches!(p.phase, BindingPhase::Capturing(id) if id == outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        let Some(p) = self
            .pending
            .values_mut()
            .find(|p| matches!(p.phase, BindingPhase::Capturing(id) if id == outcome.correlation))
        else {
            return Err(outcome);
        };
        p.phase = match outcome.result {
            Ok(snapshot) if snapshot.binding() == p.binding => {
                BindingPhase::Captured(snapshot, unix)
            }
            Ok(_) => BindingPhase::Failed("binding snapshot owner mismatch".into()),
            Err(error) => BindingPhase::Failed(format!("binding snapshot: {error:?}")),
        };
        Ok(())
    }
    pub(super) fn matches_event(&self, event: &RecallEvent) -> bool {
        if matches!(
            event,
            RecallEvent::BindingStarted { .. }
                | RecallEvent::BindingActionStarted { .. }
                | RecallEvent::BindingStaged { .. }
        ) {
            return true;
        }
        let context = match event {
            RecallEvent::Retry { context }
            | RecallEvent::Rejected { context, .. }
            | RecallEvent::Cancelled { context } => context,
            _ => return false,
        };
        self.pending.values().any(|p| p.context == *context)
    }
    fn record_completion(&mut self, operation: u64, committed: bool) -> Result<(), String> {
        if operation == 0
            || self.completions.len() >= CAPACITY && !self.completions.contains_key(&operation)
        {
            return Err("binding completion capacity or identity".into());
        }
        if self
            .completions
            .get(&operation)
            .is_some_and(|old| *old != committed)
        {
            return Err("binding completion changed".into());
        }
        self.completions.insert(operation, committed);
        Ok(())
    }
}
impl GameRuntime {
    pub fn binding_failure(&self, key: SessionKey) -> Option<&str> {
        if let Some(error) = self.recalls.bindings.failures.get(&key) {
            return Some(error);
        }
        match &self.recalls.bindings.pending.get(&key)?.phase {
            BindingPhase::Failed(error) => Some(error),
            _ => None,
        }
    }
    pub(super) fn retry_binding(&mut self, key: SessionKey) -> bool {
        let Some(p) = self.recalls.bindings.pending.get_mut(&key) else {
            return false;
        };
        if !matches!(p.phase, BindingPhase::Failed(_)) {
            return false;
        }
        p.phase = BindingPhase::Capture;
        p.prepared = None;
        p.staged = None;
        true
    }
    pub fn register_binding_presentation(
        &mut self,
        object: EntityId,
        kind: BindingKind,
    ) -> Result<(), String> {
        if object.0 == 0
            || self.recalls.bindings.objects.len() >= 4096
            || self.recalls.bindings.objects.contains_key(&object)
        {
            return Err("binding presentation capacity or duplicate".into());
        }
        self.recalls.bindings.objects.insert(object, kind);
        Ok(())
    }
    pub fn unregister_binding_presentation(&mut self, object: EntityId) -> Result<(), String> {
        // Region retirement cancels the simulation action and queues its
        // retained Cancelled event. The presentation may leave now: the
        // existing action keeps its exact object identity until that event.
        self.recalls.bindings.objects.remove(&object);
        Ok(())
    }
    pub(in crate::game_runtime) fn record_binding_allegiance_completion(
        &mut self,
        operation: u64,
        committed: bool,
    ) -> Result<(), String> {
        if self.recalls.bindings.pending.values().any(|p| {
            p.staged
                .as_ref()
                .is_some_and(|(id, allegiance, ..)| *id == operation && *allegiance)
        }) {
            self.recalls
                .bindings
                .record_completion(operation, committed)?;
        }
        Ok(())
    }
    pub(in crate::game_runtime) fn record_binding_portal_completion(
        &mut self,
        operation: u64,
        committed: bool,
    ) -> Result<(), String> {
        if self.recalls.bindings.pending.values().any(|p| {
            p.staged
                .as_ref()
                .is_some_and(|(id, allegiance, ..)| *id == operation && !*allegiance)
        }) {
            self.recalls
                .bindings
                .record_completion(operation, committed)?;
        }
        Ok(())
    }
    pub(in crate::game_runtime) fn handle_binding_message(
        &mut self,
        key: SessionKey,
        message: &bace_transport::ReceivedMessage,
    ) -> Result<ProgressionIngress, String> {
        let Some(session) = self.sessions.get(&key) else {
            return Ok(ProgressionIngress::Blocked);
        };
        if session.terminated || session.disconnected {
            return Ok(ProgressionIngress::Blocked);
        }
        let Some(loading) = session.loading.as_ref() else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        if !self.players.entered(binding.actor) {
            return Ok(ProgressionIngress::Unsupported);
        }
        let envelope = match GameActionEnvelope::decode(&message.bytes, self.limits.message_bytes) {
            Ok(value) => value,
            Err(bace_wire::WireError::UnexpectedOpcode(_)) => {
                return Ok(ProgressionIngress::Unsupported);
            }
            Err(error) => return Err(format!("binding envelope: {error:?}")),
        };
        if envelope.action != bace_wire::opcode::GameActionType::Use {
            return Ok(ProgressionIngress::Unsupported);
        }
        let request = InventoryRequest::decode(
            envelope.action,
            envelope.payload,
            self.limits.message_bytes,
            0,
        )
        .map_err(|error| format!("binding Use: {error:?}"))?;
        let InventoryAction::Use(raw) = request.action else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let object = EntityId(raw);
        let Some(&kind) = self.recalls.bindings.objects.get(&object) else {
            if self
                .world
                .as_ref()
                .is_some_and(|world| world.regions.binding_notice_pending(object))
            {
                return Ok(ProgressionIngress::Blocked);
            }
            return Ok(ProgressionIngress::Unsupported);
        };
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("binding authenticated owner mismatch".into());
        }
        if self.recalls.bindings.pending.len() >= CAPACITY
            || self.recalls.bindings.pending.contains_key(&key)
        {
            return Ok(ProgressionIngress::Blocked);
        }
        self.recalls.bindings.failures.remove(&key);
        self.recalls.bindings.pending.insert(
            key,
            BindingPending {
                context: ActionContext {
                    actor: binding.actor,
                    account: binding.account,
                    session: binding.session,
                    sequence: message.sequence,
                },
                binding,
                object,
                kind,
                phase: BindingPhase::Capture,
                prepared: None,
                staged: None,
                cancelling: false,
            },
        );
        Ok(ProgressionIngress::Accepted)
    }
    pub(super) fn poll_bindings(&mut self) -> Result<(), String> {
        self.recalls
            .bindings
            .failures
            .retain(|key, _| self.sessions.contains_key(key));
        if let Some((key, result)) = ready(&mut self.recalls.bindings.cold) {
            let p = self
                .recalls
                .bindings
                .pending
                .get_mut(&key)
                .ok_or("binding cold owner missing")?;
            match result {
                Ok(prepared) => {
                    p.prepared = Some(prepared);
                    p.phase = BindingPhase::Ready;
                }
                Err(error) => p.phase = BindingPhase::Failed(error),
            }
        }
        let keys: Vec<_> = self
            .recalls
            .bindings
            .pending
            .keys()
            .copied()
            .take(self.limits.work_per_poll)
            .collect();
        for key in keys {
            if self.binding_session_closing(key) {
                let phase = &self.recalls.bindings.pending[&key].phase;
                if matches!(
                    phase,
                    BindingPhase::Capture
                        | BindingPhase::Captured(..)
                        | BindingPhase::Ready
                        | BindingPhase::Failed(_)
                ) {
                    self.recalls.bindings.pending.remove(&key);
                    continue;
                }
                if matches!(phase, BindingPhase::Submitted | BindingPhase::Running)
                    && !self.recalls.bindings.pending[&key].cancelling
                {
                    let actor = self.recalls.bindings.pending[&key].context.actor;
                    match self
                        .simulation
                        .input()
                        .try_submit(Command::Recall(RecallCommand::Cancel { actor }))
                    {
                        Ok(()) => {
                            self.recalls
                                .bindings
                                .pending
                                .get_mut(&key)
                                .expect("retained binding")
                                .cancelling = true
                        }
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err("binding cancel owner channel closed".into());
                        }
                    }
                }
                continue;
            }
            self.advance_binding(key)?;
        }
        self.project_completed_bindings()?;
        Ok(())
    }
    fn advance_binding(&mut self, key: SessionKey) -> Result<(), String> {
        let token = self.token()?;
        let p = self
            .recalls
            .bindings
            .pending
            .get_mut(&key)
            .ok_or("binding pending owner missing")?;
        match &p.phase {
            BindingPhase::Capture => match self.simulation.input().try_submit(
                Command::PlayerSnapshot(PlayerSnapshotRequest {
                    correlation: token,
                    binding: p.binding,
                    operation: None,
                }),
            ) {
                Ok(()) => p.phase = BindingPhase::Capturing(token),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("binding capture channel closed".into());
                }
            },
            BindingPhase::Captured(snapshot, unix) if self.recalls.bindings.cold.is_none() => {
                let saved = crate::player_saves::freeze_player_snapshot(
                    self.online_saves
                        .baseline(p.binding.actor.0)
                        .ok_or("binding baseline missing")?
                        .0,
                    snapshot,
                    *unix,
                )
                .map_err(|e| e.to_string())?;
                let current = snapshot
                    .entry_motion()
                    .ok_or("binding accepted motion missing")?;
                let revision = snapshot.character().progression().revision();
                let manifest = self.bootstrap.assets.clone();
                self.recalls.bindings.cold = Some(Box::pin(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let mut assets =
                            crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                        let prepared =
                            assets.prepare_binding_motion(&saved.player.entity.state, current)?;
                        Ok(BindingPrepared {
                            revision,
                            style: prepared.style,
                            motion: prepared.motion,
                            seconds: prepared.seconds,
                        })
                    })
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|value| value);
                    (key, result)
                }));
                p.phase = BindingPhase::Preparing;
            }
            BindingPhase::Ready => {
                let prepared = p.prepared.as_ref().ok_or("binding motion missing")?;
                let command = Command::Recall(RecallCommand::UseBinding {
                    context: p.context,
                    object: p.object,
                    before_revision: prepared.revision,
                    animation_seconds: prepared.seconds,
                    style: prepared.style.clone(),
                    motion: prepared.motion.clone(),
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = BindingPhase::Submitted,
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("binding owner channel closed".into());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn binding_session_closing(&self, key: SessionKey) -> bool {
        self.sessions
            .get(&key)
            .is_none_or(|s| s.terminated || s.disconnected)
    }
    pub(super) fn project_binding_event(&mut self, event: RecallEvent) -> Result<(), String> {
        let context = match &event {
            RecallEvent::Retry { context }
            | RecallEvent::BindingStarted { context, .. }
            | RecallEvent::BindingActionStarted { context, .. }
            | RecallEvent::BindingStaged { context, .. }
            | RecallEvent::Rejected { context, .. }
            | RecallEvent::Cancelled { context } => *context,
            _ => return Err("nonbinding event on binding output".into()),
        };
        let key = self
            .recalls
            .bindings
            .pending
            .iter()
            .find_map(|(key, p)| (p.context == context).then_some(*key))
            .ok_or("unmatched binding event retained")?;
        let p = self
            .recalls
            .bindings
            .pending
            .get(&key)
            .ok_or("binding event owner missing")?;
        let closing = self.binding_session_closing(key);
        let mut terminal = false;
        let mut steps = Vec::new();
        let mut started = false;
        let mut action_started = false;
        match &event {
            RecallEvent::Retry { .. } => {
                if !matches!(p.phase, BindingPhase::Submitted) {
                    return Err("binding retry phase".into());
                }
                if closing {
                    self.recalls.bindings.pending.remove(&key);
                    return Ok(());
                }
                let p = self
                    .recalls
                    .bindings
                    .pending
                    .get_mut(&key)
                    .expect("matched binding");
                p.phase = BindingPhase::Capture;
                p.prepared = None;
                return Ok(());
            }
            RecallEvent::BindingStarted { object, .. } => {
                if *object != p.object || !matches!(p.phase, BindingPhase::Submitted) {
                    return Err("binding start identity".into());
                }
                action_started = p.prepared.as_ref().is_some_and(|v| v.style.is_none());
                if action_started && p.kind == BindingKind::Lifestone {
                    steps.push(P::Effect(bace_wire::CombatEffect::Sound {
                        object_id: context.actor.0,
                        sound_id: 0x51,
                        volume: 1.,
                    }));
                }
                started = true;
            }
            RecallEvent::BindingActionStarted { object, .. } => {
                if *object != p.object
                    || !matches!(p.phase, BindingPhase::Running)
                    || p.prepared.as_ref().is_none_or(|v| v.style.is_none())
                {
                    return Err("binding delayed action identity".into());
                }
                if p.kind == BindingKind::Lifestone {
                    steps.push(P::Effect(bace_wire::CombatEffect::Sound {
                        object_id: context.actor.0,
                        sound_id: 0x51,
                        volume: 1.,
                    }));
                }
                started = true;
                action_started = true;
            }
            RecallEvent::BindingStaged {
                object,
                operation,
                allegiance,
                use_message,
                stamina_after,
                ..
            } => {
                if *object != p.object
                    || !matches!(p.phase, BindingPhase::Running)
                    || *allegiance != (p.kind == BindingKind::Allegiance)
                    || *operation == 0
                {
                    return Err("binding stage identity".into());
                }
                let p = self
                    .recalls
                    .bindings
                    .pending
                    .get_mut(&key)
                    .expect("matched binding");
                p.staged = Some((*operation, *allegiance, use_message.clone(), *stamina_after));
                p.phase = BindingPhase::Staged;
                return Ok(());
            }
            RecallEvent::Rejected { error, .. } => {
                if *error == RecallError::Stale || closing {
                    self.recalls.bindings.pending.remove(&key);
                    return Ok(());
                }
                let code = match error {
                    RecallError::NoAllegiance => 0x414,
                    RecallError::Invalid if p.kind == BindingKind::Allegiance => 0x535,
                    RecallError::MovedTooFar => 0x498,
                    _ => {
                        return Err(format!(
                            "binding infrastructure rejection retained: {error:?}"
                        ));
                    }
                };
                steps.push(P::Simple(bace_wire::SimpleGameEvent::WeenieError(code)));
                terminal = true;
            }
            RecallEvent::Cancelled { .. } => terminal = true,
            _ => unreachable!(),
        }
        if (!steps.is_empty() || started) && !closing {
            let observer_count = if action_started && p.kind == BindingKind::Allegiance {
                2
            } else {
                1
            };
            if self.visibility.service.pending()
                || self.network_output.len() >= self.limits.messages
                || !self.observer_room(observer_count, 16 * 1024)
            {
                return Err("binding output pressure retained".into());
            }
            let (kind, object, style) = (
                p.kind,
                p.object,
                p.prepared.as_ref().and_then(|v| v.style.as_ref()).is_some(),
            );
            let r = self
                .players
                .replication(context.actor)
                .ok_or("binding canonical recipient missing")?;
            let mut messages = Vec::new();
            let mut actor_observers = Vec::new();
            if !steps.is_empty() {
                let batch = r
                    .events
                    .project_inventory_with_actor(
                        p.binding,
                        &steps,
                        &mut r.item_properties,
                        Some(&mut r.properties),
                        binding_objects(self.limits.message_bytes),
                        binding_limits(self.limits.message_bytes),
                    )
                    .map_err(|error| format!("binding projection: {error:?}"))?;
                if started {
                    actor_observers.extend(batch.messages.iter().cloned());
                }
                messages.extend(batch.messages.into_iter().map(|m| (m.queue, m.bytes)));
            }
            if started {
                if style && !action_started {
                    let transition = bace_replication::project_server_motion(
                        context.actor.0,
                        &binding_motion_view(None),
                        &mut r.properties,
                        binding_limits(self.limits.message_bytes),
                    )
                    .map_err(|error| format!("binding stance projection: {error:?}"))?;
                    messages.push((transition.queue, transition.bytes.clone()));
                    actor_observers.push(transition);
                }
                if action_started {
                    let action = bace_replication::project_server_motion(
                        context.actor.0,
                        &binding_motion_view(Some(0x57)),
                        &mut r.properties,
                        binding_limits(self.limits.message_bytes),
                    )
                    .map_err(|error| format!("binding action projection: {error:?}"))?;
                    messages.push((action.queue, action.bytes.clone()));
                    actor_observers.push(action);
                }
            }
            let mut observer_batches = Vec::new();
            if action_started && kind == BindingKind::Allegiance {
                let stone = self
                    .visibility
                    .service
                    .project_object_motion(object, &binding_stone_motion_view())
                    .map_err(|error| format!("binding stone motion: {error:?}"))?;
                observer_batches.push((object, vec![stone]));
            }
            if !actor_observers.is_empty() {
                observer_batches.push((context.actor, actor_observers));
            }
            if !observer_batches.is_empty() {
                self.retain_observer_messages(observer_batches)?;
            }
            self.network_output
                .push_back(NetworkCommand::SendOrderedBatch { key, messages });
        }
        if terminal {
            self.recalls.bindings.pending.remove(&key);
        } else {
            self.recalls
                .bindings
                .pending
                .get_mut(&key)
                .expect("matched binding")
                .phase = BindingPhase::Running;
        }
        Ok(())
    }
    fn project_completed_bindings(&mut self) -> Result<(), String> {
        let ready = self.recalls.bindings.pending.iter().find_map(|(key, p)| {
            let (operation, ..) = p.staged.as_ref()?;
            self.recalls
                .bindings
                .completions
                .get(operation)
                .map(|committed| (*key, *operation, *committed))
        });
        let Some((key, operation, committed)) = ready else {
            return Ok(());
        };
        if !committed {
            self.recalls
                .bindings
                .failures
                .insert(key, "binding durable operation rejected".into());
            self.recalls.bindings.completions.remove(&operation);
            self.recalls.bindings.pending.remove(&key);
            return Ok(());
        }
        if self.binding_session_closing(key) {
            self.recalls.bindings.completions.remove(&operation);
            self.recalls.bindings.pending.remove(&key);
            return Ok(());
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(());
        }
        let p = self
            .recalls
            .bindings
            .pending
            .get(&key)
            .ok_or("binding completion owner missing")?;
        let (_, _, message, stamina) = p.staged.as_ref().ok_or("binding staged receipt missing")?;
        let mut steps = vec![P::System {
            text: message,
            chat_type: 7,
        }];
        if let Some(current) = stamina {
            steps.push(P::Vital {
                vital: 4,
                current: *current,
            });
        }
        let r = self
            .players
            .replication(p.context.actor)
            .ok_or("binding canonical recipient missing")?;
        let batch = r
            .events
            .project_inventory_with_actor(
                p.binding,
                &steps,
                &mut r.item_properties,
                Some(&mut r.properties),
                binding_objects(self.limits.message_bytes),
                binding_limits(self.limits.message_bytes),
            )
            .map_err(|error| format!("binding completion projection: {error:?}"))?;
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch).map_err(|e| e.to_string())?,
        );
        self.recalls.bindings.completions.remove(&operation);
        self.recalls.bindings.pending.remove(&key);
        Ok(())
    }
}

fn binding_motion_view(action: Option<u16>) -> bace_wire::MovementDescription {
    bace_wire::MovementDescription {
        autonomous: false,
        motion_flags: 0,
        current_style: 0x3d,
        body: bace_wire::MotionBody::State {
            state: bace_wire::InterpretedMotion {
                current_style: Some(0x3d),
                forward_command: Some(3),
                commands: action
                    .into_iter()
                    .map(|raw_command| bace_wire::MotionCommandItem {
                        raw_command,
                        sequence: 0,
                        autonomous: false,
                        speed: 1.,
                    })
                    .collect(),
                ..Default::default()
            },
            sticky_object: None,
        },
    }
}

fn binding_stone_motion_view() -> bace_wire::MovementDescription {
    let mut view = binding_motion_view(Some(0x51));
    view.current_style = 0x3d;
    view
}

fn binding_limits(bytes: usize) -> BatchLimits {
    BatchLimits {
        max_messages: 4,
        max_bytes: bytes,
        max_message_bytes: bytes,
        max_string_bytes: 4096,
    }
}
fn binding_objects(bytes: usize) -> bace_wire::ObjectCodecLimits {
    bace_wire::ObjectCodecLimits {
        max_message_bytes: bytes,
        max_model_entries: 255,
        max_children: 128,
        max_restrictions: 1024,
        max_motion_commands: 32,
        max_string_bytes: 4096,
    }
}

#[cfg(test)]
#[path = "bindings/tests.rs"]
mod tests;
