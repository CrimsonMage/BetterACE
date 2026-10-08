//! Vendors, currencies and player trading.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod pricing;
pub use pricing::{PriceError, vendor_buy_cost, vendor_sell_cost};
mod vendor_buy_quote;
mod vendor_stock;
pub use vendor_buy_quote::{
    VendorBuyLine, VendorBuyQuote, VendorBuyQuoteError, VendorBuyRequest, VendorDefaultOffer,
    quote_default_buy,
};
pub use vendor_stock::{
    VendorLazyStockProposal, VendorStock, VendorStockError, VendorStockItem, VendorStockProposal,
    VendorStockReceipt, VendorStockWithdrawal,
};
