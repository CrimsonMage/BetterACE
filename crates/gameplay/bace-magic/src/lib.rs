//! Casting, spells and enchantments.
//!
//! See ARCHITECTURE.md for dependency and implementation contracts.

mod casting;
mod effects;
mod enchantments;
mod formulas;
mod projectiles;
pub use casting::{
    CastDriver, CastError, CastGesture, CastObservation, CastPreparation, CastSignal, CastStage,
};
pub use effects::{
    DispelSpec, EffectError, EnchantmentSpec, MagicSchool, PortalEffect, PreparedSpell,
    ProjectileShape, ProjectileSpec, SpellEffect, TransferChange, Vital, VitalChange, VitalState,
    boost, transfer,
};
pub use enchantments::{
    EnchantmentApplication, EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry,
    EnchantmentStack, PreparedEnchantment, RegistryError, VitaeMutation,
};
pub use formulas::{MagicFormulaError, cast_chance, component_burn_rate, mana_cost, resisted};
pub use projectiles::{
    ProjectileLayoutError, life_projectile_drain, projectile_origins, projectile_pre_offset,
    spread_angle_step,
};

mod enchantment_audit;
pub use enchantment_audit::{EquippedSpellOwner, EquippedSpellSet, audit_equipped_spells};

mod recovery;
pub use recovery::{CastRecovery, CastRecoveryClock};

mod components;
pub use components::foci_formula;

mod trajectory;
pub use trajectory::projectile_velocity;

mod quality_modifiers;
pub use quality_modifiers::enchantment_modifiers;

mod launch;
pub use launch::{ProjectileActorFrame, ProjectileLaunch, gdle_single_projectile};
mod projectile_lifetime;
pub use projectile_lifetime::{ProjectileLifeAction, SpellProjectileLifetime};

mod physical_quality;
pub use physical_quality::{enchant_body_quality, enchant_physical_quality};

mod multishot;
pub use multishot::gdle_projectiles;
mod strike;
pub use strike::{ace_strike_projectiles, ace_strike_velocity};
mod damage_profile;
pub use damage_profile::{
    MagicDamageProfile, MagicDamageRolls, MagicProcItem, MagicWand, magic_cloak_projectile_skill,
    magic_item_skill,
};

mod damage_preparation;
pub use damage_preparation::{
    MagicDamageEquipment, MagicDamagePreparation, magic_rating_ids, magic_ratings,
    prepare_magic_damage_profile, prepare_magic_wand, prepare_magic_wand_properties,
    spell_formula_level, spell_projectile_intensity,
};

mod damage;
pub use damage::{
    MagicDamageInput, MagicDamageResult, magic_damage_after_mitigation,
    magic_damage_before_mitigation, magic_resistance_index, validate_magic_damage_profile,
};

mod procs;
pub use procs::{
    MagicCloak, MagicCloakEffect, MagicCloakInput, MagicCloakResult, MagicItemProc,
    MagicProcPolicy, MagicSigil, magic_cloak_proc, magic_sigil_procs,
};

mod vital_effects;
pub use vital_effects::{gdle_heal, gdle_natural_resistance, gdle_transfer};

mod periodic_native;
pub use periodic_native::{
    NativePeriodicEffect, NativePeriodicKind, gdle_periodic_batches, gdle_periodic_damage,
};

mod native;
pub use native::{
    NativeProjectileMotion, NativeSpellDefinition, NativeSpellError, NativeSpellHeader,
    prepare_native_spell,
};

mod proc_references;
pub use proc_references::required_proc_spells;
