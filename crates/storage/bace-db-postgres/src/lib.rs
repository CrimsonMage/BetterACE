//! Explicit PostgreSQL persistence. Binary validation belongs to the caller.
mod accounts;
mod content;
mod store;
mod writes;
pub use content::ContentStatus;
pub use store::{PgStore, StoreError};
mod inventory;
mod local;
pub use local::initialize_local_database;
mod housing;
mod world_owner;
pub use world_owner::WorldOwner;
mod character_leases;
mod house_maintenance;
mod inventory_load;
mod mapped;
mod offline;
mod owned_saves;
mod ownership;
mod placements;
mod players;
mod random_key;
mod snapshot_identity;
mod world_items;
mod world_tree;

mod native_content;

mod npc_workflow;
pub use npc_workflow::StoredNpcSourceHead;

mod allegiance;

mod account_admin;

mod allegiance_load;

mod social_identity;
pub use social_identity::{PlayerIdentity, PlayerIdentityQuery};

mod login_instances;

mod reserved_writer;

mod player_creation_conflict;
pub use player_creation_conflict::PlayerCreationConflict;

mod staff_gags;

mod vendor_stock;

mod constructed_promotion;

mod account_bans;
