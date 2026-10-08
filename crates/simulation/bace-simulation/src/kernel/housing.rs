//! Single-owner kernel housing operations.
use super::*;
impl Kernel {
    pub fn pending_housing_ticket(&self, operation: u64) -> Option<&crate::HousingTicket> {
        self.housing.pending_ticket(operation)
    }

    pub fn register_housing(
        &mut self,
        registration: crate::HousingRegistration,
    ) -> Result<(), bace_gameplay_api::HousingRejection> {
        self.housing.register(registration)
    }
    pub fn housing_state(&self, house: EntityId) -> Option<&bace_housing::HousingState> {
        self.housing.state(house)
    }
    pub fn query_housing(
        &self,
        actor: bace_housing::HousingActor,
    ) -> Option<&bace_housing::HousingState> {
        self.housing.query(actor)
    }
    pub fn housing_permits(
        &self,
        house: EntityId,
        actor: bace_housing::HousingActor,
        storage: bool,
    ) -> bool {
        self.housing.permits(house, actor, storage)
    }
    pub fn propose_housing(
        &mut self,
        context: ActionContext,
        request: bace_gameplay_api::HousingRequest,
        actor: bace_housing::HousingActor,
        payments: &[bace_housing::PaymentItem],
        now: i64,
    ) -> Result<u64, bace_gameplay_api::HousingRejection> {
        use bace_gameplay_api::HousingRejection as E;
        if actor.actor != context.actor || actor.account != context.account.0 {
            return Err(E::NotOwner);
        }
        if !self.housing.can_accept() {
            return Err(E::Capacity);
        }
        if self.inventory.reserved(actor.actor)
            || self.npcs.reserved(actor.actor)
            || self.characters.reserved(actor.actor)
        {
            return Err(E::DurabilityPending);
        }
        let house = self.housing.request_house(&request, actor)?;
        let mut ids = vec![house, actor.actor];
        ids.extend(self.housing.state(house).and_then(|state| state.owner));
        ids.extend(payments.iter().map(|payment| payment.item));
        let reserved = self.reserve_housing_magic(ids)?;
        let result = (|| {
            self.sync_registry_revisions().map_err(|_| E::Overflow)?;
            for payment in payments {
                let item = self.inventory.item(payment.item).ok_or(E::Requirements)?;
                if item.template != payment.template
                    || item.revision != payment.revision
                    || payment.count > item.stack
                    || self.inventory.reserved(payment.item)
                    || !self.inventory.owned(actor.actor, payment.item)
                {
                    return Err(E::Requirements);
                }
            }
            self.characters
                .authorize(context, self.world.body(context.actor).is_ok())
                .map_err(|_| E::NotOwner)?;
            self.housing.apply(request, actor, payments, now)
        })();
        match result {
            Ok(operation) => {
                let used = Self::housing_magic_ids(
                    self.housing
                        .pending_ticket(operation)
                        .expect("admitted housing ticket"),
                );
                let unused: Vec<_> = reserved
                    .into_iter()
                    .filter(|id| !used.contains(id))
                    .collect();
                self.release_housing_magic_ids(&unused);
            }
            Err(_) => self.release_housing_magic_ids(&reserved),
        }
        result
    }

