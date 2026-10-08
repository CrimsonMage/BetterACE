//! Immutable DAT formulas and live authoritative modifiers share one vital owner.
use super::*;
use bace_character::{CharacterProgression, VitalFormula, VitalValueInputs};
use bace_gameplay_api::{AttributeId, ProgressionTarget, TraitDetails, VitalId};
#[derive(Clone, Debug)]
pub struct PreparedVitalInputs {
    pub formulas: [VitalFormula; 3],
    pub equipped_health: Vec<(EntityId, u32)>,
}
impl Kernel {
    pub(super) fn project_live_maxima(
        &self,
        actor: EntityId,
    ) -> Result<Option<[u32; 3]>, crate::SkillRefreshError> {
        let Some(inputs) = self.vital_inputs.get(&actor) else {
            return Ok(None);
        };
        let gear = inputs.equipped_health.iter().try_fold(0u32, |sum, (item, health)| {
            if self.inventory.item(*item).is_some_and(|i| matches!(i.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)) {
                sum.checked_add(*health)
            } else { Some(sum) }
        }).ok_or(crate::SkillRefreshError::InvalidInput)?;
        project_maxima(
            self.characters
                .get(actor)
                .ok_or(crate::SkillRefreshError::MissingActor)?,
            self.magic
                .registry(actor)
                .ok_or(crate::SkillRefreshError::InvalidInput)?,
            inputs.formulas,
            gear,
            self.characters
                .native_services(actor)
                .ok_or(crate::SkillRefreshError::InvalidInput)?
                .enlightenment,
        )
        .map(Some)
    }
}
pub(super) fn project_maxima(
    character: &CharacterProgression,
    registry: &bace_magic::EnchantmentRegistry,
    formulas: [VitalFormula; 3],
    gear_health: u32,
    enlightenment: u32,
) -> Result<[u32; 3], crate::SkillRefreshError> {
    use crate::SkillRefreshError as E;
    let mut attributes = [0; 6];
    for (index, value) in attributes.iter_mut().enumerate() {
        let key = index as u32 + 1;
        let p = character
            .projection(ProgressionTarget::Attribute(
                AttributeId::try_from(key).map_err(|_| E::InvalidInput)?,
            ))
            .ok_or(E::MissingSkill)?;
        let Some(TraitDetails::Attribute { starting_value }) = p.details else {
            return Err(E::InvalidInput);
        };
        let base = starting_value
            .checked_add(u32::from(p.ranks))
            .ok_or(E::InvalidInput)?;
        let (multiplier, additive) = super::skill_modifiers::modifiers(registry, 1, key)?;
        *value = bace_character::project_attribute_value(base, multiplier, additive)
            .map_err(|_| E::InvalidInput)?;
    }
    let vitae = registry
        .entries()
        .iter()
        .find(|e| e.spell == 666)
        .map_or(1.0, |e| e.spec.value.min(1.0));
    let mut maxima = [0; 3];
    for (index, (key, formula)) in [1, 3, 5].into_iter().zip(formulas).enumerate() {
        let p = character
            .projection(ProgressionTarget::Vital(
                VitalId::try_from(key).map_err(|_| E::InvalidInput)?,
            ))
            .ok_or(E::MissingSkill)?;
        let Some(TraitDetails::Vital { starting_value, .. }) = p.details else {
            return Err(E::InvalidInput);
        };
        let multiplier = bace_magic::enchantment_modifiers(registry, 2 | 0x4000, key)
            .iter()
            .fold(1.0, |value, e| value * e.spec.value);
        let additive = bace_magic::enchantment_modifiers(registry, 2 | 0x8000, key)
            .iter()
            .fold(0.0, |value, e| value + e.spec.value);
        let base_bonus = if index == 0 {
            enlightenment
                .checked_mul(2)
                .and_then(|v| v.checked_add(gear_health))
                .ok_or(E::InvalidInput)?
        } else {
            0
        };
        maxima[index] = bace_character::project_vital_values(VitalValueInputs {
            formula,
            starting_value,
            ranks: u32::from(p.ranks),
            current_attributes: attributes,
            base_bonus,
            multiplier,
            vitae,
            additive,
        })
        .map_err(|_| E::InvalidInput)?
        .maximum;
    }
    Ok(maxima)
}
