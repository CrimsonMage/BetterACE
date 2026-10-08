//! Recipes, salvage and crafting mutations.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.
mod chance;
mod recipe;
mod salvage;
mod script_table;
mod scripts;
mod types;
pub use scripts::{apply_recipe_script, recipe_script_supported};

pub use chance::{ChanceInput, TinkerChance, material_modifier, tinker_chance};
pub use recipe::{
    Confirmation, CraftContext, CraftItem, CraftPropertyUpdate, CraftProposal, PreparedRecipe,
    RecipeBranch, propose_craft, quote_craft, roll_tinker,
};
pub use salvage::{
    SalvageBag, SalvageInput, SalvageProposal, SalvageRequest, SalvageResult, SalvageSkills,
    SalvageTool, UnsupportedSalvage, propose_salvage, salvage_amount,
};
pub use types::{
    CraftError, Mutation, MutationKind, Participant, PropertyKey, PropertyKind, PropertyValue,
    Requirement, RequirementComparison,
};

mod native_recipe;
pub use native_recipe::{NativeRecipe, NativeRecipeRows, prepare_native_recipe};

mod confirmation;
pub use confirmation::propose_confirmed_craft;

mod tinker_selection;
pub use tinker_selection::{is_foolproof_tinker, select_new_tinkering_recipe};

mod messages;
pub use messages::{tinker_confirmation_text, tinker_result_text};