    pub fn next_housing_rent(&self, now: i64) -> Option<EntityId> {
        self.housing.next_rent(now)
    }
    pub fn propose_due_housing_rent(
        &mut self,
        house: EntityId,
        now: i64,
        enabled: bool,
        requirements_met: bool,
    ) -> Result<Option<u64>, bace_gameplay_api::HousingRejection> {
        let mut ids = vec![house];
        ids.extend(self.housing.state(house).and_then(|state| state.owner));
        let reserved = self.reserve_housing_magic(ids)?;
        let result = self
            .sync_registry_revisions()
            .map_err(|_| bace_gameplay_api::HousingRejection::Overflow)
            .and_then(|_| self.housing.due_rent(house, now, enabled, requirements_met));
        if !matches!(result, Ok(Some(_))) {
            self.release_housing_magic_ids(&reserved);
        }
        result
    }
    pub fn take_housing_proposal(&mut self) -> Option<crate::HousingTicket> {
        self.housing.take_proposal()
    }
    pub fn retry_housing(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::HousingRejection> {
        self.housing.retry(operation)
    }
    pub fn reject_housing(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::HousingRejection> {
        if self
            .housing_inventory_bindings
            .values()
            .any(|id| *id == operation)
        {
            return Err(bace_gameplay_api::HousingRejection::DurabilityPending);
        }
        let ticket = self
            .housing
            .pending_ticket(operation)
            .ok_or(bace_gameplay_api::HousingRejection::MissingHouse)?
            .clone();
        self.preflight_housing_magic(&ticket)?;
        self.housing.reject(operation)?;
        self.release_housing_magic(&ticket);
        Ok(())
    }
    pub fn confirm_housing_committed(
        &mut self,
        receipt: crate::HousingReceipt,
    ) -> Result<crate::HousingTicket, bace_gameplay_api::HousingRejection> {
        if self
            .housing_inventory_bindings
            .values()
            .any(|id| *id == receipt.operation)
        {
            return Err(bace_gameplay_api::HousingRejection::DurabilityPending);
        }
        let ticket = self
            .housing
            .pending_ticket(receipt.operation)
            .ok_or(bace_gameplay_api::HousingRejection::StaleView)?;
        if !ticket.proposal.payments.is_empty() || ticket.proposal.currency_change != 0 {
            return Err(bace_gameplay_api::HousingRejection::InvalidPayment);
        }
        self.preflight_housing_magic(ticket)?;
        let ticket = self.housing.confirm(receipt)?;
        self.release_housing_magic(&ticket);
        Ok(ticket)
    }
    fn reserve_housing_magic(
        &mut self,
        mut ids: Vec<EntityId>,
    ) -> Result<Vec<EntityId>, bace_gameplay_api::HousingRejection> {
        use bace_gameplay_api::HousingRejection as E;
        ids.sort_unstable();
        ids.dedup();
        ids.retain(|id| self.magic.registry(*id).is_some());
        if ids.iter().any(|id| {
            self.magic.registry_reserved(*id)
                || self.housing.reserved(*id)
                || self.inventory.reserved(*id)
                || self.characters.reserved(*id)
                || self.npcs.reserved(*id)
        }) {
            return Err(E::DurabilityPending);
        }
        let now = self.tick as f64 / 30.0;
        for (index, &id) in ids.iter().enumerate() {
            if self.magic.reserve_registry(id, true, now).is_err() {
                self.release_housing_magic_ids(&ids[..index]);
                return Err(E::DurabilityPending);
            }
        }
        Ok(ids)
    }
    pub(super) fn preflight_housing_magic(
        &self,
        ticket: &crate::HousingTicket,
    ) -> Result<(), bace_gameplay_api::HousingRejection> {
        if Self::housing_magic_ids(ticket).iter().any(|id| {
            self.magic.registry(*id).is_some()
                && (!self.magic.registry_reserved(*id)
                    || self.magic.registry_failure(*id).is_some())
        }) {
            return Err(bace_gameplay_api::HousingRejection::DurabilityPending);
        }
        Ok(())
    }
    pub(super) fn release_housing_magic(&mut self, ticket: &crate::HousingTicket) {
        self.release_housing_magic_ids(&Self::housing_magic_ids(ticket));
    }
    pub(super) fn housing_magic_ids(ticket: &crate::HousingTicket) -> Vec<EntityId> {
        let mut ids = vec![ticket.actor, ticket.proposal.before.house];
        ids.extend(ticket.proposal.before.owner);
        ids.extend(ticket.proposal.after.owner);
        ids.extend(ticket.proposal.payments.iter().map(|payment| payment.item));
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    fn release_housing_magic_ids(&mut self, ids: &[EntityId]) {
        let now = self.tick as f64 / 30.0;
        for &id in ids {
            if self.magic.registry(id).is_some() {
                self.magic
                    .reserve_registry(id, false, now)
                    .expect("single-owner reserved registry release");
            }
        }
    }
    pub fn has_housing_state(&self) -> bool {
        self.housing.has_state()
    }
}
