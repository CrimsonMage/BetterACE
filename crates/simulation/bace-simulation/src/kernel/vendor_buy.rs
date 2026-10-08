//! One simulation-owned default Shop Buy reservation. The runtime Buy route
//! remains closed until it can freeze and deliver the joined durable operation.
use super::*;
use bace_economy::{
    VendorBuyQuote, VendorBuyQuoteError, VendorBuyRequest, VendorDefaultOffer, quote_default_buy,
};
use bace_gameplay_api::{ActionContext, InventoryRejection};
use bace_inventory::{
    InventoryItem, InventoryView, propose_vendor_purchase, select_vendor_currency_debits,
};
use std::collections::BTreeSet;

const PYREAL_TEMPLATE: u32 = 273;

/// Immutable accepted source evidence from the durable vendor adapter. The
/// source revision/hash must match the simulation owner's loaded Shop program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VendorBuySource {
    pub vendor_expected_version: i64,
    pub revision: u64,
    pub hash: [u8; 32],
    pub sell_rate: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorBuyReservation {
    pub vendor: EntityId,
    pub marker: EntityId,
    pub actor_revision: u64,
    pub marker_expected_version: i64,
    pub marker_stock_revision: u64,
    pub vendor_expected_version: i64,
    pub source_revision: u64,
    pub source_hash: [u8; 32],
    pub operation_id: String,
    pub quote: VendorBuyQuote,
    pub inventory: crate::InventoryTicket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorBuyReceipt {
    pub vendor: EntityId,
    pub marker: EntityId,
    pub operation_id: String,
    pub marker_version: i64,
    pub marker_stock_revision: u64,
    pub inventory: crate::InventoryReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorBuyError {
    Invalid,
    Unsupported,
    Stale,
    Busy,
    Use(crate::GeneratorServiceError),
    Quote(VendorBuyQuoteError),
    Inventory(InventoryRejection),
}

impl Kernel {
    pub fn vendor_buy_owns_inventory(&self, operation: u64) -> bool {
        self.generated_vendors.values().any(|vendor| {
            vendor
                .pending_buy
                .as_ref()
                .is_some_and(|buy| buy.inventory.operation == operation)
        })
    }

    /// Source-priced default leaf items only. Request identities are checked
    /// against the accepted vendor stock; currency IDs come solely from the
    /// accepted inventory graph in pinned ACE traversal order.
    pub fn reserve_vendor_default_buy(
        &mut self,
        context: ActionContext,
        vendor: EntityId,
        source: VendorBuySource,
        requests: &[VendorBuyRequest],
        prepared: &[InventoryItem],
        operation_id: String,
    ) -> Result<VendorBuyReservation, VendorBuyError> {
        if vendor.0 == 0
            || source.vendor_expected_version <= 0
            || source.revision == 0
            || source.hash == [0; 32]
            || operation_id.is_empty()
            || operation_id.len() > 128
            || !operation_id.bytes().all(|byte| byte.is_ascii_graphic())
            || prepared.is_empty()
            || prepared.len() > 1024
        {
            return Err(VendorBuyError::Invalid);
        }
        let owner = self
            .generated_vendors
            .get(&vendor)
            .ok_or(VendorBuyError::Stale)?;
        let loaded = owner.committed_lazy.as_ref().ok_or(VendorBuyError::Stale)?;
        if owner.pending_lazy.is_some()
            || owner.pending_buy.is_some()
            || !owner.lazy_loaded
            || owner.marker_version <= 0
        {
            return Err(VendorBuyError::Busy);
        }
        if loaded.vendor != vendor
            || loaded.vendor_expected_version > source.vendor_expected_version
            || loaded.source_revision != source.revision
            || loaded.source_hash != source.hash
            || loaded.stock_revision != owner.stock.revision()
            || owner.marker_stock_revision < owner.stock.revision()
            || owner.marker_version.checked_add(1).is_none()
            || owner.marker_stock_revision.checked_add(1).is_none()
        {
            return Err(VendorBuyError::Stale);
        }
        let requested: BTreeSet<_> = requests.iter().map(|request| request.stock_id).collect();
        let offers = owner
            .stock
            .items()
            .iter()
            .filter(|stock| requested.contains(&stock.id))
            .map(|stock| {
                offer(
                    stock,
                    owner.items.get(&stock.id),
                    owner.contents.get(&stock.id),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let quote = quote_default_buy(&owner.stock, requests, &offers, source.sell_rate, 1024)
            .map_err(VendorBuyError::Quote)?;
        if quote.lines.is_empty() || quote.total_cost == 0 {
            return Err(VendorBuyError::Unsupported);
        }
        let expected: Vec<_> = quote
            .lines
            .iter()
            .flat_map(|line| line.stacks.iter().map(|&stack| (line.template, stack)))
            .collect();
        if expected.len() != prepared.len()
            || expected
                .iter()
                .zip(prepared)
                .any(|(&(template, stack), item)| item.template != template || item.stack != stack)
        {
            return Err(VendorBuyError::Invalid);
        }
        let mut fresh_ids = BTreeSet::new();
        if prepared
            .iter()
            .any(|item| !fresh_ids.insert(item.id) || !self.available_generator_id(item.id))
        {
            return Err(VendorBuyError::Invalid);
        }
        let marker = loaded.marker;
        let marker_expected_version = owner.marker_version;
        let marker_stock_revision = owner.marker_stock_revision;
        self.authorize_vendor_use(context, vendor)
            .map_err(VendorBuyError::Use)?;
        let operation = self
            .stage_inventory(context.actor, |inventory| {
                let items: Vec<_> = inventory.items().cloned().collect();
                let containers: Vec<_> = inventory.containers().copied().collect();
                let view = || InventoryView {
                    items: &items,
                    containers: &containers,
                };
                let debits = select_vendor_currency_debits(
                    context.actor,
                    PYREAL_TEMPLATE,
                    quote.total_cost,
                    view(),
                )?;
                let proposal = propose_vendor_purchase(
                    context.actor,
                    PYREAL_TEMPLATE,
                    quote.total_cost,
                    &debits,
                    prepared,
                    view(),
                )?;
                inventory.reserve(context.actor, proposal)
            })
            .map_err(VendorBuyError::Inventory)?;
        let actor_revision = match self.characters.reserve_vendor_buy(context.actor, operation) {
            Ok(revision) => revision,
            Err(()) => {
                self.reject_inventory_inner(operation)
                    .map_err(VendorBuyError::Inventory)?;
                return Err(VendorBuyError::Busy);
            }
        };
        if let Err(error) = self.inventory.claim(operation) {
            self.reject_inventory_inner(operation)
                .map_err(VendorBuyError::Inventory)?;
            self.characters
                .finish_vendor_buy(context.actor, operation, actor_revision, false)
                .map_err(|()| VendorBuyError::Stale)?;
            return Err(VendorBuyError::Inventory(error));
        }
        let Some(inventory) = self.inventory.pending_ticket(operation).cloned() else {
            self.reject_inventory_inner(operation)
                .map_err(VendorBuyError::Inventory)?;
            self.characters
                .finish_vendor_buy(context.actor, operation, actor_revision, false)
                .map_err(|()| VendorBuyError::Stale)?;
            return Err(VendorBuyError::Stale);
        };
        let ticket = VendorBuyReservation {
            vendor,
            marker,
            actor_revision,
            marker_expected_version,
            marker_stock_revision,
            vendor_expected_version: source.vendor_expected_version,
            source_revision: source.revision,
            source_hash: source.hash,
            operation_id,
            quote,
            inventory,
        };
        let Some(owner) = self.generated_vendors.get_mut(&vendor) else {
            self.reject_inventory_inner(operation)
                .map_err(VendorBuyError::Inventory)?;
            self.characters
                .finish_vendor_buy(context.actor, operation, actor_revision, false)
                .map_err(|()| VendorBuyError::Stale)?;
            return Err(VendorBuyError::Stale);
        };
        owner.pending_buy = Some(Box::new(ticket.clone()));
        Ok(ticket)
    }

    pub(super) fn validate_vendor_buy_snapshot(
        &self,
        binding: bace_gameplay_api::CharacterBinding,
        operation: u64,
        revision: u64,
    ) -> bool {
        operation != 0
            && self.generated_vendors.values().any(|vendor| {
                vendor.pending_buy.as_ref().is_some_and(|buy| {
                    buy.inventory.operation == operation
                        && buy.inventory.actor == binding.actor
                        && buy.actor_revision == revision
                        && self.characters.vendor_buy_operation(binding.actor) == Some(operation)
                        && self.inventory.pending_ticket(operation) == Some(&buy.inventory)
                        && self.inventory.reserved(binding.actor)
                })
            })
    }

    /// Only a definite durable rejection may release this joint reservation.
    /// Timeouts and uncertain commits keep the ticket and every item held.
    pub fn reject_vendor_buy(
        &mut self,
        ticket: &VendorBuyReservation,
    ) -> Result<(), VendorBuyError> {
        if self
            .generated_vendors
            .get(&ticket.vendor)
            .and_then(|owner| owner.pending_buy.as_deref())
            != Some(ticket)
        {
            return Err(VendorBuyError::Stale);
        }
        self.reject_inventory_inner(ticket.inventory.operation)
            .map_err(VendorBuyError::Inventory)?;
        self.characters
            .finish_vendor_buy(
                ticket.inventory.actor,
                ticket.inventory.operation,
                ticket.actor_revision,
                false,
            )
            .map_err(|()| VendorBuyError::Stale)?;
        self.generated_vendors
            .get_mut(&ticket.vendor)
            .ok_or(VendorBuyError::Stale)?
            .pending_buy = None;
        Ok(())
    }

    /// Called only after one exact VendorStockOperation receipt verifies the
    /// marker and every player/item participant. No packet publication occurs.
    pub fn confirm_vendor_buy_committed(
        &mut self,
        ticket: &VendorBuyReservation,
        receipt: &VendorBuyReceipt,
    ) -> Result<crate::InventoryTicket, VendorBuyError> {
        let owner = self
            .generated_vendors
            .get(&ticket.vendor)
            .ok_or(VendorBuyError::Stale)?;
        if owner.pending_buy.as_deref() != Some(ticket)
            || owner.stock.revision() != ticket.quote.stock_revision
            || owner.marker_version != ticket.marker_expected_version
            || owner.marker_stock_revision != ticket.marker_stock_revision
            || receipt.vendor != ticket.vendor
            || receipt.marker != ticket.marker
            || receipt.operation_id != ticket.operation_id
            || Some(receipt.marker_version) != ticket.marker_expected_version.checked_add(1)
            || Some(receipt.marker_stock_revision) != ticket.marker_stock_revision.checked_add(1)
            || receipt.inventory.operation != ticket.inventory.operation
            || self.characters.vendor_buy_operation(ticket.inventory.actor)
                != Some(ticket.inventory.operation)
            || self
                .characters
                .get(ticket.inventory.actor)
                .is_none_or(|character| character.revision() != ticket.actor_revision)
        {
            return Err(VendorBuyError::Stale);
        }
        let adopted = self
            .confirm_inventory_committed_inner(&receipt.inventory)
            .map_err(VendorBuyError::Inventory)?;
        self.characters
            .finish_vendor_buy(
                ticket.inventory.actor,
                ticket.inventory.operation,
                ticket.actor_revision,
                true,
            )
            .map_err(|()| VendorBuyError::Stale)?;
        let owner = self
            .generated_vendors
            .get_mut(&ticket.vendor)
            .ok_or(VendorBuyError::Stale)?;
        owner.marker_version = receipt.marker_version;
        owner.marker_stock_revision = receipt.marker_stock_revision;
        owner.pending_buy = None;
        Ok(adopted)
    }
}

fn offer(
    stock: &bace_economy::VendorStockItem,
    source: Option<&std::sync::Arc<bace_content::WeenieV1>>,
    contents: Option<&crate::VendorContents>,
) -> Result<VendorDefaultOffer, VendorBuyError> {
    let source = source.ok_or(VendorBuyError::Stale)?;
    let contents = contents.ok_or(VendorBuyError::Stale)?;
    if source.weenie_id != stock.template
        || !contents.items.is_empty()
        || !contents.containers.is_empty()
        || matches!(source.weenie_type, 10 | 12 | 15 | 21 | 61 | 69 | 71)
    {
        return Err(VendorBuyError::Unsupported);
    }
    let int = |id| {
        source
            .properties
            .ints
            .iter()
            .find(|property| property.id == id)
            .map(|property| property.value)
    };
    let max = int(11);
    if max.is_some_and(|value| value < 0) {
        return Err(VendorBuyError::Invalid);
    }
    let maximum_stack = max.filter(|value| *value > 0).map(|value| value as u32);
    Ok(VendorDefaultOffer {
        stock_id: stock.id,
        template: stock.template,
        value: int(19).unwrap_or(0),
        stack_unit_value: int(15).unwrap_or(0),
        maximum_stack,
        promissory_note: int(1) == Some(0x0004_0000),
        vendor_service: source
            .properties
            .bools
            .iter()
            .any(|property| property.id == 51 && property.value),
        unique: false,
    })
}

#[cfg(test)]
mod tests;
