use super::*;
use bace_gameplay_api::{AttributeId, ProgressionTarget, TraitDetails, VitalId};
impl Kernel {
    pub(super) fn death_vital_maxima(
        &self,
        actor: EntityId,
        registry: &bace_magic::EnchantmentRegistry,
        formulas: [bace_character::VitalFormula; 3],
        gear_health: u32,
    ) -> Result<[u32; 3], E> {
        let character = self.characters.get(actor).ok_or(E::MissingAssets)?;
        let mut attributes = [0; 6];
        for (index, attribute) in attributes.iter_mut().enumerate() {
            let p = character
                .projection(ProgressionTarget::Attribute(
                    AttributeId::try_from(index as u32 + 1).map_err(|_| E::Invalid)?,
                ))
                .ok_or(E::MissingAssets)?;
            let Some(TraitDetails::Attribute { starting_value }) = p.details else {
                return Err(E::MissingAssets);
            };
            let base = starting_value
                .checked_add(u32::from(p.ranks))
                .ok_or(E::Invalid)?;
            let multiplier =
                bace_magic::enchantment_modifiers(registry, 1 | 0x4000, index as u32 + 1)
                    .iter()
                    .fold(1f32, |v, e| v * e.spec.value);
            let additive =
                bace_magic::enchantment_modifiers(registry, 1 | 0x8000, index as u32 + 1)
                    .iter()
                    .try_fold(0i32, |v, e| v.checked_add(e.spec.value as i32))
                    .ok_or(E::Invalid)?;
            *attribute = bace_character::project_attribute_value(base, multiplier, additive)
                .map_err(|_| E::Invalid)?;
        }
        let vitae = registry
            .entries()
            .iter()
            .find(|e| e.spell == 666)
            .map_or(1., |e| e.spec.value.min(1.));
        let mut maxima = [0; 3];
        for (i, (id, formula)) in [1, 3, 5].into_iter().zip(formulas).enumerate() {
            let p = character
                .projection(ProgressionTarget::Vital(
                    VitalId::try_from(id).map_err(|_| E::Invalid)?,
                ))
                .ok_or(E::MissingAssets)?;
            let Some(TraitDetails::Vital { starting_value, .. }) = p.details else {
                return Err(E::MissingAssets);
            };
            let multiplier = bace_magic::enchantment_modifiers(registry, 2 | 0x4000, id)
                .iter()
                .fold(1f32, |v, e| v * e.spec.value);
            let additive = bace_magic::enchantment_modifiers(registry, 2 | 0x8000, id)
                .iter()
                .fold(0f32, |v, e| v + e.spec.value);
            let base_bonus = if i == 0 {
                let enlightenment = self
                    .characters
                    .native_services(actor)
                    .ok_or(E::MissingAssets)?
                    .enlightenment;
                enlightenment
                    .checked_mul(2)
                    .and_then(|v| v.checked_add(gear_health))
                    .ok_or(E::Invalid)?
            } else {
                0
            };
            maxima[i] = bace_character::project_vital_values(bace_character::VitalValueInputs {
                formula,
                starting_value,
                ranks: u32::from(p.ranks),
                current_attributes: attributes,
                base_bonus,
                multiplier,
                vitae,
                additive,
            })
            .map_err(|_| E::Invalid)?
            .maximum;
        }
        Ok(maxima)
    }
}
