//! Bounded adapter requests for one simulation-owned Shop stock transition.
use crate::{
    GeneratorServiceError, PreparedVendorLazyStock, VendorBuyError, VendorBuyReceipt,
    VendorBuyReservation, VendorBuySource, VendorLazyStockReceipt, VendorLazyStockTicket,
};
use bace_economy::VendorBuyRequest;
use bace_gameplay_api::ActionContext;
use bace_inventory::InventoryItem;
use bace_types::EntityId;

pub struct VendorCommand {
    pub correlation: u64,
    pub action: VendorAction,
}

pub enum VendorAction {
    Reserve {
        context: ActionContext,
        batch: Box<PreparedVendorLazyStock>,
    },
    Adopt {
        context: ActionContext,
        batch: Box<PreparedVendorLazyStock>,
        receipt: VendorLazyStockReceipt,
    },
    Confirm(VendorLazyStockReceipt),
    ReserveDefaultBuy {
        context: ActionContext,
        vendor: EntityId,
        source: VendorBuySource,
        requests: Vec<VendorBuyRequest>,
        prepared: Vec<InventoryItem>,
        operation_id: String,
    },
    RejectBuy(Box<VendorBuyReservation>),
    ConfirmBuy {
        ticket: Box<VendorBuyReservation>,
        receipt: VendorBuyReceipt,
    },
}

impl VendorCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0
            && match &self.action {
                VendorAction::Reserve { context, batch }
                | VendorAction::Adopt { context, batch, .. } => {
                    context.actor.0 != 0
                        && batch.vendor != context.actor
                        && batch.entries.len() <= 1024
                        && batch
                            .entries
                            .iter()
                            .try_fold(0_usize, |total, entry| {
                                entry
                                    .tree
                                    .items
                                    .len()
                                    .checked_add(1)
                                    .and_then(|count| total.checked_add(count))
                            })
                            .is_some_and(|total| total <= 1024)
                        && batch.operation_id.len() <= 128
                        && !matches!(&self.action, VendorAction::Adopt { receipt, .. } if receipt.items.len() > 1024)
                }
                VendorAction::Confirm(receipt) => receipt.items.len() <= 1024,
                VendorAction::ReserveDefaultBuy {
                    context,
                    vendor,
                    source,
                    requests,
                    prepared,
                    operation_id,
                } => {
                    context.actor.0 != 0
                        && vendor.0 != 0
                        && *vendor != context.actor
                        && source.vendor_expected_version > 0
                        && source.revision != 0
                        && source.hash != [0; 32]
                        && !requests.is_empty()
                        && requests.len() <= 1024
                        && !prepared.is_empty()
                        && prepared.len() <= 1024
                        && !operation_id.is_empty()
                        && operation_id.len() <= 128
                        && operation_id.bytes().all(|byte| byte.is_ascii_graphic())
                }
                VendorAction::RejectBuy(ticket) => buy_ticket_bounds(ticket),
                VendorAction::ConfirmBuy { ticket, receipt } => {
                    buy_ticket_bounds(ticket)
                        && receipt.operation_id == ticket.operation_id
                        && receipt.inventory.revisions.len() <= 1024
                }
            }
    }
}

fn buy_ticket_bounds(ticket: &VendorBuyReservation) -> bool {
    ticket.vendor.0 != 0
        && ticket.marker.0 != 0
        && ticket.inventory.operation != 0
        && ticket.inventory.proposal.changes.len() <= 1024
        && !ticket.operation_id.is_empty()
        && ticket.operation_id.len() <= 128
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VendorDecision {
    Reserved(VendorLazyStockTicket),
    Adopted(VendorLazyStockTicket),
    Confirmed,
    BuyReserved(Box<VendorBuyReservation>),
    BuyRejected,
    BuyConfirmed(crate::InventoryTicket),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorCommandError {
    Lazy(GeneratorServiceError),
    Buy(VendorBuyError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorOutcome {
    pub correlation: u64,
    pub result: Result<VendorDecision, VendorCommandError>,
}
