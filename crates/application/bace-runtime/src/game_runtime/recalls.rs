//! Bounded authenticated command recalls. One cold job prepares immutable DAT
//! evidence; each actor retains its action independently through owner output.
mod admission;
mod bindings;
mod output;
use super::*;
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_interactions::RecallKind;
use bace_simulation::{
    Command, PlayerSnapshotOutcome, PlayerSnapshotRequest, RecallCommand, RecallEvent,
};
use std::sync::mpsc::TrySendError;
const CAPACITY: usize = 64;
enum Phase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>, u64),
    Preparing,
    Regions,
    Ready,
    Submitted,
    Running,
    Failed(String),
}
struct Pending {
    context: ActionContext,
    binding: CharacterBinding,
    kind: RecallKind,
    name: String,
    phase: Phase,
    prepared: Option<Prepared>,
}
struct Prepared {
    motion: crate::region_activation::PreparedRecallMotion,
    revision: u64,
    blocks: Vec<u16>,
}
pub(super) struct RecallRuntime {
    pending: BTreeMap<SessionKey, Pending>,
    cold: Option<Job<(SessionKey, Result<Prepared, String>)>>,
    regions: BTreeMap<u64, SessionKey>,
    event: Option<RecallEvent>,
    cursor: Option<SessionKey>,
    rejected: BTreeMap<SessionKey, bace_interactions::RecallError>,
    bindings: bindings::BindingRuntime,
}
impl RecallRuntime {
    pub(super) fn output_pending(&self) -> bool {
        self.bindings.output_pending()
            || self.event.is_some()
            || self
                .pending
                .values()
                .any(|p| matches!(p.phase, Phase::Submitted))
    }
    pub(super) fn new() -> Self {
        Self {
            pending: BTreeMap::new(),
            cold: None,
            regions: BTreeMap::new(),
            event: None,
            cursor: None,
            rejected: BTreeMap::new(),
            bindings: bindings::BindingRuntime::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.bindings.has_pending()
            || !self.pending.is_empty()
            || self.cold.is_some()
            || self.event.is_some()
            || !self.regions.is_empty()
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.bindings.owns_capture(outcome)
            || self
                .pending
                .values()
                .any(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.bindings.owns_capture(&outcome) {
            return self.bindings.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .pending
            .values_mut()
            .find(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
        else {
            return Err(outcome);
        };
        p.phase = match outcome.result {
            Ok(snapshot) if snapshot.binding() == p.binding => Phase::Captured(snapshot, unix),
            Ok(_) => Phase::Failed("recall snapshot binding mismatch".into()),
            Err(e) => Phase::Failed(format!("recall capture: {e:?}")),
        };
        Ok(())
    }
}
impl GameRuntime {
    pub fn recall_rejection(&self, key: SessionKey) -> Option<bace_interactions::RecallError> {
        self.recalls.rejected.get(&key).copied()
    }
    pub(super) fn recall_ingress_blocked(&self, key: SessionKey) -> bool {
        self.recalls.bindings.pending.contains_key(&key)
            || self
                .recalls
                .pending
                .get(&key)
                .is_some_and(|p| !matches!(p.phase, Phase::Running))
    }
    pub(super) fn recall_session_pending(&self, key: SessionKey) -> bool {
        self.recalls.pending.contains_key(&key) || self.recalls.bindings.pending.contains_key(&key)
    }
    pub fn recall_failure(&self, key: SessionKey) -> Option<&str> {
        if let Some(error) = self.binding_failure(key) {
            return Some(error);
        }
        match &self.recalls.pending.get(&key)?.phase {
            Phase::Failed(e) => Some(e),
            _ => None,
        }
    }
    pub(super) fn retry_recall(&mut self, key: SessionKey) -> bool {
        if self.retry_binding(key) {
            return true;
        }
        let Some(p) = self.recalls.pending.get_mut(&key) else {
            return false;
        };
        if !matches!(p.phase, Phase::Failed(_)) {
            return false;
        }
        p.phase = Phase::Capture;
        p.prepared = None;
        true
    }
    pub(super) fn recall_region_outcome(
        &mut self,
        correlation: u64,
        result: Result<(), bace_simulation::GeneratorServiceError>,
    ) -> bool {
        let Some(key) = self.recalls.regions.remove(&correlation) else {
            return false;
        };
        if let Err(e) = result
            && let Some(p) = self.recalls.pending.get_mut(&key)
        {
            p.phase = Phase::Failed(format!("recall region request: {e:?}"));
        }
        true
    }
    pub(super) fn poll_recalls(&mut self) -> Result<(), String> {
        self.recalls
            .rejected
            .retain(|key, _| self.sessions.contains_key(key));
        self.project_recall_output()?;
        self.poll_bindings()?;
        if let Some((key, result)) = ready(&mut self.recalls.cold) {
            let p = self
                .recalls
                .pending
                .get_mut(&key)
                .ok_or("recall cold owner missing")?;
            match result {
                Ok(prepared) => {
                    p.prepared = Some(prepared);
                    p.phase = Phase::Regions;
                }
                Err(e) => p.phase = Phase::Failed(e),
            }
        }
        let mut keys: Vec<_> = self.recalls.pending.keys().copied().collect();
        if let Some(cursor) = self.recalls.cursor {
            let split = keys.partition_point(|key| *key <= cursor);
            keys.rotate_left(split);
        }
        for key in keys.into_iter().take(self.limits.work_per_poll) {
            self.recalls.cursor = Some(key);
            self.advance_recall(key)?;
        }
        Ok(())
    }
    fn advance_recall(&mut self, key: SessionKey) -> Result<(), String> {
        let token = self.token()?;
        let p = self
            .recalls
            .pending
            .get_mut(&key)
            .ok_or("recall pending missing")?;
        match &p.phase {
            Phase::Capture => {
                match self.simulation.input().try_submit(Command::PlayerSnapshot(
                    PlayerSnapshotRequest {
                        correlation: token,
                        binding: p.binding,
                        operation: None,
                    },
                )) {
                    Ok(()) => p.phase = Phase::Capturing(token),
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("recall capture channel closed".into());
                    }
                }
            }
            Phase::Captured(snapshot, unix) if self.recalls.cold.is_none() => {
                let baseline = self
                    .online_saves
                    .baseline(p.binding.actor.0)
                    .ok_or("recall baseline missing")?
                    .0;
                let saved = crate::player_saves::freeze_player_snapshot(baseline, snapshot, *unix)
                    .map_err(|e| e.to_string())?;
                let current = snapshot
                    .entry_motion()
                    .ok_or("recall accepted motion missing")?;
                let revision = snapshot.character().progression().revision();
                // A missing destination is deliberately left for the owner to
                // reject in source policy order. Only known candidates load assets.
                let mut blocks = Vec::new();
                if let Ok(candidates) = snapshot.recall_destinations().candidates(p.kind) {
                    for destination in candidates {
                        let block = (destination.cell >> 16) as u16;
                        if !blocks.contains(&block) {
                            blocks.push(block);
                        }
                    }
                }
                let kind = p.kind;
                let manifest = self.bootstrap.assets.clone();
                self.recalls.cold = Some(Box::pin(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        let mut assets =
                            crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
                        let motion = assets.prepare_recall_motion(
                            &saved.player.entity.state,
                            current,
                            kind,
                        )?;
                        Ok(Prepared {
                            motion,
                            revision,
                            blocks,
                        })
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|v| v);
                    (key, result)
                }));
                p.phase = Phase::Preparing;
            }
            Phase::Regions => {
                if self.recalls.regions.values().any(|k| *k == key) {
                    return Ok(());
                }
                let Some(world) = self.world.as_ref() else {
                    return Ok(());
                };
                let prepared = p
                    .prepared
                    .as_ref()
                    .ok_or("recall prepared motion missing")?;
                let missing = prepared
                    .blocks
                    .iter()
                    .copied()
                    .find(|block| world.regions.prepared_region(*block).is_none());
                if let Some(block) = missing {
                    match self.simulation.input().try_submit(Command::Generator(
                        bace_simulation::GeneratorCommand {
                            correlation: token,
                            action: bace_simulation::GeneratorAction::RequestRegion {
                                landblock: block,
                                permanent: false,
                            },
                        },
                    )) {
                        Ok(()) => {
                            self.request_regions.insert(token, (key, block));
                            self.recalls.regions.insert(token, key);
                        }
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => {
                            return Err("recall region channel closed".into());
                        }
                    }
                } else {
                    p.phase = Phase::Ready;
                }
            }
            Phase::Ready => {
                let prepared = p
                    .prepared
                    .as_ref()
                    .ok_or("recall prepared evidence missing")?;
                let command = Command::Recall(RecallCommand::StartPrepared {
                    context: p.context,
                    kind: p.kind,
                    before_revision: prepared.revision,
                    animation_seconds: f64::from(prepared.motion.source_animation_seconds),
                    style: prepared.motion.style.clone(),
                    motion: prepared.motion.chain.clone(),
                });
                match self.simulation.input().try_submit(command) {
                    Ok(()) => p.phase = Phase::Submitted,
                    Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("recall owner channel closed".into());
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
