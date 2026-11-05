//! One retained valuable inventory action across cold preparation, owner proposal,
//! immutable save, owner receipt and canonical reliable output admission.
mod cold;
mod equipment;
mod equipment_output;
pub(super) mod output;
mod physical;
mod registration;
#[cfg(test)]
mod tests;
use super::*;
use crate::inventory_service::{InventoryCompletion, InventoryService, InventoryWork};
use bace_gameplay_api::{ActionContext, CharacterBinding, InventoryRejection, InventoryRequest};
use bace_simulation::{
    Command, InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryOutcome,
    InventoryPreparedRequest,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use std::sync::mpsc::TrySendError;

/// Immutable avatar DAT preparation depends only on these authored references,
/// scale and the accepted content generation. Retaining one decoded closure
/// bounds the extra heap cost to one avatar, rather than one copy per player.
/// The generation revision and portal digest prevent reuse after publication.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AvatarKey {
    generation: u64,
    portal_sha256: String,
    setup: u32,
    motions: u32,
    maneuvers: u32,
    scale_bits: u32,
}
impl AvatarKey {
    fn from_source(
        source: &bace_content::WeenieV1,
        generation: u64,
        portal_sha256: &str,
    ) -> Result<Self, String> {
        let did = |id| {
            source
                .properties
                .data_ids
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.value)
                .ok_or("missing avatar DAT reference".to_owned())
        };
        let scale = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1.0, |p| p.value) as f32;
        Ok(Self {
            generation,
            portal_sha256: portal_sha256.to_owned(),
            setup: did(1)?,
            motions: did(2)?,
            maneuvers: did(4)?,
            scale_bits: scale.to_bits(),
        })
    }
}

