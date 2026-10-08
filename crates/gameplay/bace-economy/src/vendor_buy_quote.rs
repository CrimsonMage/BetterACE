//! Bounded default-item Buy quote from accepted vendor stock. Pinned ACE
//! Vendor.BuyItems_ValidateTransaction, ItemProfileToWorldObjects and
//! WorldObject.SetStackSize supply selection, partition and value order.
use crate::{PriceError, VendorStock, vendor_sell_cost};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VendorBuyRequest {
    pub stock_id: u32,
    pub amount: i32,
}

/// Server-owned materialization inputs for one default stock identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VendorDefaultOffer {
    pub stock_id: u32,
    pub template: u32,
    /// Int19 Value of a freshly constructed nonstackable item.
    pub value: i32,
    /// Int15 StackUnitValue, used by SetStackSize for stackable items.
    pub stack_unit_value: i32,
    /// Positive Int11 MaxStackSize; None means one item per requested unit.
    pub maximum_stack: Option<u32>,
    pub promissory_note: bool,
    pub vendor_service: bool,
    pub unique: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorBuyLine {
    pub stock_id: u32,
    pub template: u32,
    /// Source ItemProfileToWorldObjects order. One entry is one fresh object.
    pub stacks: Vec<u32>,
    pub cost: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorBuyQuote {
    pub stock_revision: u64,
    pub lines: Vec<VendorBuyLine>,
    pub total_cost: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorBuyQuoteError {
    Invalid,
    Capacity,
    Unsupported,
    Overflow,
}

/// Quote only. Inventory capacity, uniqueness, currency ownership and durable
/// receipt must still join under one owner before Buy can be accepted.
pub fn quote_default_buy(
    stock: &VendorStock,
    requests: &[VendorBuyRequest],
    offers: &[VendorDefaultOffer],
    sell_rate: Option<f64>,
    max_instances: usize,
) -> Result<VendorBuyQuote, VendorBuyQuoteError> {
    if stock.revision() == 0
        || requests.len() > 1024
        || offers.len() > 4096
        || !(1..=1024).contains(&max_instances)
        || sell_rate.is_some_and(|rate| !rate.is_finite() || rate < 0.0)
    {
        return Err(VendorBuyQuoteError::Invalid);
    }
    let mut ids = BTreeSet::new();
    for offer in offers {
        if offer.stock_id == 0
            || offer.template == 0
            || offer.value < 0
            || offer.stack_unit_value < 0
            || offer.maximum_stack == Some(0)
            || offer
                .maximum_stack
                .is_some_and(|maximum| maximum > i32::MAX as u32)
            || !ids.insert(offer.stock_id)
            || !stock
                .items()
                .iter()
                .any(|item| item.id == offer.stock_id && item.template == offer.template)
        {
            return Err(VendorBuyQuoteError::Invalid);
        }
    }
    let mut lines = Vec::new();
    let mut total_cost = 0_u32;
    let mut instances = 0_usize;
    for request in requests {
        // ACE tests IsValidAmount before it resolves the stock identity.
        if request.amount <= 0 {
            return Err(VendorBuyQuoteError::Invalid);
        }
        let Some(offer) = offers
            .iter()
            .find(|offer| offer.stock_id == request.stock_id)
        else {
            continue; // ACE silently skips unknown GUIDs after amount validation.
        };
        if offer.vendor_service || offer.unique {
            return Err(VendorBuyQuoteError::Unsupported);
        }
        let maximum = offer.maximum_stack.unwrap_or(1);
        let amount = u32::try_from(request.amount).map_err(|_| VendorBuyQuoteError::Invalid)?;
        let count = amount.div_ceil(maximum);
        instances = instances
            .checked_add(usize::try_from(count).map_err(|_| VendorBuyQuoteError::Capacity)?)
            .filter(|count| *count <= max_instances)
            .ok_or(VendorBuyQuoteError::Capacity)?;
        let mut remaining = amount;
        let mut stacks = Vec::with_capacity(count as usize);
        let mut line_cost = 0_u32;
        while remaining != 0 {
            let stack = remaining.min(maximum);
            remaining -= stack;
            let value = if offer.maximum_stack.is_some() {
                i32::try_from(stack)
                    .ok()
                    .and_then(|stack| offer.stack_unit_value.checked_mul(stack))
                    .ok_or(VendorBuyQuoteError::Overflow)?
            } else {
                offer.value
            };
            let cost = vendor_sell_cost(Some(value), sell_rate, offer.promissory_note).map_err(
                |error| match error {
                    PriceError::InvalidInput => VendorBuyQuoteError::Invalid,
                    PriceError::Overflow => VendorBuyQuoteError::Overflow,
                },
            )?;
            line_cost = line_cost
                .checked_add(cost)
                .ok_or(VendorBuyQuoteError::Overflow)?;
            stacks.push(stack);
        }
        total_cost = total_cost
            .checked_add(line_cost)
            .ok_or(VendorBuyQuoteError::Overflow)?;
        lines.push(VendorBuyLine {
            stock_id: request.stock_id,
            template: offer.template,
            stacks,
            cost: line_cost,
        });
    }
    Ok(VendorBuyQuote {
        stock_revision: stock.revision(),
        lines,
        total_cost,
    })
}
