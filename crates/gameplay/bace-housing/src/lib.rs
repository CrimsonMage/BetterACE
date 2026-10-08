//! Ownership, rent, permissions, hooks and storage.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.
mod lifecycle;
pub use lifecycle::{
    HouseGuest, HousePayment, HouseView, HousingActor, HousingProposal, HousingState, PaymentItem,
    PaymentUse, PermissionChange, PurchaseRules, RentScheduler, propose_abandon, propose_due_rent,
    propose_eviction, propose_permission, propose_purchase, propose_rent, rent_window,
};

pub use lifecycle::HousingReason;
