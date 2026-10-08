//! Entry projections consume a complete accepted player checkpoint and explicit
//! live enchantment projections. Object appearance/physics are prepared separately.
mod description;
mod visibility;
pub use description::{EntryInventoryItem, prepare_player_description, prepare_titles};
#[cfg(test)]
mod tests;

mod appearance;
mod object;
pub use appearance::{
    EntryAppearanceAssets, PlayerAppearanceOptions, prepare_item_model, prepare_player_model,
};
pub(crate) use object::prepare_vendor_game_data;
pub use object::{EntryObjectState, PreparedEntryModel, prepare_entry_object};

mod appearance_assets;
pub use appearance_assets::PreparedEntryAppearanceAssets;
mod attachments;
pub use attachments::{EntryItemAttachment, prepare_entry_attachments};
mod plan;
pub use plan::{
    PlayerEntryInput, PreparedEntryPossession, PreparedPlayerEntry, prepare_player_entry,
};

mod login_instance;
pub use login_instance::instance_from_login;

mod state;
pub use state::{physics_sequences, prepare_player_entry_state};

pub use appearance::prepare_creature_model;
pub use state::initial_physics_state;
