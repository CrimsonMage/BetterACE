//! Lossless stored registry conversion. Call on bounded preparation/save adapter
//! capacity; live gameplay and dirty revisions remain owned by simulation.
use bace_gameplay_api::EnchantmentProjection;
use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
    RegistryError,
};
use bace_storage_codec::{
    CorpseSaveV3, FrozenEnchantmentV1, HouseSaveV3, ItemSaveV4, ItemSaveV5, PlayerSaveV6,
    SaveCodecError,
};

/// Prepared from the pinned spell table, not from stored/client numeric hints.
#[derive(Clone, Copy, Debug)]
pub struct EnchantmentDefinition {
    pub school: MagicSchool,
    pub is_set_spell: bool,
    pub is_level8_aura: bool,
    pub category: u16,
    pub power: u32,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    pub stat_type: u32,
    pub stat_key: u32,
    pub beneficial: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EnchantmentSaveError {
    #[error(transparent)]
    Save(#[from] SaveCodecError),
    #[error("missing prepared enchantment spell {0}")]
    MissingSpell(u32),
    #[error("invalid enchantment registry: {0:?}")]
    Registry(RegistryError),
}

pub fn freeze_enchantment(
    entry: &EnchantmentEntry,
) -> Result<FrozenEnchantmentV1, EnchantmentSaveError> {
    let result = FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: entry.metadata.enchantment_category,
        spell_id: i32::try_from(entry.spell)
            .map_err(|_| SaveCodecError::Invalid("enchantment spell width"))?,
        layer_id: entry.spec.layer,
        has_spell_set_id: entry.metadata.has_spell_set_id,
        spell_category: u32::from(entry.spec.category),
        power_level: entry.spec.power,
        start_time: entry.start_time,
        duration: entry.spec.duration,
        caster_object_id: entry.caster,
        degrade_modifier: entry.metadata.degrade_modifier,
        degrade_limit: entry.metadata.degrade_limit,
        last_time_degraded: entry.metadata.last_time_degraded,
        stat_mod_type: entry.spec.stat_type,
        stat_mod_key: entry.spec.stat_key,
        stat_mod_value: entry.spec.value,
        spell_set_id: entry.metadata.spell_set_id,
    };
    result.validate()?;
    Ok(result)
}

pub fn restore_enchantment(
    saved: &FrozenEnchantmentV1,
    definition: Option<EnchantmentDefinition>,
) -> Result<EnchantmentEntry, EnchantmentSaveError> {
    saved.validate()?;
    let cooldown = saved.spell_category == 0x8000;
    let definition = if cooldown {
        None
    } else {
        Some(definition.ok_or(EnchantmentSaveError::MissingSpell(saved.spell_id as u32))?)
    };
    Ok(EnchantmentEntry {
        spell: saved.spell_id as u32,
        caster: saved.caster_object_id,
        // Cooldowns have no spell-table school and are excluded from dispels.
        school: definition.map_or(MagicSchool::Item, |d| d.school),
        spec: EnchantmentSpec {
            category: saved.spell_category as u16,
            power: saved.power_level,
            duration: saved.duration,
            layer: saved.layer_id,
            stat_type: saved.stat_mod_type,
            stat_key: saved.stat_mod_key,
            value: saved.stat_mod_value,
            beneficial: saved.stat_mod_type & 0x02000000 != 0,
            set_id: saved.has_spell_set_id.then_some(saved.spell_set_id as u32),
        },
        start_time: saved.start_time,
        is_set_spell: definition.is_some_and(|d| d.is_set_spell),
        is_level8_aura: definition.is_some_and(|d| d.is_level8_aura),
        metadata: EnchantmentMetadata {
            enchantment_category: saved.enchantment_category,
            has_spell_set_id: saved.has_spell_set_id,
            degrade_modifier: saved.degrade_modifier,
            degrade_limit: saved.degrade_limit,
            last_time_degraded: saved.last_time_degraded,
            spell_set_id: saved.spell_set_id,
        },
    })
}

/// Pinned Enchantment(target, entry) reconstructs ordinary spell metadata for
/// wire output while cooldowns retain stored metadata (power/set ID default 0).
pub fn prepare_enchantment_projection(
    entry: &EnchantmentEntry,
    definition: Option<EnchantmentDefinition>,
) -> Result<EnchantmentProjection, EnchantmentSaveError> {
    freeze_enchantment(entry)?;
    let cooldown = entry.spec.category == 0x8000;
    let definition = if cooldown {
        None
    } else {
        Some(definition.ok_or(EnchantmentSaveError::MissingSpell(entry.spell))?)
    };
    let result = EnchantmentProjection {
        spell_id: entry.spell as u16,
        layer: entry.spec.layer,
        category: definition.map_or(entry.spec.category, |d| d.category),
        power: definition.map_or(0, |d| d.power),
        start_time: entry.start_time,
        duration: entry.spec.duration,
        caster_id: entry.caster,
        degrade_modifier: definition
            .map_or(entry.metadata.degrade_modifier, |d| d.degrade_modifier),
        degrade_limit: definition.map_or(entry.metadata.degrade_limit, |d| d.degrade_limit),
        last_time_degraded: if cooldown {
            entry.metadata.last_time_degraded
        } else {
            0.0
        },
        stat_type: definition.map_or(entry.spec.stat_type, |d| {
            d.stat_type | if d.beneficial { 0x02000000 } else { 0 }
        }),
        stat_key: definition.map_or(entry.spec.stat_key, |d| d.stat_key),
        stat_value: entry.spec.value,
        spell_set_id: if cooldown {
            0
        } else {
            entry.metadata.spell_set_id as u32
        },
    };
    if !result.degrade_modifier.is_finite() || !result.degrade_limit.is_finite() {
        return Err(SaveCodecError::Invalid("prepared enchantment metadata").into());
    }
    Ok(result)
}

pub trait SavedEnchantments {
    fn enchantments(&self) -> &[FrozenEnchantmentV1];
    fn revision(&self) -> u64;
    fn validate_save(&self) -> Result<(), SaveCodecError>;
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64);
}
impl SavedEnchantments for PlayerSaveV6 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.previous.enchantments
    }
    fn revision(&self) -> u64 {
        self.previous.player.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.previous.enchantments = entries;
        self.previous.player.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for ItemSaveV4 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.previous.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.previous.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for ItemSaveV5 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for CorpseSaveV3 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.previous.corpse.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.previous.corpse.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for bace_storage_codec::CorpseSaveV4 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.corpse.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.corpse.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for bace_storage_codec::CorpseSaveV5 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.corpse.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.corpse.entity.mutation_revision = revision;
    }
}
impl SavedEnchantments for HouseSaveV3 {
    fn enchantments(&self) -> &[FrozenEnchantmentV1] {
        &self.enchantments
    }
    fn revision(&self) -> u64 {
        self.previous.entity.mutation_revision
    }
    fn validate_save(&self) -> Result<(), SaveCodecError> {
        self.validate()
    }
    fn install_enchantments(&mut self, entries: Vec<FrozenEnchantmentV1>, revision: u64) {
        self.enchantments = entries;
        self.previous.entity.mutation_revision = revision;
    }
}

