//! Single simulation-owner housing lifecycle reservations. Content-derived rules
//! and frozen payment item views are supplied by the owning authoritative services.
use bace_gameplay_api::{HousingRejection as Error, HousingRequest};
use bace_housing::{
    HousingActor, HousingProposal, HousingState, PaymentItem, PermissionChange, PurchaseRules,
    RentScheduler,
};
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingRegistration {
    pub slumlord: EntityId,
    pub state: HousingState,
    pub rules: PurchaseRules,
    pub owner_account: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingTicket {
    pub operation: u64,
    pub actor: EntityId,
    pub actor_account: u64,
    pub proposal: HousingProposal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HousingReceipt {
    pub operation: u64,
    pub house: EntityId,
    pub revision: u64,
    pub generation: u64,
    pub owner: Option<EntityId>,
}
struct Pending {
    ticket: HousingTicket,
    submitted: bool,
}
pub(crate) struct Housing {
    houses: BTreeMap<EntityId, HousingRegistration>,
    slumlords: BTreeMap<EntityId, EntityId>,
    pending: BTreeMap<u64, Pending>,
    reserved: BTreeMap<EntityId, u64>,
    outbox: VecDeque<u64>,
    rent: RentScheduler,
    capacity: usize,
    next: u64,
}
impl Housing {
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.clamp(1, 4096);
        Self {
            houses: BTreeMap::new(),
            slumlords: BTreeMap::new(),
            pending: BTreeMap::new(),
            reserved: BTreeMap::new(),
            outbox: VecDeque::with_capacity(capacity),
            rent: RentScheduler::new(capacity).expect("bounded housing capacity"),
            capacity,
            next: 0,
        }
    }
    pub(crate) fn register(&mut self, registration: HousingRegistration) -> Result<(), Error> {
        registration.state.validate()?;
        if registration.slumlord.0 == 0
            || registration.owner_account == Some(0)
            || registration.state.owner.is_some() != registration.owner_account.is_some()
        {
            return Err(Error::Requirements);
        }
        if self.houses.len() == self.capacity
            || self.houses.contains_key(&registration.state.house)
            || self.slumlords.contains_key(&registration.slumlord)
        {
            return Err(Error::Capacity);
        }
        if registration.state.owner.is_some() {
            self.rent
                .schedule(registration.state.house, registration.state.rent_due)?;
        }
        self.slumlords
            .insert(registration.slumlord, registration.state.house);
        self.houses.insert(registration.state.house, registration);
        Ok(())
    }
    pub(crate) fn touch_registry(&mut self, id: EntityId) -> Result<bool, Error> {
        if self.reserved(id) {
            return Ok(false);
        }
        let Some(house) = self.houses.get_mut(&id) else {
            return Ok(false);
        };
        house.state.revision = house.state.revision.checked_add(1).ok_or(Error::Overflow)?;
        Ok(true)
    }
    pub(crate) fn request_house(
        &self,
        request: &HousingRequest,
        actor: HousingActor,
    ) -> Result<EntityId, Error> {
        match request {
            HousingRequest::Buy { slumlord, .. } | HousingRequest::Rent { slumlord, .. } => self
                .slumlords
                .get(slumlord)
                .copied()
                .ok_or(Error::MissingHouse),
            _ => self
                .query(actor)
                .map(|state| state.house)
                .ok_or(Error::MissingHouse),
        }
    }
    pub(crate) fn state(&self, id: EntityId) -> Option<&HousingState> {
        self.houses.get(&id).map(|r| &r.state)
    }
    pub(crate) fn reserved(&self, id: EntityId) -> bool {
        self.reserved.contains_key(&id)
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.houses.is_empty() || !self.pending.is_empty()
    }
    pub(crate) fn can_accept(&self) -> bool {
        self.pending.len() < self.capacity && self.outbox.len() < self.capacity
    }
    pub(crate) fn query(&self, actor: HousingActor) -> Option<&HousingState> {
        self.houses
            .values()
            .find(|h| h.state.owner == Some(actor.actor) || h.owner_account == Some(actor.account))
            .map(|h| &h.state)
    }
    pub(crate) fn permits(&self, house: EntityId, actor: HousingActor, storage: bool) -> bool {
        self.houses.get(&house).is_some_and(|h| {
            h.state.owner.is_some()
                && (h.owner_account == Some(actor.account) || h.state.permits(actor.actor, storage))
        })
    }
    pub(crate) fn apply(
        &mut self,
        request: HousingRequest,
        actor: HousingActor,
        items: &[PaymentItem],
        now: i64,
    ) -> Result<u64, Error> {
        if !self.can_accept() {
            return Err(Error::Capacity);
        }
        if self.reserved(actor.actor) {
            return Err(Error::DurabilityPending);
        }
        let id = match &request {
            HousingRequest::Buy { slumlord, .. } | HousingRequest::Rent { slumlord, .. } => {
                *self.slumlords.get(slumlord).ok_or(Error::MissingHouse)?
            }
            _ => self.query(actor).ok_or(Error::MissingHouse)?.house,
        };
        let registered = self.houses.get(&id).ok_or(Error::MissingHouse)?;
        let proposal = match request {
            HousingRequest::Buy { payments, .. } => {
                validate_payments(&payments, items)?;
                let mut actor = actor;
                actor.owns_house |= self.query(actor).is_some();
                bace_housing::propose_purchase(
                    &registered.state,
                    actor,
                    &registered.rules,
                    items,
                    now,
                )?
            }
            HousingRequest::Rent { payments, .. } => {
                validate_payments(&payments, items)?;
                if !actor.in_range {
                    return Err(Error::OutOfRange);
                }
                if registered.owner_account != Some(actor.account) {
                    return Err(Error::NotOwner);
                }
                bace_housing::propose_rent(
                    &registered.state,
                    registered.state.owner.ok_or(Error::NotOwner)?,
                    items,
                )?
            }
            HousingRequest::Abandon => {
                bace_housing::propose_abandon(&registered.state, actor.actor)?
            }
            HousingRequest::Query => return Err(Error::Requirements),
            other => {
                let change = match other {
                    HousingRequest::SetOpen(v) => PermissionChange::Open(v),
                    HousingRequest::SetStorageOpen(v) => PermissionChange::StorageOpen(v),
                    HousingRequest::SetHooksVisible(v) => PermissionChange::HooksVisible(v),
                    HousingRequest::SetGuest { guest, storage } => PermissionChange::Guest {
                        player: guest,
                        storage,
                    },
                    HousingRequest::RemoveGuest(id) => PermissionChange::RemoveGuest(id),
                    HousingRequest::ClearGuests => PermissionChange::ClearGuests,
                    HousingRequest::ClearStorageGuests => PermissionChange::ClearStorage,
                    _ => return Err(Error::Requirements),
                };
                if registered.owner_account != Some(actor.account) {
                    return Err(Error::NotOwner);
                }
                bace_housing::propose_permission(
                    &registered.state,
                    registered.state.owner.ok_or(Error::NotOwner)?,
                    change,
                )?
            }
        };
        self.reserve(actor.actor, actor.account, proposal)
    }
    pub(crate) fn next_rent(&self, now: i64) -> Option<EntityId> {
        self.rent.next_due(now).map(|(_, id)| id)
    }
    pub(crate) fn due_rent(
        &mut self,
        house: EntityId,
        now: i64,
        enabled: bool,
        requirements_met: bool,
    ) -> Result<Option<u64>, Error> {
        let r = self.houses.get(&house).ok_or(Error::MissingHouse)?;
        let Some(proposal) =
            bace_housing::propose_due_rent(&r.state, now, enabled, requirements_met)?
        else {
            return Ok(None);
        };
        let actor = r.state.owner.ok_or(Error::NotOwner)?;
        let account = r.owner_account.ok_or(Error::NotOwner)?;
        self.reserve(actor, account, proposal).map(Some)
    }
    fn reserve(
        &mut self,
        actor: EntityId,
        account: u64,
        proposal: HousingProposal,
    ) -> Result<u64, Error> {
        if !self.can_accept() {
            return Err(Error::Capacity);
        }
        let mut ids = vec![actor, proposal.before.house];
        if let Some(owner) = proposal.before.owner {
            ids.push(owner)
        }
        if let Some(owner) = proposal.after.owner {
            ids.push(owner)
        }
        ids.extend(proposal.payments.iter().map(|p| p.item));
        ids.sort_unstable();
        ids.dedup();
        if ids.iter().any(|id| self.reserved(*id)) {
            return Err(Error::DurabilityPending);
        }
        let operation = self.next.checked_add(1).ok_or(Error::Overflow)?;
        for id in ids {
            self.reserved.insert(id, operation);
        }
        self.next = operation;
        self.pending.insert(
            operation,
            Pending {
                ticket: HousingTicket {
                    operation,
                    actor,
                    actor_account: account,
                    proposal,
                },
                submitted: false,
            },
        );
        self.outbox.push_back(operation);
        Ok(operation)
    }
    pub(crate) fn take_proposal(&mut self) -> Option<HousingTicket> {
        while let Some(id) = self.outbox.pop_front() {
            if let Some(p) = self.pending.get_mut(&id) {
                p.submitted = true;
                return Some(p.ticket.clone());
            }
        }
        None
    }
    pub(crate) fn retry(&mut self, id: u64) -> Result<(), Error> {
        if self.outbox.len() == self.capacity {
            return Err(Error::Capacity);
        }
        let p = self.pending.get_mut(&id).ok_or(Error::MissingHouse)?;
        if !p.submitted {
            return Err(Error::DurabilityPending);
        }
        p.submitted = false;
        self.outbox.push_back(id);
        Ok(())
    }
    /// Definite rollback only. Uncertain commits retain their reservation.
    pub(crate) fn reject(&mut self, id: u64) -> Result<(), Error> {
        self.pending.remove(&id).ok_or(Error::MissingHouse)?;
        self.reserved.retain(|_, operation| *operation != id);
        self.outbox.retain(|operation| *operation != id);
        Ok(())
    }
    pub(crate) fn pending_ticket(&self, operation: u64) -> Option<&HousingTicket> {
        self.pending.get(&operation).map(|p| &p.ticket)
    }
    pub(crate) fn validate_receipt(&self, receipt: HousingReceipt) -> Result<(), Error> {
        let pending = self
            .pending
            .get(&receipt.operation)
            .ok_or(Error::MissingHouse)?;
        let after = pending.ticket.proposal.after.clone();
        if !pending.submitted
            || receipt.house != after.house
            || receipt.revision != after.revision
            || receipt.generation != after.generation
            || receipt.owner != after.owner
        {
            return Err(Error::Requirements);
        }
        let current = self.houses.get(&receipt.house).ok_or(Error::MissingHouse)?;
        if current.state != pending.ticket.proposal.before {
            return Err(Error::StaleView);
        }
        self.rent.validate_replace(
            receipt.house,
            pending
                .ticket
                .proposal
                .before
                .owner
                .map(|_| pending.ticket.proposal.before.rent_due),
            after.owner.map(|_| after.rent_due),
        )?;
        Ok(())
    }
    pub(crate) fn confirm(&mut self, receipt: HousingReceipt) -> Result<HousingTicket, Error> {
        self.validate_receipt(receipt)?;
        let pending = self
            .pending
            .get(&receipt.operation)
            .ok_or(Error::MissingHouse)?;
        let after = pending.ticket.proposal.after.clone();
        self.rent.replace(
            receipt.house,
            pending
                .ticket
                .proposal
                .before
                .owner
                .map(|_| pending.ticket.proposal.before.rent_due),
            after.owner.map(|_| after.rent_due),
        )?;
        let pending = self
            .pending
            .remove(&receipt.operation)
            .ok_or(Error::MissingHouse)?;
        let registration = self
            .houses
            .get_mut(&receipt.house)
            .ok_or(Error::MissingHouse)?;
        registration.state = pending.ticket.proposal.after.clone();
        registration.owner_account = if registration.state.owner.is_none() {
            None
        } else if pending.ticket.proposal.ownership_changed {
            Some(pending.ticket.actor_account)
        } else {
            registration.owner_account
        };
        self.reserved
            .retain(|_, operation| *operation != receipt.operation);
        self.outbox
            .retain(|operation| *operation != receipt.operation);
        Ok(pending.ticket)
    }
}
fn validate_payments(ids: &[EntityId], items: &[PaymentItem]) -> Result<(), Error> {
    if ids.len() > 1024 || ids.len() != items.len() {
        return Err(Error::InvalidPayment);
    }
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    if ids.windows(2).any(|p| p[0] == p[1])
        || items
            .iter()
            .any(|item| ids.binary_search(&item.item).is_err())
    {
        return Err(Error::InvalidPayment);
    }
    Ok(())
}
