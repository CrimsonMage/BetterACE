//! Awareness, behavior, navigation intent and combat pets.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod awareness;
pub use awareness::{Awareness, AwarenessError, TargetCandidate};

mod leash;
pub use leash::{MonsterLeash, ReturnHome};

mod spellcasting;
pub use spellcasting::{MonsterCastError, MonsterSpell, MonsterSpellcasting};
mod pets;
pub use pets::{
    CombatPetLifetime, PetTarget, PetUseError, PetUseRequirements, PetUser, combat_pet_target,
};
