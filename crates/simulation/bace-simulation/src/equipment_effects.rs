//! Immutable spell, set, mana and item-XP companions to an equipment operation.
//! Prepared source inputs are revision-fenced; adoption belongs to the same
//! durable inventory transaction as the accepted locations and physical profile.
use bace_types::EntityId;
pub type EquipmentActivation = bace_inventory::ActivationMessage;
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedEquipmentItemEffects {
    pub mana: Option<crate::EquipmentManaItem>,
    pub item: EntityId,
    pub before_revision: u64,
    /// All non-proc spell-book enchantments, including the item's SpellDID.
    pub removals: Vec<crate::PreparedGeneratorEnchantment>,
    /// Source activation excludes proc entries and SpellDID.
    pub activations: Vec<crate::PreparedGeneratorEnchantment>,
    pub experience: crate::PreparedItemExperience,
    pub current_mana: Option<i32>,
    pub gear_health: Option<u32>,
    pub affecting: Option<bool>,
    pub activation: bace_inventory::ActivationRequirements,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedEquipmentEffectInputs {
    pub actor: EntityId,
    pub before_revision: u64,
    /// Complete current and candidate equipment; changed non-equipped objects
    /// may be included when a formerly equipped object leaves the actor.
    pub items: Vec<PreparedEquipmentItemEffects>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentItemPropertyChange {
    pub item: EntityId,
    pub before_revision: u64,
    pub after_revision: u64,
    pub mana_before: Option<i32>,
    pub mana_after: Option<i32>,
    pub affecting_before: Option<bool>,
    pub affecting_after: Option<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentItemExperienceChange {
    pub item: EntityId,
    pub before: Option<crate::PreparedItemExperience>,
    pub after: Option<crate::PreparedItemExperience>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentEffectsPatch {
    pub mana: Vec<(
        EntityId,
        Option<crate::EquipmentManaItem>,
        Option<crate::EquipmentManaItem>,
    )>,
    pub actor: EntityId,
    pub before_revision: u64,
    pub registries: Vec<crate::ItemExperienceRegistryChange>,
    pub item_experience: Vec<EquipmentItemExperienceChange>,
    pub properties: Vec<EquipmentItemPropertyChange>,
    pub minimum_vital_maxima: Option<[u32; 3]>,
    pub activation_messages: Vec<EquipmentActivation>,
}
impl EquipmentEffectsPatch {
    pub fn registry(
        &self,
        actor: EntityId,
    ) -> Result<Option<bace_magic::EnchantmentRegistry>, bace_magic::RegistryError> {
        self.registries
            .iter()
            .find(|p| p.actor == actor)
            .map(|p| {
                bace_magic::EnchantmentRegistry::restore(
                    p.capacity,
                    p.after_revision,
                    p.after.clone(),
                )
                .map_err(|(error, _unadopted_copy)| error)
            })
            .transpose()
    }
}

/// Player_Inventory.TryActivateSpells: eligible items spend one mana even when
/// every attempted spell reports false; its isAffecting local starts true.
pub(crate) fn activation_properties(
    mana: Option<i32>,
    affecting: Option<bool>,
    allowed: bool,
) -> (Option<i32>, Option<bool>, bool) {
    if !allowed || mana.is_none_or(|value| value <= 0) {
        return (mana, affecting, false);
    }
    if mana == Some(1) {
        return (Some(0), affecting, false);
    }
    (mana.map(|value| value - 1), Some(true), true)
}

#[cfg(test)]
mod tests;
