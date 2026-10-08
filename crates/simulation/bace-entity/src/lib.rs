//! Runtime entity state and read projections.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod actor;
mod combatant;

pub use actor::Actor;
pub use combatant::{Combatant, CombatantError, CombatantProfile};

mod properties;
pub use properties::{
    EntityProperties, PropertyChange, PropertyError, PropertyFamily, PropertyValue,
};

mod vitals;
pub use vitals::{EntityVital, VitalMutation, VitalMutationResult, VitalPool};
