//! Coupled receipt adoption on the one simulation owner. These APIs do not
//! acknowledge a database write; callers must commit both proposals atomically.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HousingPaymentBinding {
    housing: crate::HousingTicket,
    inventory_operation: u64,
}
impl HousingPaymentBinding {
    pub fn housing(&self) -> &crate::HousingTicket {
        &self.housing
    }
    pub fn inventory_operation(&self) -> u64 {
        self.inventory_operation
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HousingPaymentError {
    Housing(bace_gameplay_api::HousingRejection),
    Inventory(bace_gameplay_api::InventoryRejection),
    Conflict,
    CurrencyChangeRequired,
}
impl Kernel {
    /// Exact-ID payment proposal bound to an already admitted housing operation.
    /// Nonzero change requires a prepared combined take/grant transaction; this
    /// narrower entry point refuses it instead of burning the overpayment.
    pub fn prepare_housing_payment(
        &mut self,
        ticket: &crate::HousingTicket,
    ) -> Result<HousingPaymentBinding, HousingPaymentError> {
        if self.housing.pending_ticket(ticket.operation) != Some(ticket) {
            return Err(HousingPaymentError::Conflict);
        }
        if ticket.proposal.currency_change != 0 {
            return Err(HousingPaymentError::CurrencyChangeRequired);
        }
        if ticket.proposal.payments.is_empty() {
            return Err(HousingPaymentError::Conflict);
        }
        if self
            .housing_inventory_bindings
            .values()
            .any(|id| *id == ticket.operation)
        {
            return Err(HousingPaymentError::Conflict);
        }
        self.prepare_inventory_time()
            .map_err(HousingPaymentError::Inventory)?;
        for payment in &ticket.proposal.payments {
            let item = self
                .inventory
                .item(payment.item)
                .ok_or(HousingPaymentError::Conflict)?;
            if !self.inventory.owned(ticket.actor, payment.item)
                || item.revision != payment.revision
                || item.stack < payment.count
            {
                return Err(HousingPaymentError::Conflict);
            }
        }
        let payments: Vec<_> = ticket
            .proposal
            .payments
            .iter()
            .map(|p| (p.item, p.count))
            .collect();
        let inventory_operation = self
            .inventory
            .take_items(ticket.actor, &payments)
            .map_err(HousingPaymentError::Inventory)?;
        if let Err(error) =
            self.reserve_inventory_registries(inventory_operation, &Self::housing_magic_ids(ticket))
        {
            self.inventory
                .reject(inventory_operation)
                .map_err(HousingPaymentError::Inventory)?;
            return Err(HousingPaymentError::Inventory(error));
        }
        self.housing_inventory_bindings
            .insert(inventory_operation, ticket.operation);
        Ok(HousingPaymentBinding {
            housing: ticket.clone(),
            inventory_operation,
        })
    }
    /// Adopt BOTH effects only after the same durable operation commits them.
    /// Every fallible receipt/queue/CAS preflight precedes either mutation.
    pub fn confirm_housing_payment(
        &mut self,
        binding: &HousingPaymentBinding,
        house: crate::HousingReceipt,
        inventory: &crate::InventoryReceipt,
    ) -> Result<(crate::HousingTicket, crate::InventoryTicket), HousingPaymentError> {
        if self
            .housing_inventory_bindings
            .get(&binding.inventory_operation)
            != Some(&binding.housing.operation)
            || house.operation != binding.housing.operation
            || inventory.operation != binding.inventory_operation
            || self.housing.pending_ticket(house.operation) != Some(&binding.housing)
        {
            return Err(HousingPaymentError::Conflict);
        }
        self.housing
            .validate_receipt(house)
            .map_err(HousingPaymentError::Housing)?;
        self.inventory
            .validate_receipt(inventory)
            .map_err(HousingPaymentError::Inventory)?;
        self.preflight_housing_magic(&binding.housing)
            .map_err(HousingPaymentError::Housing)?;
        self.preflight_inventory_registries(binding.inventory_operation, true)
            .map_err(HousingPaymentError::Inventory)?;
        // No I/O, callbacks or other owner mutations occur after these checks.
        let inventory = self
            .inventory
            .confirm(inventory)
            .expect("single-owner inventory receipt preflight");
        let housing = self
            .housing
            .confirm(house)
            .expect("single-owner housing receipt preflight");
        self.retire_inventory_registries(&inventory)
            .map_err(HousingPaymentError::Inventory)?;
        self.release_inventory_registries(binding.inventory_operation)
            .map_err(HousingPaymentError::Inventory)?;
        self.release_housing_magic(&housing);
        self.housing_inventory_bindings
            .remove(&binding.inventory_operation);
        Ok((housing, inventory))
    }
    /// Confirmed rollback only. Unknown commit outcomes must retain both owners.
    pub fn reject_housing_payment(
        &mut self,
        binding: &HousingPaymentBinding,
    ) -> Result<(), HousingPaymentError> {
        if self
            .housing_inventory_bindings
            .get(&binding.inventory_operation)
            != Some(&binding.housing.operation)
            || self.housing.pending_ticket(binding.housing.operation) != Some(&binding.housing)
        {
            return Err(HousingPaymentError::Conflict);
        }
        self.preflight_housing_magic(&binding.housing)
            .map_err(HousingPaymentError::Housing)?;
        self.preflight_inventory_registries(binding.inventory_operation, false)
            .map_err(HousingPaymentError::Inventory)?;
        self.inventory
            .reject(binding.inventory_operation)
            .map_err(HousingPaymentError::Inventory)?;
        self.housing
            .reject(binding.housing.operation)
            .expect("single-owner retained housing proposal");
        self.release_inventory_registries(binding.inventory_operation)
            .map_err(HousingPaymentError::Inventory)?;
        self.release_housing_magic(&binding.housing);
        self.housing_inventory_bindings
            .remove(&binding.inventory_operation);
        Ok(())
    }
    /// Read projections for freeze/admission adapters, never duplicate owners.
    pub fn inventory_items(&self) -> impl Iterator<Item = &bace_inventory::InventoryItem> {
        self.inventory.items()
    }
    pub fn inventory_containers(
        &self,
    ) -> impl Iterator<Item = &bace_inventory::InventoryContainer> {
        self.inventory.containers()
    }
}