pub fn restore_saved_enchantments(
    saved: &impl SavedEnchantments,
    capacity: usize,
    mut resolve: impl FnMut(u32) -> Option<EnchantmentDefinition>,
) -> Result<EnchantmentRegistry, EnchantmentSaveError> {
    saved.validate_save()?;
    if saved.enchantments().len() > capacity || !(1..=4096).contains(&capacity) {
        return Err(EnchantmentSaveError::Registry(RegistryError::Capacity));
    }
    let entries = saved
        .enchantments()
        .iter()
        .map(|entry| restore_enchantment(entry, resolve(entry.spell_id as u32)))
        .collect::<Result<Vec<_>, _>>()?;
    EnchantmentRegistry::restore(capacity, saved.revision(), entries)
        .map_err(|(error, _)| EnchantmentSaveError::Registry(error))
}

/// Caller supplies the aggregate revision after joining all subsystem changes.
/// A registry-local revision is never treated as a database CAS version.
pub fn snapshot_enchantments(
    saved: &mut impl SavedEnchantments,
    registry: &EnchantmentRegistry,
    revision: u64,
) -> Result<(), EnchantmentSaveError> {
    saved.validate_save()?;
    let entries = registry
        .entries()
        .iter()
        .map(freeze_enchantment)
        .collect::<Result<Vec<_>, _>>()?;
    bace_storage_codec::validate_enchantments_v1(&entries)?;
    if revision < saved.revision()
        || entries != saved.enchantments() && revision == saved.revision()
    {
        return Err(SaveCodecError::Invalid("enchantment snapshot revision must advance").into());
    }
    saved.install_enchantments(entries, revision);
    Ok(())
}
