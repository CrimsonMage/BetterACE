//! Bounded dirty-save coordination and opaque persistence contracts.
mod contracts;
mod dirty;
pub use contracts::*;
pub use dirty::*;
mod mapped;
pub use mapped::*;
mod offline;
pub use offline::*;
mod inventory;
pub use inventory::*;
mod players;
pub use players::*;
mod owned_saves;
pub use owned_saves::{InventoryLoadLimits, LoadedInventory, OwnedSaveBatch, StoredInventoryItem};
mod placements;
pub use placements::HouseMaintenanceSnapshot;
pub use placements::LocatedSnapshot;
pub use placements::{
    DurableItemPlace, HouseOwnershipChange, HousingOperation, PlacementChange, PlacementOperation,
    StorageViewFence,
};

mod native_content;
pub use native_content::{MappedContentCandidate, NativeContentCandidate, NativePublication};

mod npc_workflow;
pub use npc_workflow::{NpcStageOperation, NpcWorkflowUpdate, StoredNpcWorkflow};

pub use placements::WorldPlacementOperation;

mod allegiance;
pub use allegiance::{AllegianceCommit, AllegianceOperation, AllegianceWrite, StoredAllegiance};

pub use allegiance::AllegiancePlacementOperation;

mod staff_gags;
pub use staff_gags::{OfflineStaffPlayer, StaffGagOperation, StaffGagReceipt};

mod vendor_stock;
pub use vendor_stock::{
    StoredVendorSource, StoredVendorState, StoredVendorStock, StoredVendorStockForest,
    StoredVendorStockItem, VendorStockOperation, VendorStockWrite,
};

mod constructed_creatures;
pub use constructed_creatures::ConstructedCreaturePromotionOperation;

mod account_bans;
pub use account_bans::{
    AccountBanChange, AccountBanListEntry, AccountBanOperation, AccountBanReceipt,
    AccountBanRecord, AccountBanVerdict,
};
