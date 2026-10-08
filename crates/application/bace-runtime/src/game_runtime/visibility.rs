//! Complete cold blueprints and accepted owner views meet only in replication.
//! The client knowledge set advances on exact reliable admission receipts.
use super::*;
use crate::{
    visibility_assets::PreparedVisibilityObject,
    visibility_service::{VisibilityLimits, VisibilityService},
};
pub(super) struct VisibilityRuntime {
    pub service: VisibilityService,
    pending_source: Option<(u64, PreparedVisibilityObject)>,
    pub retirements: BTreeMap<bace_types::EntityId, u64>,
}
impl VisibilityRuntime {
    pub(super) fn new(limits: GameRuntimeLimits) -> Result<Self, String> {
        let batch_bytes = limits.message_bytes.min(1024 * 1024);
        let codec = bace_wire::ObjectCodecLimits {
            max_message_bytes: batch_bytes,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 4096,
        };
        let service = VisibilityService::new(VisibilityLimits {
            observers: limits.sessions,
            objects: 65536,
            known_per_observer: 4096,
            batch_bytes,
            retained_bytes: 64 * 1024 * 1024,
            codec,
        })
        .map_err(|error| format!("visibility startup: {error:?}"))?;
        Ok(Self {
            service,
            pending_source: None,
            retirements: BTreeMap::new(),
        })
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending_source.is_some() || !self.retirements.is_empty() || self.service.pending()
    }
}
impl GameRuntime {
    pub(super) fn register_player_visibility(
        &mut self,
        key: SessionKey,
        revision: u64,
        tick: u64,
        entry: &crate::player_entry::PreparedPlayerEntry,
    ) -> Result<(), String> {
        let mut children = Vec::with_capacity(entry.self_object.physics.options.children.len());
        for child in &entry.self_object.physics.options.children {
            let object = entry
                .possessions
                .iter()
                .find_map(|p| match p {
                    crate::player_entry::PreparedEntryPossession::Create(object)
                        if object.object_id == child.object_id =>
                    {
                        Some(object)
                    }
                    _ => None,
                })
                .ok_or("public player attachment source missing")?;
            children.push(Arc::new((**object).clone()));
        }
        self.visibility
            .service
            .register_object(
                key.generation,
                revision
                    .checked_add(1)
                    .ok_or("player blueprint revision overflow")?,
                tick,
                Arc::new(entry.self_object.clone()),
                children,
            )
            .map_err(|error| format!("player visibility blueprint: {error:?}"))
    }
    pub(super) fn poll_visibility(&mut self) -> Result<(), String> {
        if let Some(update) = self.pending_inventory_equipment_visibility().cloned() {
            self.visibility
                .service
                .replace_equipment_blueprint(&update)
                .map_err(|error| format!("equipment visibility handoff retained: {error:?}"))?;
            self.acknowledge_inventory_equipment_visibility(update.operation)?;
        }

        for _ in 0..self.limits.work_per_poll {
            let Some((&actor, &tick)) = self.visibility.retirements.first_key_value() else {
                break;
            };
            self.visibility
                .service
                .retire_object(actor, tick)
                .map_err(|error| format!("visibility retirement retained: {error:?}"))?;
            self.visibility.retirements.remove(&actor);
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(retirement) = self
                .world
                .as_ref()
                .and_then(|world| world.regions.visibility_retirement())
            else {
                break;
            };
            self.visibility
                .service
                .retire_object(retirement.0, retirement.1)
                .map_err(|error| format!("region visibility retirement retained: {error:?}"))?;
            self.world
                .as_mut()
                .expect("retained region owner")
                .regions
                .acknowledge_visibility_retirement(retirement)?;
        }
        for _ in 0..self.limits.work_per_poll {
            if self.visibility.pending_source.is_none() {
                self.visibility.pending_source = self
                    .world
                    .as_mut()
                    .and_then(|world| world.regions.take_visibility_source());
            }
            let Some((tick, source)) = &self.visibility.pending_source else {
                break;
            };
            source
                .register(&mut self.visibility.service, *tick)
                .map_err(|error| format!("region visibility blueprint retained: {error:?}"))?;
            self.visibility.pending_source = None;
        }
        for _ in 0..self.limits.work_per_poll {
            let Some(index) = self
                .reliable_admissions
                .iter()
                .position(|(_, id, _)| id & 0xff00_0000_0000_0000 == 0x5600_0000_0000_0000)
            else {
                break;
            };
            let (key, correlation, accepted) = self.reliable_admissions[index];
            match self
                .visibility
                .service
                .reliable_admission(key, correlation, accepted)
            {
                Ok(true) => {
                    self.reliable_admissions.remove(index);
                }
                Ok(false)
                    if self
                        .sessions
                        .get(&key)
                        .is_none_or(|session| session.terminated || session.closing) =>
                {
                    self.reliable_admissions.remove(index);
                }
                Ok(false) => return Err("unmatched visibility receipt retained".into()),
                Err(error) => {
                    // Failed admission closes that generation; its prior client
                    // knowledge remains owned until the lifecycle unbinds it.
                    if let Some(session) = self.sessions.get_mut(&key) {
                        session.terminated = true;
                    }
                    self.reliable_admissions.remove(index);
                    return Err(format!("visibility reliable admission rejected: {error:?}"));
                }
            }
        }
        // Generic snapshots share the canonical counters. Do not publish a
        // newer snapshot ahead of a retained private batch or its observer
        // effects. Receipt draining above remains active under this fence.
        if !self.network_output.is_empty()
            || self.portals.publication_pending()
            || self.portals.output_pending()
            || self.recalls.output_pending()
            || self.pending_reward_observers().is_some()
            || self.observer_output.has_pending()
        {
            return self
                .visibility
                .service
                .poll_pending(&self.simulation, &mut self.players, &self.network)
                .map_err(|error| format!("fenced visibility output retained: {error:?}"));
        }
        let result = self
            .visibility
            .service
            .poll(&self.simulation, &mut self.players, &self.network)
            .map_err(|error| format!("visibility output retained: {error:?}"));
        for (key, _) in self.visibility.service.failed_sessions() {
            if let Some(session) = self.sessions.get_mut(&key) {
                session.terminated = true;
            }
        }
        result
    }
}
