//! Attacks, damage, defenses and PvP rules.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

pub mod death_messages;
mod melee;
mod rewards;
mod skill;

pub use melee::{AttackError, AttackImpact, MeleeAttack, Strike};
pub use rewards::{DamageShare, RewardError, kill_rewards};
pub use skill::{SkillError, skill_chance};

pub mod specialization;

pub mod physical;

pub mod preparation;