pub(super) enum InventoryIngress {
    Accepted,
    Blocked,
    Unsupported,
}
struct Pending {
    key: SessionKey,
    item: EntityId,
    rejected: bool,
    binding: CharacterBinding,
    correlation: u64,
    request: Option<Box<InventoryCommand>>,
    prepared: Option<cold::Prepared>,
    saving: bool,
    sources_registered: bool,
    inspection: Option<Box<bace_simulation::InventoryInspection>>,
    motions: VecDeque<bace_simulation::InventoryMotion>,
    completion: Option<InventoryCompletion>,
}
pub(super) struct InventoryRuntime {
    pub(super) service: InventoryService,
    pending: Option<Pending>,
    cold: Option<Job<Result<cold::Prepared, String>>>,
    unexpected: Option<Box<InventoryOutcome>>,
    unexpected_completion: Option<InventoryCompletion>,
    failures: BTreeMap<SessionKey, String>,
    equipment: Option<equipment::EquipmentRuntime>,
    avatar_dat: Option<(AvatarKey, Arc<crate::player_assets::PreparedAvatarDat>)>,
}
impl InventoryRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: InventoryService::new(),
            pending: None,
            cold: None,
            unexpected: None,
            unexpected_completion: None,
            failures: BTreeMap::new(),
            equipment: None,
            avatar_dat: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.cold.is_some()
            || self.service.requires_drain()
            || self.unexpected.is_some()
            || self.unexpected_completion.is_some()
    }
    #[cfg(test)]
    pub(super) fn cached_avatar_dat(
        &self,
    ) -> Option<&Arc<crate::player_assets::PreparedAvatarDat>> {
        self.avatar_dat.as_ref().map(|(_, dat)| dat)
    }
}
impl GameRuntime {
    pub fn inventory_failure(&self, key: SessionKey) -> Option<&str> {
        self.inventory
            .failures
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.inventory_ingress_blocked(key)
                    .then(|| self.inventory.service.blocked())
                    .flatten()
            })
    }
    pub(super) fn inventory_ingress_blocked(&self, key: SessionKey) -> bool {
        self.inventory
            .pending
            .as_ref()
            .is_some_and(|p| p.key == key)
    }
    pub(super) fn retry_inventory(&mut self, key: SessionKey) -> bool {
        if self.inventory_ingress_blocked(key)
            && self
                .inventory
                .equipment
                .as_mut()
                .is_some_and(|p| p.failure.take().is_some())
        {
            self.inventory.failures.remove(&key);
            true
        } else if self.inventory_ingress_blocked(key) && self.inventory.service.blocked().is_some()
        {
            self.inventory.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn forget_inventory_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.inventory_ingress_blocked(key) {
            return Err("inventory remains pending during logout".into());
        }
        self.inventory.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_inventory_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<InventoryIngress, String> {
        let Some(opcode) = message.bytes.get(..4) else {
            return Ok(InventoryIngress::Unsupported);
        };
        if u32::from_le_bytes(opcode.try_into().expect("four bytes"))
            != bace_wire::opcode::GameMessageOpcode::GameAction.0
        {
            return Ok(InventoryIngress::Unsupported);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(InventoryIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(InventoryIngress::Unsupported);
        };
        let binding = loading.loaded.binding;
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let decoded = match bace_session::decode_inventory(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(bace_session::DispatchError::UnsupportedAction(_)) => {
                return Ok(InventoryIngress::Unsupported);
            }
            Err(error) => return Err(format!("malformed inventory request: {error:?}")),
        };
        // The GameAction payload counter is distinct from the ordered message
        // sequence. ACE reads it but does not use it to authorize actions.
        // Characters has one cursor shared with other gameplay actions, whose
        // contexts use the ordered message sequence. Mixing the two counters
        // rejects valid retail inventory requests after unrelated actions.
        if self.inventory.has_pending() {
            return Ok(InventoryIngress::Blocked);
        }
        if !self.players.entered(binding.actor) || session.terminated || session.disconnected {
            return Ok(InventoryIngress::Blocked);
        }
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("inventory authenticated binding mismatch".into());
        }
        let equipped_source = self.online_saves.inventory_baseline(binding.actor.0,item_id(decoded.request).0).is_some_and(|i|matches!(i.placement,bace_storage_codec::ItemPlacementV2::Contained{equipped,..}if equipped!=0));
        if matches!(
            decoded.request,
            InventoryRequest::Equip { .. } | InventoryRequest::SplitToWield { .. }
        ) || equipped_source && matches!(decoded.request, InventoryRequest::Move { .. })
        {
            self.diagnostic(
                "info",
                "game.equip_request",
                &format!(
                    "session {} generation {}: {:?}",
                    key.id, key.generation, decoded.request
                ),
            );
            self.begin_inventory_equipment(key, binding, context, decoded.request)
                .map_err(|error| {
                    self.diagnostic(
                        "error",
                        "game.equip_failure",
                        &format!("session {} generation {}: {error}", key.id, key.generation),
                    );
                    error
                })?;
            return Ok(InventoryIngress::Accepted);
        }
        let source = cold::owned_source(binding.actor, decoded.request, |id| {
            self.online_saves
                .inventory_baseline(binding.actor.0, id)
                .map(|row| (&row.entity.state, &row.placement))
        });
        if let InventoryRequest::Move {
            item,
            container,
            placement,
        } = decoded.request
        {
            self.diagnostic(
                "info",
                "game.inventory_move_request",
                &format!(
                    "session {} generation {}: item {:#010x} to container {:#010x} slot {placement}, route {}",
                    key.id,
                    key.generation,
                    item.0,
                    container.0,
                    if source.is_ok() { "carried" } else { "inspection" }
                ),
            );
        }
        if matches!(
            decoded.request,
            InventoryRequest::SplitToContainer { .. } | InventoryRequest::SplitToWorld { .. }
        ) && self.online_saves.inventory_count(binding.actor.0) >= 1023
        {
            self.reject_inventory_ingress(
                key,
                binding,
                decoded.request,
                "player inventory snapshot capacity".into(),
            );
            return Ok(InventoryIngress::Accepted);
        }
        if source.is_err()
            || matches!(
                decoded.request,
                InventoryRequest::Drop { .. } | InventoryRequest::SplitToWorld { .. }
            )
        {
            let correlation = self.token()?;
            self.inventory.pending = Some(Pending {
                key,
                item: item_id(decoded.request),
                rejected: false,
                binding,
                correlation,
                request: Some(Box::new(InventoryCommand {
                    correlation,
                    kind: InventoryCommandKind::InspectLive {
                        context,
                        request: decoded.request,
                    },
                })),
                prepared: None,
                saving: false,
                sources_registered: false,
                inspection: None,
                motions: VecDeque::new(),
                completion: None,
            });
            return Ok(InventoryIngress::Accepted);
        }
        let source = source.expect("owned source checked");
        let correlation = self.token()?;
        let authority = cold::authority(binding.actor);
        let request = InventoryPreparedRequest {
            context,
            request: decoded.request,
            authority,
            split: None,
            drop: None,
        };
        let split = matches!(decoded.request, InventoryRequest::SplitToContainer { .. });
        let prepared = (!split).then(cold::Prepared::ordinary);
        let command = (!split).then(|| {
            Box::new(InventoryCommand {
                correlation,
                kind: InventoryCommandKind::ProposeOwned(Box::new(request)),
            })
        });
        if split {
            let store = self.bootstrap.store.clone();
            let generation = self.bootstrap.pack.generation.clone();
            let manifest = self.bootstrap.assets.clone();
            self.inventory.cold = Some(Box::pin(cold::prepare_split(
                store,
                generation,
                manifest,
                context,
                decoded.request,
                source,
            )));
        }
        self.inventory.failures.remove(&key);
        self.inventory.pending = Some(Pending {
            key,
            item: item_id(decoded.request),
            rejected: false,
            binding,
            correlation,
            request: command,
            prepared,
            saving: false,
            sources_registered: false,
            inspection: None,
            motions: VecDeque::new(),
            completion: None,
        });
        Ok(InventoryIngress::Accepted)
    }
    fn reject_inventory_ingress(
        &mut self,
        key: SessionKey,
        binding: CharacterBinding,
        request: InventoryRequest,
        reason: String,
    ) {
        self.inventory.failures.insert(key, reason);
        self.inventory.pending = Some(Pending {
            key,
            item: item_id(request),
            rejected: true,
            binding,
            correlation: 0,
            request: None,
            prepared: None,
            saving: false,
            sources_registered: false,
            inspection: None,
            motions: VecDeque::new(),
            completion: None,
        });
    }
    pub(super) fn poll_inventory(&mut self) -> Result<(), String> {
        self.poll_inventory_equipment()?;
        if self.inventory.unexpected.is_some() || self.inventory.unexpected_completion.is_some() {
            return Err("uncorrelated inventory outcome retained".into());
        }
        if let Some(done) = ready(&mut self.inventory.cold) {
            let pending = self
                .inventory
                .pending
                .as_mut()
                .ok_or("inventory cold completion has no owner")?;
            match done {
                Ok(mut prepared) => {
                    if let Some(dat) = prepared.equipment_dat.as_ref()
                        && let Some(key) = self
                            .inventory
                            .equipment
                            .as_ref()
                            .and_then(equipment::EquipmentRuntime::avatar_key)
                    {
                        self.inventory.avatar_dat = Some((key.clone(), dat.clone()));
                    }
                    if let Some(fresh) = prepared.fresh.as_ref() {
                        pending.item = fresh.item.id;
                    }
                    let request = prepared
                        .request
                        .take()
                        .ok_or("cold split request missing")?;
                    pending.request = Some(Box::new(InventoryCommand {
                        correlation: pending.correlation,
                        kind: request,
                    }));
                    pending.prepared = Some(prepared);
                }
                Err(error) => {
                    self.inventory.failures.insert(pending.key, error);
                    pending.rejected = true;
                }
            }
        }
        for _ in 0..self.limits.work_per_poll {
            if self
                .inventory
                .pending
                .as_ref()
                .is_some_and(|p| p.motions.len() >= 8)
            {
                break;
            }
            let Ok(outcome) = self.simulation.inventory_outcomes().try_recv() else {
                break;
            };
            let outcome = match self.inventory.service.accept_outcome(outcome) {
                Ok(()) => continue,
                Err(value) => *value,
            };
            let retry_token = if matches!(outcome.result, Err(InventoryRejection::StaleSnapshot)) {
                Some(self.token()?)
            } else {
                None
            };
            let Some(pending) = self
                .inventory
                .pending
                .as_mut()
                .filter(|p| p.correlation == outcome.correlation && p.request.is_none())
            else {
                self.inventory.unexpected = Some(Box::new(outcome));
                return Err("uncorrelated inventory proposal retained".into());
            };
            match outcome.result {
                Ok(InventoryDecision::Inspected(evidence)) => {
                    pending.inspection = Some(evidence);
                }
                Ok(InventoryDecision::MotionStarted) => {}
                Ok(InventoryDecision::Motion(motion)) => {
                    pending.motions.push_back(motion);
                }
                Ok(InventoryDecision::Proposed(operation)) => {
                    let prepared = pending
                        .prepared
                        .as_mut()
                        .ok_or("inventory proposal missing cold inputs")?;
                    let work = InventoryWork {
                        binding: pending.binding,
                        operation: *operation,
                        fresh: prepared.fresh.take(),
                        external: std::mem::take(&mut prepared.external),
                        storage_views: vec![],
                        world_epoch: self.bootstrap.world_owner.epoch(),
                    };
                    if let Err(work) = self.inventory.service.stage(work) {
                        // Never lose a successfully reserved proposal on adapter failure.
                        prepared.fresh = work.fresh;
                        prepared.external = work.external;
                        self.inventory.unexpected = Some(Box::new(InventoryOutcome {
                            correlation: outcome.correlation,
                            result: Ok(InventoryDecision::Proposed(Box::new(work.operation))),
                        }));
                        return Err(
                            "inventory service staging rejected; exact proposal retained".into(),
                        );
                    }
                    pending.saving = true;
                }
                Err(error) => {
                    if error == InventoryRejection::StaleSnapshot
                        && let Some(attempt) = self.inventory.equipment.as_mut().and_then(|gear| {
                            gear.restart_stale(pending, retry_token.expect("stale token"))
                        })
                    {
                        let detail = format!(
                            "session {} generation {} item {:#010x}: recapturing accepted player revision (attempt {attempt}/2)",
                            pending.key.id, pending.key.generation, pending.item.0
                        );
                        self.diagnostic("info", "game.equipment_snapshot_stale", &detail);
                        continue;
                    }
                    self.inventory
                        .failures
                        .insert(pending.key, format!("inventory rejected: {error:?}"));
                    pending.rejected = true;
                }
                Ok(result) => {
                    self.inventory.unexpected = Some(Box::new(InventoryOutcome {
                        correlation: outcome.correlation,
                        result: Ok(result),
                    }));
                    return Err("unexpected inventory proposal response retained".into());
                }
            }
        }
        if self.inventory.cold.is_none()
            && self
                .inventory
                .pending
                .as_ref()
                .is_some_and(|p| p.inspection.is_some())
            && self.world.is_some()
        {
            let p = self
                .inventory
                .pending
                .as_ref()
                .expect("inspected operation");
            let actor = self
                .online_saves
                .baseline(p.binding.actor.0)
                .ok_or("inventory actor baseline missing")?
                .0
                .player
                .entity
                .state
                .clone();
            let evidence = p.inspection.as_ref().expect("inspected operation");
            let mut metadata = BTreeMap::new();
            let mut owned = std::collections::BTreeSet::new();
            for item in &evidence.rows {
                if let Some(saved) = self
                    .online_saves
                    .inventory_baseline(p.binding.actor.0, item.id.0)
                {
                    owned.insert(item.id.0);
                    metadata.insert(
                        item.id.0,
                        crate::game_inventory::FrozenInventoryItem {
                            corpse: None,
                            construction: saved.construction.clone(),
                            source_destination: self
                                .online_saves
                                .inventory_source_destination(p.binding.actor.0, item.id.0),
                            entity: saved.entity.clone(),
                            placement: Some(saved.placement.clone()),
                            persisted_version: 0,
                            enchantments: saved.enchantments.clone(),
                        },
                    );
                } else if let Some(source) = self
                    .world
                    .as_ref()
                    .expect("world retained")
                    .regions
                    .transient_source(item.id)
                {
                    metadata.insert(item.id.0, source.item.clone());
                }
            }
            let extra = evidence
                .rows
                .iter()
                .filter(|item| !owned.contains(&item.id.0))
                .count()
                + usize::from(matches!(
                    evidence.request,
                    InventoryRequest::SplitToContainer { .. }
                        | InventoryRequest::SplitToWorld { .. }
                        | InventoryRequest::SplitToWield { .. }
                ));
            if self
                .online_saves
                .inventory_count(p.binding.actor.0)
                .saturating_add(extra)
                > 1023
            {
                let key = p.key;
                let pending = self
                    .inventory
                    .pending
                    .as_mut()
                    .expect("inspected operation");
                pending.inspection = None;
                pending.rejected = true;
                self.inventory
                    .failures
                    .insert(key, "inventory acquisition capacity".into());
                return self.project_inventory();
            }
            let evidence = *self
                .inventory
                .pending
                .as_mut()
                .expect("inspected operation")
                .inspection
                .take()
                .expect("inspected operation");
            self.inventory.cold = Some(Box::pin(physical::prepare(
                self.bootstrap.store.clone(),
                self.bootstrap.pack.generation.clone(),
                self.bootstrap.assets.clone(),
                actor,
                evidence,
                metadata,
                owned,
            )));
        }
        if let Some(pending) = &mut self.inventory.pending
            && let Some(command) = pending.request.take()
        {
            match self
                .simulation
                .input()
                .try_submit(Command::Inventory(command))
            {
                Ok(()) => {}
                Err(TrySendError::Full(Command::Inventory(command))) => {
                    pending.request = Some(command)
                }
                Err(TrySendError::Disconnected(Command::Inventory(command))) => {
                    pending.request = Some(command);
                    return Err("inventory owner ingress closed".into());
                }
                _ => unreachable!("typed inventory command"),
            }
        }
        let correlation = self.token()?;
        let durable = self.inventory.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        );
        if let Some(completion) = self.inventory.service.take_completion() {
            let Some(pending) = self.inventory.pending.as_mut().filter(|p| {
                p.binding == completion.work.binding && p.saving && p.completion.is_none()
            }) else {
                self.inventory.unexpected_completion = Some(completion);
                return Err(
                    "inventory completion ownership mismatch; exact result retained".into(),
                );
            };
            pending.completion = Some(completion);
        }
        if self.register_inventory_world_sources()? {
            self.project_inventory()?;
        }
        durable
    }
}

fn item_id(request: InventoryRequest) -> EntityId {
    match request {
        InventoryRequest::Move { item, .. }
        | InventoryRequest::Equip { item, .. }
        | InventoryRequest::Drop { item }
        | InventoryRequest::SplitToContainer { item, .. }
        | InventoryRequest::SplitToWorld { item, .. }
        | InventoryRequest::SplitToWield { item, .. } => item,
        InventoryRequest::Merge { source, .. } => source,
    }
}
