//! GDLE 353cbab physical combat. Prepared immutable inputs retain source-owned
//! enchantment/equipment resolution; random draws and accepted geometry explicit.
mod damage;
mod maneuver;
mod policy;
pub use damage::{
    PhysicalContact, PhysicalDamageInput, finish_physical_contact, imbue,
    physical_dirty_spell_dependencies, physical_dirty_spells, physical_rating_modifier,
    prepare_physical_contact, resolve_physical,
};
pub use maneuver::{
    SelectedPhysicalAttack, attack_speed, fallback_melee_motion, missile_attack_speed,
    select_melee, validate_physical_profile,
};
pub use policy::{
    hostile_pk_allowed, physical_permission, physical_permission_with_status, pk_action_allowed,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalError {
    InvalidInput,
    MissingWeapon,
    MissingSkill,
    MissingMotion,
    MissingBody,
    Forbidden,
    Overflow,
}

mod trajectory;
pub use trajectory::missile_velocity;
mod timing;
pub use timing::{MissileLifetime, MissileLifetimeEvent, charge_seconds, missile_settings};

mod stamina;
pub use stamina::{AttackStaminaInput, attack_stamina, defense_stamina};

mod armor;
pub use armor::{clothing_coverage, decreasing_only, enchanted};

mod npc_missile;
pub use npc_missile::{npc_missile_range, npc_reload_speed};
