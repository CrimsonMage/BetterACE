//! Exact player snapshot and lease bound to one staff gag command receipt.
use crate::{CharacterLease, SaveAck, SaveSnapshot, StoredAggregate};
#[derive(Clone, Debug, PartialEq)]
pub struct StaffGagOperation {
    pub operation_id: [u8; 16],
    pub issuer_account: u64,
    pub lease: CharacterLease,
    pub enabled: bool,
    pub unix_seconds: f64,
    pub snapshot: SaveSnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffGagReceipt {
    pub operation_id: [u8; 16],
    pub acknowledgement: SaveAck,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfflineStaffPlayer {
    pub lease: CharacterLease,
    pub snapshot: StoredAggregate,
}
