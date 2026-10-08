//! Quest state, counters and contracts.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod eligibility;
mod predicates;

pub use eligibility::{
    QuestDefinition, QuestEligibility, QuestProgress, QuestTimeError, next_solve,
};
pub use predicates::{has_bits, has_no_bits, has_solves};

mod registry;
pub use registry::{QuestChange, QuestMutation, QuestRegistry, QuestRegistryError, quest_key};

mod contracts;
pub use contracts::{ContractChange, ContractError, ContractRegistry, ContractState};
