//! Kernel-owned inventory graph metadata. Physical state remains in World.
//! Proposals reserve identities and never mutate accepted ownership before a
//! correlated durable receipt confirms every proposed item revision.
pub(crate) mod admission;
mod burden;
mod constructed_creatures;
mod corpse_decay;
mod equipment;
mod equipment_operation;
mod generated;
mod npc_snapshot;
mod player_death;
pub(crate) mod region_admission;
mod region_unload;
use bace_gameplay_api::{InventoryRejection as Error, InventoryRequest};
use bace_inventory::{
    InventoryAuthority, InventoryContainer, InventoryItem, InventoryProposal, InventoryView,
    ItemPlace, propose_grant, propose_inventory, propose_take,
};
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryTicket {
    pub operation: u64,
    pub actor: EntityId,
    pub proposal: InventoryProposal,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryReceipt {
    pub operation: u64,
    pub revisions: Vec<(EntityId, u64)>,
}
#[derive(Clone)]
struct Pending {
    ticket: InventoryTicket,
    submitted: bool,
}
#[derive(Clone)]
pub(crate) struct Inventory {
    constructed: std::collections::BTreeSet<EntityId>,
    transient: std::collections::BTreeSet<EntityId>,
    items: BTreeMap<EntityId, InventoryItem>,
    equipped: BTreeMap<EntityId, std::collections::BTreeSet<EntityId>>,
    burden: BTreeMap<EntityId, u64>,
    burden_dirty: std::collections::BTreeSet<EntityId>,
    containers: BTreeMap<EntityId, InventoryContainer>,
    pending: BTreeMap<u64, Pending>,
    reserved: BTreeMap<EntityId, u64>,
    death_holds: BTreeMap<EntityId, u64>,
    region_holds: BTreeMap<EntityId, u64>,
    npc_holds: BTreeMap<EntityId, (EntityId, u64)>,
    outbox: VecDeque<u64>,
    capacity: usize,
    next: u64,
}
impl Inventory {
    pub(crate) fn items(&self) -> impl Iterator<Item = &InventoryItem> {
        self.items.values()
    }
    pub(crate) fn containers(&self) -> impl Iterator<Item = &InventoryContainer> {
        self.containers.values()
    }
    pub(crate) fn pending_ticket(&self, operation: u64) -> Option<&InventoryTicket> {
        self.pending.get(&operation).map(|pending| &pending.ticket)
    }
    pub(crate) fn claim(&mut self, operation: u64) -> Result<(), Error> {
        let pending = self
            .pending
            .get_mut(&operation)
            .ok_or(Error::InvalidState)?;
        if pending.submitted {
            return Err(Error::InvalidState);
        }
        pending.submitted = true;
        self.outbox.retain(|id| *id != operation);
        Ok(())
    }
    pub(crate) fn touch_registry(&mut self, id: EntityId) -> Result<bool, ()> {
        if self.reserved(id) {
            return Ok(false);
        }
        let Some(item) = self.items.get_mut(&id) else {
            return Ok(false);
        };
        item.revision = item.revision.checked_add(1).ok_or(())?;
        if let Some(container) = self.containers.get_mut(&id) {
            container.revision = item.revision;
        }
        Ok(true)
    }
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            constructed: Default::default(),
            transient: Default::default(),
            items: BTreeMap::new(),
            equipped: BTreeMap::new(),
            containers: BTreeMap::new(),
            burden: BTreeMap::new(),
            burden_dirty: Default::default(),
            pending: BTreeMap::new(),
            reserved: BTreeMap::new(),
            death_holds: BTreeMap::new(),
            region_holds: BTreeMap::new(),
            npc_holds: BTreeMap::new(),
            outbox: VecDeque::with_capacity(capacity.min(4096)),
            capacity: capacity.min(4096),
            next: 0,
        }
    }
    pub(crate) fn register_item(&mut self, item: InventoryItem) -> Result<(), Error> {
        if self.items.len() >= self.capacity
            || self.items.contains_key(&item.id)
            || self.reserved(item.id)
            || self.death_held_ancestor(&item)
        {
            return Err(Error::Capacity);
        }
        if item.id.0 == 0
            || item.id.0 == u32::MAX
            || item.stack == 0
            || item.stack > item.maximum_stack
        {
            return Err(Error::InvalidState);
        }
        self.register_equipment_index(&item)?;
        self.invalidate_burden_place(item.place);
        self.items.insert(item.id, item);
        Ok(())
    }
    pub(crate) fn register_container(
        &mut self,
        container: InventoryContainer,
    ) -> Result<(), Error> {
        if self.containers.len() >= self.capacity
            || self.containers.contains_key(&container.id)
            || self.reserved(container.id)
        {
            return Err(Error::Capacity);
        }
        if let Some(owner) = container.root_owner {
            self.invalidate_burden_owner(owner);
        }
        self.containers.insert(container.id, container);
        Ok(())
    }
    pub(crate) fn item(&self, id: EntityId) -> Option<&InventoryItem> {
        self.items.get(&id)
    }
    pub(crate) fn container(&self, id: EntityId) -> Option<&InventoryContainer> {
        self.containers.get(&id)
    }
    pub(crate) fn reserved(&self, id: EntityId) -> bool {
        self.reserved.contains_key(&id)
            || self.death_holds.contains_key(&id)
            || self.region_holds.contains_key(&id)
            || self.npc_holds.contains_key(&id)
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.items.is_empty() || !self.containers.is_empty() || !self.pending.is_empty()
    }
    pub(crate) fn can_accept(&self) -> bool {
        self.pending.len() < self.capacity && self.outbox.len() < self.capacity
    }
    pub(crate) fn apply(
        &mut self,
        request: InventoryRequest,
        authority: InventoryAuthority,
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = propose_inventory(
            request,
            authority,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(authority.actor, proposal)
    }
    pub(crate) fn split(
        &mut self,
        request: InventoryRequest,
        authority: InventoryAuthority,
        prepared: bace_inventory::StackSplitPreparation,
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_stack_split(
            request,
            authority,
            prepared,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(authority.actor, proposal)
    }
    pub(crate) fn validate_grant_containers(
        &self,
        receipt: &InventoryReceipt,
        containers: &[InventoryContainer],
    ) -> Result<(), Error> {
        if containers.is_empty() {
            return Ok(());
        }
        let ticket = self
            .pending_ticket(receipt.operation)
            .ok_or(Error::InvalidState)?;
        if containers.len() > 1024
            || self
                .containers
                .len()
                .checked_add(containers.len())
                .is_none_or(|n| n > self.capacity)
        {
            return Err(Error::Capacity);
        }
        let mut ids = std::collections::BTreeSet::new();
        for container in containers {
            if !ids.insert(container.id)
                || self.containers.contains_key(&container.id)
                || container.root_owner.is_some()
                || container.slots > 1024
                || container.pack_slots > 1024
                || container.generation != 1
                || !ticket.proposal.changes.iter().any(|c| {
                    c.before.is_none()
                        && c.after.id == container.id
                        && c.after.is_container
                        && c.after.revision == container.revision
                })
            {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }
    pub(crate) fn install_grant_containers(
        &mut self,
        actor: EntityId,
        containers: &[InventoryContainer],
    ) {
        for container in containers {
            let mut container = *container;
            container.root_owner = Some(actor);
            self.containers.insert(container.id, container);
        }
        if !containers.is_empty() {
            self.burden.remove(&actor);
        }
    }
    pub(crate) fn inspect_npc_handin(
        &mut self,
        actor: EntityId,
        item: EntityId,
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_npc_inspection(
            actor,
            item,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn grant_many(
        &mut self,
        actor: EntityId,
        prepared: &[InventoryItem],
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_npc_grants(
            actor,
            prepared,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn grant(&mut self, actor: EntityId, item: InventoryItem) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = propose_grant(
            actor,
            item,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn take(
        &mut self,
        actor: EntityId,
        item: EntityId,
        count: u32,
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = propose_take(
            actor,
            item,
            count,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn take_template(
        &mut self,
        actor: EntityId,
        template: u32,
        count: Option<u32>,
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_take_template(
            actor,
            template,
            count,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn has_required(
        &self,
        actor: EntityId,
        required: &[(u32, u32)],
    ) -> Result<bool, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        bace_inventory::has_required_components(
            actor,
            required,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
    }
    pub(crate) fn take_requirements(
        &mut self,
        actor: EntityId,
        required: &[(u32, u32)],
        consumed: &[(u32, u32)],
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_component_use(
            actor,
            required,
            consumed,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn take_items(
        &mut self,
        actor: EntityId,
        requested: &[(EntityId, u32)],
    ) -> Result<u64, Error> {
        let items: Vec<_> = self.items.values().cloned().collect();
        let containers: Vec<_> = self.containers.values().copied().collect();
        let proposal = bace_inventory::propose_take_items(
            actor,
            requested,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )?;
        self.reserve(actor, proposal)
    }
    pub(crate) fn reserve(
        &mut self,
        actor: EntityId,
        mut proposal: InventoryProposal,
    ) -> Result<u64, Error> {
        self.include_generated_descendants(&mut proposal)?;
        if proposal
            .participants
            .iter()
            .any(|(id, _)| self.constructed_ancestor(*id))
        {
            return Err(Error::InvalidState);
        }
        if !self.can_accept() {
            return Err(Error::Capacity);
        }
        self.equipment_after(&proposal.changes)?;
        let additions = proposal
            .changes
            .iter()
            .filter(|c| c.before.is_none())
            .count();
        if self
            .items
            .len()
            .checked_add(additions)
            .is_none_or(|n| n > self.capacity)
        {
            return Err(Error::Capacity);
        }
        if proposal
            .participants
            .iter()
            .any(|(id, _)| self.reserved(*id))
        {
            return Err(Error::DurabilityPending);
        }
        let operation = self.next.checked_add(1).ok_or(Error::Overflow)?;
        for (id, _) in &proposal.participants {
            self.reserved.insert(*id, operation);
        }
        self.next = operation;
        self.pending.insert(
            operation,
            Pending {
                ticket: InventoryTicket {
                    operation,
                    actor,
                    proposal,
                },
                submitted: false,
            },
        );
        self.outbox.push_back(operation);
        Ok(operation)
    }
    pub(crate) fn take_proposal(&mut self) -> Option<InventoryTicket> {
        while let Some(id) = self.outbox.pop_front() {
            if let Some(p) = self.pending.get_mut(&id) {
                p.submitted = true;
                return Some(p.ticket.clone());
            }
        }
        None
    }
    pub(crate) fn retry(&mut self, operation: u64) -> Result<(), Error> {
        if self.outbox.len() == self.capacity {
            return Err(Error::Capacity);
        }
        let p = self
            .pending
            .get_mut(&operation)
            .ok_or(Error::InvalidState)?;
        if !p.submitted {
            return Err(Error::Busy);
        }
        p.submitted = false;
        self.outbox.push_back(operation);
        Ok(())
    }
    /// Call only after a definite rollback, never for timeout/uncertain commit.
    pub(crate) fn reject(&mut self, operation: u64) -> Result<(), Error> {
        let p = self.pending.remove(&operation).ok_or(Error::InvalidState)?;
        for (id, _) in p.ticket.proposal.participants {
            self.reserved.remove(&id);
        }
        self.outbox.retain(|id| *id != operation);
        Ok(())
    }
    pub(crate) fn validate_receipt(&self, receipt: &InventoryReceipt) -> Result<(), Error> {
        let p = self
            .pending
            .get(&receipt.operation)
            .ok_or(Error::InvalidState)?;
        if !p.submitted {
            return Err(Error::InvalidState);
        }
        let mut expected: Vec<_> = p
            .ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect();
        expected.sort_unstable();
        let mut supplied = receipt.revisions.clone();
        supplied.sort_unstable();
        if supplied != expected {
            return Err(Error::InvalidState);
        }
        for (id, revision) in &p.ticket.proposal.participants {
            let current = self
                .items
                .get(id)
                .map(|i| i.revision)
                .or_else(|| self.containers.get(id).map(|c| c.revision))
                .unwrap_or(0);
            if current != *revision || self.reserved.get(id) != Some(&receipt.operation) {
                return Err(Error::InvalidState);
            }
        }
        Ok(())
    }
    pub(crate) fn confirm(&mut self, receipt: &InventoryReceipt) -> Result<InventoryTicket, Error> {
        self.validate_receipt(receipt)?;
        let equipment =
            self.equipment_after(&self.pending[&receipt.operation].ticket.proposal.changes)?;
        let proposal = &self.pending[&receipt.operation].ticket.proposal;
        let container_owners = proposal
            .changes
            .iter()
            .filter(|c| c.after.is_container && c.after.place != ItemPlace::Removed)
            .map(|c| {
                Ok((
                    c.after.id,
                    c.after.revision,
                    self.placement_owner(c.after.place, Some(proposal))?,
                ))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let p = self
            .pending
            .remove(&receipt.operation)
            .ok_or(Error::InvalidState)?;
        self.invalidate_burden_proposal(&p.ticket.proposal);
        for c in &p.ticket.proposal.changes {
            if c.after.place == ItemPlace::Removed {
                self.items.remove(&c.after.id);
                self.containers.remove(&c.after.id);
                self.transient.remove(&c.after.id);
            } else {
                self.items.insert(c.after.id, c.after.clone());
            }
        }
        for (id, revision, owner) in container_owners {
            if let Some(container) = self.containers.get_mut(&id) {
                container.root_owner = owner;
                container.revision = revision;
            }
        }
        for (actor, ids) in equipment {
            if ids.is_empty() {
                self.equipped.remove(&actor);
            } else {
                self.equipped.insert(actor, ids);
            }
        }
        for (id, _) in &p.ticket.proposal.participants {
            self.reserved.remove(id);
        }
        self.outbox.retain(|id| *id != receipt.operation);
        Ok(p.ticket)
    }
    pub(crate) fn owned(&self, actor: EntityId, item: EntityId) -> bool {
        self.items
            .get(&item)
            .is_some_and(|item| self.owned_by(item, actor))
    }
    pub(crate) fn count(&self, actor: EntityId, template: u32) -> u64 {
        self.items
            .values()
            .filter(|i| i.template == template && self.owned_by(i, actor))
            .map(|i| u64::from(i.stack))
            .sum()
    }
    fn owned_by(&self, item: &InventoryItem, actor: EntityId) -> bool {
        self.placement_owner(item.place, None).ok().flatten() == Some(actor)
    }

    pub(crate) fn free_slots(&self, container: EntityId, pack: bool) -> Result<u32, Error> {
        let c = self
            .containers
            .get(&container)
            .ok_or(Error::MissingContainer)?;
        let used=self.items.values().filter(|i|i.pack_slot==pack&&matches!(i.place,ItemPlace::Contained{container:id,equipped:0,..} if id==container)).count();
        (if pack { c.pack_slots } else { c.slots })
            .checked_sub(used as u32)
            .ok_or(Error::InvalidState)
    }
}
