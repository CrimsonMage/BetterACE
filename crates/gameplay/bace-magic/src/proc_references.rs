//! Dependencies of the already prepared active equipment proc profile. These IDs
//! authorize no casts; a cold definition closure must exist before profile adoption.
use crate::{EffectError, MagicCloakEffect, MagicDamageProfile};
use bace_gameplay_api::weapon_combat::PhysicalCombatProfile;
pub fn required_proc_spells(
    magic: &MagicDamageProfile,
    physical: &PhysicalCombatProfile,
) -> Result<Vec<u32>, EffectError> {
    let mut ids = std::collections::BTreeSet::new();
    if let Some(cloak) = magic.cloak
        && let MagicCloakEffect::Spell(id) = cloak.effect
    {
        ids.insert(id);
    }
    ids.extend(magic.sigils.iter().map(|s| s.spell));
    for weapon in [
        &physical.main,
        &physical.offhand,
        &physical.launcher,
        &physical.ammunition,
        &physical.gloves,
        &physical.boots,
    ]
    .into_iter()
    .flatten()
    {
        if !weapon.proc_chance.is_finite() {
            return Err(EffectError::InvalidState);
        }
        // GDLE InqFloatQuality defaults raw=false: an explicitly stored zero
        // ProcSpellRate can become positive through live float enchantments.
        if let Some(id) = weapon.proc_spell.filter(|id| *id != 0) {
            ids.insert(id);
        }
    }
    if ids.len() > 16 || ids.iter().any(|id| *id == 0 || *id > 65535) {
        return Err(EffectError::InvalidState);
    }
    Ok(ids.into_iter().collect())
}
