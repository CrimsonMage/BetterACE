//! Owner-side physical half of a durable stack split. Pending bodies are not
//! accepted world actors and remain reserved until exact completion or rejection.
use super::*;
use bace_gameplay_api::InventoryRejection as Error;
use std::collections::{BTreeMap, VecDeque};

pub struct PreparedStackDrop {
    /// Cold content geometry and a server-selected drop pose, never client input.
    pub spawn: bace_physics::GeometrySpawn,
    pub source_epoch: u16,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackWorldPlacement {
    pub operation: u64,
    pub item: EntityId,
    pub cell: bace_types::CellId,
    pub position: bace_geometry::Vec3,
    pub heading: f32,
    pub grounded: bool,
    pub velocity: bace_geometry::Vec3,
    pub epoch: u16,
}
pub(super) struct InventoryPlacements {
    pending: BTreeMap<u64, bace_entity::Actor>,
    events: VecDeque<StackWorldPlacement>,
    capacity: usize,
}
impl InventoryPlacements {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            events: VecDeque::new(),
            capacity: capacity.min(4096),
        }
    }
    fn placement(operation: u64, actor: &bace_entity::Actor) -> StackWorldPlacement {
        let state = actor.body.accepted();
        StackWorldPlacement {
            operation,
            item: actor.id,
            cell: actor.cell,
            position: state.position(),
            heading: state.heading_radians(),
            grounded: state.grounded(),
            velocity: state.velocity(),
            epoch: state.epoch(),
        }
    }
}
impl Kernel {
    /// Prepare geometry before reserving inventory. The exact prepared position
    /// must accompany the durable inventory freeze; a boolean alone is insufficient.
    pub fn propose_stack_split_to_world(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::InventoryRequest,
        mut authority: bace_inventory::InventoryAuthority,
        prepared: bace_inventory::StackSplitPreparation,
        drop: PreparedStackDrop,
    ) -> Result<u64, Error> {
        if !matches!(
            request,
            bace_gameplay_api::InventoryRequest::SplitToWorld { .. }
        ) {
            return Err(Error::InvalidState);
        }
        if self.inventory_placements.pending.len() + self.inventory_placements.events.len()
            >= self.inventory_placements.capacity
        {
            return Err(Error::Capacity);
        }
        let (_, state) = self
            .world
            .actor_state(context.actor)
            .map_err(|_| Error::NotBound)?;
        if state.epoch() != drop.source_epoch {
            return Err(Error::InvalidState);
        }
        let id = prepared.fresh.id;
        self.validate_split_identity(id)?;
        let cell = bace_types::CellId(drop.spawn.cell);
        let body = self
            .world
            .prepare_geometry_body(drop.spawn)
            .map_err(|_| Error::MissingGeometry)?;
        let actor = bace_entity::Actor { id, cell, body };
        self.world
            .validate_actor(&actor)
            .map_err(|_| Error::MissingGeometry)?;
        authority.geometry_ready = true;
        authority.drop_validated = true;
        self.authorize_inventory(context, authority)?;
        let operation = self.stage_inventory(context.actor, |inventory| {
            inventory.split(request, authority, prepared)
        })?;
        self.inventory_placements.pending.insert(operation, actor);
        Ok(operation)
    }
    pub fn inventory_world_placement(&self, operation: u64) -> Option<StackWorldPlacement> {
        self.inventory_placements
            .pending
            .get(&operation)
            .map(|actor| InventoryPlacements::placement(operation, actor))
    }
    pub fn take_stack_world_placement(&mut self) -> Option<StackWorldPlacement> {
        self.inventory_placements.events.pop_front()
    }
    pub(super) fn validate_split_identity(&self, id: EntityId) -> Result<(), Error> {
        if self.population.reserves_identity(id)
            || self.generator_reserves_identity(id)
            || self.magic.reserves_identity(id)
            || self.world.contains_identity(id)
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    pub(super) fn preflight_inventory_placement(&self, operation: u64) -> Result<(), Error> {
        if let Some(actor) = self.inventory_placements.pending.get(&operation) {
            if self.inventory_placements.events.len() >= self.inventory_placements.capacity {
                return Err(Error::Capacity);
            }
            self.world
                .validate_actor(actor)
                .map_err(|_| Error::MissingGeometry)?;
            let ticket = self
                .inventory
                .pending_ticket(operation)
                .ok_or(Error::InvalidState)?;
            if !ticket.proposal.changes.iter().any(|change| {
                change
                    .before
                    .as_ref()
                    .is_none_or(|before| before.place != bace_inventory::ItemPlace::World)
                    && change.after.id == actor.id
                    && change.after.place == bace_inventory::ItemPlace::World
            }) {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }
    pub(super) fn finish_inventory_placement(&mut self, operation: u64, committed: bool) {
        if let Some(actor) = self.inventory_placements.pending.remove(&operation)
            && committed
        {
            let event = InventoryPlacements::placement(operation, &actor);
            self.world
                .insert(actor)
                .expect("single-owner preflighted stack placement");
            self.inventory_placements.events.push_back(event);
        }
    }
}

impl Kernel {
    pub(super) fn propose_inventory_drop(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::InventoryRequest,
        mut authority: bace_inventory::InventoryAuthority,
        drop: PreparedStackDrop,
    ) -> Result<u64, Error> {
        let bace_gameplay_api::InventoryRequest::Drop { item } = request else {
            return Err(Error::InvalidState);
        };
        if self.inventory_placements.pending.len() + self.inventory_placements.events.len()
            >= self.inventory_placements.capacity
        {
            return Err(Error::Capacity);
        }
        let (_, state) = self
            .world
            .actor_state(context.actor)
            .map_err(|_| Error::NotBound)?;
        if state.epoch() != drop.source_epoch || self.world.contains_identity(item) {
            return Err(Error::InvalidState);
        }
        let cell = bace_types::CellId(drop.spawn.cell);
        let body = self
            .world
            .prepare_geometry_body(drop.spawn)
            .map_err(|_| Error::MissingGeometry)?;
        let actor = bace_entity::Actor {
            id: item,
            cell,
            body,
        };
        self.world
            .validate_actor(&actor)
            .map_err(|_| Error::MissingGeometry)?;
        authority.geometry_ready = true;
        authority.drop_validated = true;
        let operation = self.propose_inventory(context, request, authority)?;
        self.inventory_placements.pending.insert(operation, actor);
        Ok(operation)
    }
}
