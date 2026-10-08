//! Inventory request/receipt lane; immutable proposals remain owned until adoption.
mod corpse_decay;
mod equipment;
mod live;
mod requirements;
use super::*;
use crate::{
    InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryOperation,
    InventoryOutcome, InventoryPreparedRequest,
};
use bace_gameplay_api::{GeneratorLocation, InventoryRejection as E, InventoryRequest};
use bace_inventory::ItemPlace;
use bace_motion::MotionToken;
use std::collections::{BTreeMap, VecDeque};
struct EquipmentStance {
    actor: EntityId,
    operation: u64,
    token: MotionToken,
    epoch: u16,
    deadline: u64,
}
pub(super) struct InventoryCommands {
    pending: BTreeMap<u64, InventoryOperation>,
    equipment: BTreeMap<u64, std::sync::Arc<crate::PreparedEquipmentPhysical>>,
    equipment_stances: BTreeMap<u64, EquipmentStance>,
    live: BTreeMap<u64, live::Live>,
    outcomes: VecDeque<InventoryOutcome>,
    capacity: usize,
}
impl InventoryCommands {
    pub(super) fn owns(&self, operation: u64) -> bool {
        self.pending.contains_key(&operation)
    }
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            pending: BTreeMap::new(),
            equipment: BTreeMap::new(),
            equipment_stances: BTreeMap::new(),
            live: BTreeMap::new(),
            outcomes: VecDeque::new(),
            capacity: capacity.min(4096),
        }
    }
}
impl Kernel {
    pub fn inventory_commands_backpressured(&self) -> bool {
        self.inventory_commands.outcomes.len() >= self.inventory_commands.capacity
    }
    pub fn peek_inventory_outcome(&self) -> Option<&InventoryOutcome> {
        self.inventory_commands.outcomes.front()
    }
    pub fn take_inventory_outcome(&mut self) -> Option<InventoryOutcome> {
        self.inventory_commands.outcomes.pop_front()
    }
    pub fn restore_inventory_outcome(
        &mut self,
        outcome: InventoryOutcome,
    ) -> Result<(), Box<InventoryOutcome>> {
        if self.inventory_commands_backpressured() {
            return Err(Box::new(outcome));
        }
        self.inventory_commands.outcomes.push_front(outcome);
        Ok(())
    }
    pub fn has_inventory_command_state(&self) -> bool {
        !self.inventory_commands.live.is_empty()
            || !self.inventory_commands.equipment_stances.is_empty()
            || !self.inventory_commands.pending.is_empty()
            || !self.inventory_commands.outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::Inventory(_)))
    }
    pub(super) fn handle_inventory_command(
        &mut self,
        command: InventoryCommand,
    ) -> Result<(), WorldError> {
        let correlation = command.correlation;
        let result = if !command.valid_bounds() {
            Err(E::InvalidState)
        } else {
            self.apply_inventory_command(correlation, command.kind)
        };
        self.inventory_commands
            .outcomes
            .push_back(InventoryOutcome {
                correlation,
                result,
            });
        Ok(())
    }
    fn apply_inventory_command(
        &mut self,
        correlation: u64,
        command: InventoryCommandKind,
    ) -> Result<InventoryDecision, E> {
        match command {
            InventoryCommandKind::ProposeEquipment(p) => {
                self.prepare_inventory_equipment(correlation, *p)
            }
            InventoryCommandKind::PrepareEquipmentPhysical {
                operation,
                prepared,
            } => self
                .prepare_inventory_equipment_physical(operation, *prepared)
                .map(|p| InventoryDecision::EquipmentPrepared(Box::new(p))),
            InventoryCommandKind::InspectLive { context, request } => self
                .inspect_inventory_live(context, request)
                .map(|e| InventoryDecision::Inspected(Box::new(e))),
            InventoryCommandKind::ProposeLive(prepared) => {
                self.start_inventory_live(correlation, prepared)
            }
            InventoryCommandKind::ProposeOwned(mut request) => {
                let (source, destination) = match request.request {
                    InventoryRequest::Move {
                        item, container, ..
                    }
                    | InventoryRequest::SplitToContainer {
                        item, container, ..
                    } => (item, container),
                    InventoryRequest::Merge { source, target, .. } => (source, target),
                    _ => return Err(E::InvalidState),
                };
                if request.drop.is_some() {
                    return Err(E::InvalidState);
                }
                self.validate_owned_inventory_path(request.context.actor, source)?;
                self.validate_owned_inventory_path(request.context.actor, destination)?;
                if self.recall_busy(request.context.actor)
                    || self.magic.busy(request.context.actor)
                    || self.world.motion_busy(request.context.actor)
                    || self.portals.reserved(request.context.actor)
                    || self.world.is_in_portal_transit(request.context.actor)
                {
                    return Err(E::Busy);
                }
                // Entirely carried, unequipped graphs require no world reach/path query.
                request.authority.in_range = true;
                request.authority.clear_path = true;
                request.authority.geometry_ready = true;
                request.authority.drop_validated = false;
                request.authority.source_view = None;
                request.authority.destination_view = None;
                self.prepare_inventory_operation(*request)
                    .map(|p| InventoryDecision::Proposed(Box::new(p)))
            }
            InventoryCommandKind::Propose(request) => self
                .prepare_inventory_operation(*request)
                .map(|p| InventoryDecision::Proposed(Box::new(p))),
            InventoryCommandKind::Commit(receipt) => {
                let operation = self
                    .inventory_commands
                    .pending
                    .get(&receipt.operation)
                    .ok_or(E::InvalidState)?
                    .clone();
                let removed: Vec<_> = operation
                    .ticket
                    .proposal
                    .changes
                    .iter()
                    .filter(|c| {
                        c.before
                            .as_ref()
                            .is_some_and(|b| b.place == ItemPlace::World)
                            && c.after.place != ItemPlace::World
                    })
                    .map(|c| c.after.id)
                    .collect();
                if removed.iter().any(|id| self.world.body(*id).is_err()) {
                    return Err(E::MissingGeometry);
                }
                let mut expected = self.generated_inventory_items(receipt.operation)?;
                expected.sort_unstable();
                let mut transient = operation.transient.clone();
                transient.sort_unstable();
                if expected != transient {
                    return Err(E::InvalidState);
                }
                self.validate_inventory_corpse_decay(&operation.corpse_decay)?;
                let ticket = self.confirm_inventory_committed_inner(&receipt)?;
                self.adopt_inventory_corpse_decay(&operation.corpse_decay);
                if !transient.is_empty() {
                    self.inventory.adopt_generated_durability(&transient);
                }
                for id in removed {
                    self.world.remove(id);
                }
                self.inventory_commands.pending.remove(&receipt.operation);
                Ok(InventoryDecision::Committed(ticket))
            }
            InventoryCommandKind::Reject { operation } => {
                if !self.inventory_commands.pending.contains_key(&operation) {
                    return Err(E::InvalidState);
                }
                self.reject_inventory_inner(operation)?;
                self.inventory_commands.pending.remove(&operation);
                if let Some(correlation) =
                    self.inventory_commands.equipment_stances.iter().find_map(
                        |(correlation, hold)| (hold.operation == operation).then_some(*correlation),
                    )
                    && let Some(hold) = self
                        .inventory_commands
                        .equipment_stances
                        .remove(&correlation)
                    && self.world.source_motion_token(hold.actor) == Some(hold.token)
                {
                    let _ = self.world.cancel_motion(hold.actor, hold.token);
                }
                Ok(InventoryDecision::Rejected { operation })
            }
        }
    }
    fn validate_owned_inventory_path(&self, actor: EntityId, mut item: EntityId) -> Result<(), E> {
        for _ in 0..1024 {
            if item == actor {
                return self
                    .inventory
                    .container(actor)
                    .filter(|c| c.root_owner == Some(actor))
                    .map(|_| ())
                    .ok_or(E::OwnershipMismatch);
            }
            match self.inventory.item(item).ok_or(E::OwnershipMismatch)?.place {
                ItemPlace::Contained {
                    container,
                    equipped: 0,
                    ..
                } => item = container,
                _ => return Err(E::OwnershipMismatch),
            }
        }
        Err(E::InvalidState)
    }
    fn prepare_inventory_operation(
        &mut self,
        input: InventoryPreparedRequest,
    ) -> Result<InventoryOperation, E> {
        if self.inventory_commands.pending.len() >= self.inventory_commands.capacity {
            return Err(E::Capacity);
        }
        let binding = CharacterBinding {
            session: input.context.session,
            account: input.context.account,
            actor: input.context.actor,
        };
        let operation = match (input.split, input.drop) {
            (Some(split), Some(drop)) => self.propose_stack_split_to_world(
                input.context,
                input.request,
                input.authority,
                split,
                *drop,
            )?,
            (Some(split), None) => {
                self.propose_stack_split(input.context, input.request, input.authority, split)?
            }
            (None, Some(drop)) => {
                self.propose_inventory_drop(input.context, input.request, input.authority, *drop)?
            }
            (None, None) => {
                if matches!(input.request, InventoryRequest::Drop { .. }) {
                    return Err(E::MissingGeometry);
                }
                self.propose_inventory(input.context, input.request, input.authority)?
            }
        };
        let result = self.capture_inventory_operation(binding, operation, input.request);
        match result {
            Ok(value) => {
                self.inventory.claim(operation)?;
                self.inventory_commands
                    .pending
                    .insert(operation, value.clone());
                Ok(value)
            }
            Err(error) => {
                self.reject_inventory(operation)?;
                Err(error)
            }
        }
    }
    fn capture_inventory_operation(
        &mut self,
        binding: CharacterBinding,
        operation: u64,
        request: InventoryRequest,
    ) -> Result<InventoryOperation, E> {
        let corpse_decay = self.prepare_inventory_corpse_decay(operation)?;
        let ticket = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::InvalidState)?
            .clone();
        let actor_revision = self
            .characters
            .get(binding.actor)
            .ok_or(E::NotBound)?
            .revision();
        let transient = self.generated_inventory_items(operation)?;
        let ids: std::collections::BTreeSet<_> = ticket
            .proposal
            .participants
            .iter()
            .map(|(id, _)| *id)
            .chain(ticket.proposal.changes.iter().map(|c| c.after.id))
            .collect();
        if ids.len() > 1024
            || ids
                .iter()
                .filter_map(|id| self.magic.registry(*id))
                .map(|r| r.entries().len())
                .sum::<usize>()
                > 65536
        {
            return Err(E::Capacity);
        }
        let enchantments = ids
            .iter()
            .filter_map(|id| {
                self.magic
                    .registry(*id)
                    .map(|r| (*id, r.entries().to_vec()))
            })
            .collect();
        let mut positions = BTreeMap::new();
        for change in &ticket.proposal.changes {
            if change
                .before
                .as_ref()
                .is_some_and(|b| b.place == ItemPlace::World)
            {
                let (cell, state) = self
                    .world
                    .actor_state(change.after.id)
                    .map_err(|_| E::MissingGeometry)?;
                let p = state.position();
                let half = state.heading_radians() * 0.5;
                positions.insert(
                    change.after.id,
                    GeneratorLocation {
                        cell: cell.0,
                        origin: [p.x, p.y, p.z],
                        rotation: [0., 0., half.sin(), half.cos()],
                    },
                );
            }
        }
        if let Some(p) = self.inventory_world_placement(operation) {
            let half = p.heading * 0.5;
            positions.insert(
                p.item,
                GeneratorLocation {
                    cell: p.cell.0,
                    origin: [p.position.x, p.position.y, p.position.z],
                    rotation: [0., 0., half.sin(), half.cos()],
                },
            );
        }
        Ok(InventoryOperation {
            corpse_decay,
            equipment: None,
            equipment_vitals: None,
            equipment_health_update: false,
            equipment_mode_after: None,
            request,
            binding,
            actor_revision,
            ticket,
            transient,
            positions,
            enchantments,
            world_placement: self.inventory_world_placement(operation),
        })
    }
    pub(super) fn validate_inventory_snapshot(
        &self,
        binding: CharacterBinding,
        operation: u64,
        revision: u64,
    ) -> bool {
        self.inventory_commands
            .pending
            .get(&operation)
            .is_some_and(|p| {
                p.binding == binding
                    && p.actor_revision == revision
                    && self.inventory.pending_ticket(operation) == Some(&p.ticket)
                    && self.inventory.reserved(binding.actor)
            })
    }
}
