//! Pinned Player_Spells equipment-set deltas preserve insertion order and use
//! the last remaining set item as surrogate when unwielding.
use super::*;
use crate::{ItemExperienceRegistryChange, PreparedGeneratorEnchantment, PreparedItemExperience};
fn candidate<'a>(
    magic: &crate::magic::Magic,
    patches: &'a mut Vec<ItemExperienceRegistryChange>,
    actor: EntityId,
) -> Result<&'a mut ItemExperienceRegistryChange, E> {
    let index = if let Some(index) = patches.iter().position(|p| p.actor == actor) {
        index
    } else {
        let registry = magic.registry(actor).ok_or(E::InvalidState)?;
        patches.push(ItemExperienceRegistryChange {
            actor,
            capacity: registry.capacity(),
            before_revision: registry.revision(),
            before: registry.entries().to_vec(),
            after_revision: registry.revision(),
            after: registry.entries().to_vec(),
            events: vec![],
        });
        patches.len() - 1
    };
    Ok(&mut patches[index])
}
pub(super) fn remove_spell(
    magic: &crate::magic::Magic,
    patches: &mut Vec<ItemExperienceRegistryChange>,
    target: EntityId,
    spell: u32,
    caster: Option<u32>,
    set: Option<u32>,
) -> Result<(), E> {
    let patch = candidate(magic, patches, target)?;
    let selected = patch
        .after
        .iter()
        .find(|e| {
            e.spell == spell
                && caster.is_none_or(|id| e.caster == id)
                && set.is_none_or(|id| e.spec.set_id == Some(id))
        })
        .map(|e| (e.spell, e.spec.layer));
    let Some(selected) = selected else {
        return Ok(());
    };
    let mut registry =
        EnchantmentRegistry::restore(patch.capacity, patch.after_revision, patch.after.clone())
            .map_err(|_| E::InvalidState)?;
    registry.remove(&[selected]).map_err(|_| E::InvalidState)?;
    patch.after_revision = registry.revision();
    patch.after = registry.into_entries();
    patch.events.push(crate::MagicEvent::EnchantmentsRemoved {
        actor: target,
        entries: vec![selected],
    });
    Ok(())
}
pub(super) fn add_spell(
    magic: &crate::magic::Magic,
    patches: &mut Vec<ItemExperienceRegistryChange>,
    entry: &PreparedGeneratorEnchantment,
    now: f64,
) -> Result<(), E> {
    let patch = candidate(magic, patches, entry.target)?;
    let mut registry =
        EnchantmentRegistry::restore(patch.capacity, patch.after_revision, patch.after.clone())
            .map_err(|_| E::InvalidState)?;
    let added = registry
        .add(entry.entry.clone(), now, true)
        .map_err(|_| E::InvalidState)?;
    patch.events.push(crate::MagicEvent::Enchantment {
        actor: entry.target,
        entry: added.entry,
    });
    patch.after_revision = registry.revision();
    patch.after = registry.into_entries();
    Ok(())
}
fn tier(items: &[&PreparedItemExperience]) -> Result<Vec<PreparedGeneratorEnchantment>, E> {
    let Some(first) = items.first() else {
        return Ok(vec![]);
    };
    let set = first.set.as_ref().ok_or(E::InvalidState)?;
    let level = if first.set_uses_item_levels {
        items.iter().try_fold(0u32, |sum, item| {
            let level = item
                .experience
                .map(|xp| xp.level())
                .transpose()
                .map_err(|_| E::InvalidState)?
                .unwrap_or(0);
            sum.checked_add(level).ok_or(E::Overflow)
        })?
    } else {
        items.len() as u32
    };
    Ok(set
        .tiers
        .range(..=level)
        .next_back()
        .map_or_else(Vec::new, |(_, entries)| entries.clone()))
}
#[expect(
    clippy::too_many_arguments,
    reason = "pure source transition carries ordered before/after equipment, changed caster, and explicit time"
)]
pub(super) fn update_set(
    magic: &crate::magic::Magic,
    patches: &mut Vec<ItemExperienceRegistryChange>,
    actor: EntityId,
    changed: &PreparedItemExperience,
    before: &[PreparedItemExperience],
    after: &[PreparedItemExperience],
    equipping: bool,
    now: f64,
) -> Result<(), E> {
    let Some(set) = &changed.set else {
        return Ok(());
    };
    let members = |items: &[PreparedItemExperience]| {
        items
            .iter()
            .filter(|p| p.set.as_ref().is_some_and(|s| s.id == set.id))
            .cloned()
            .collect::<Vec<_>>()
    };
    let old = members(before);
    let new = members(after);
    let old = tier(&old.iter().collect::<Vec<_>>())?;
    let active = tier(&new.iter().collect::<Vec<_>>())?;
    for entry in old
        .iter()
        .filter(|entry| !active.iter().any(|e| e.entry.spell == entry.entry.spell))
    {
        remove_spell(magic, patches, actor, entry.entry.spell, None, Some(set.id))?;
    }
    let caster = if equipping { Some(changed) } else { new.last() };
    for entry in active
        .iter()
        .filter(|entry| !old.iter().any(|e| e.entry.spell == entry.entry.spell))
    {
        let caster = caster.ok_or(E::InvalidState)?;
        let mut entry = entry.clone();
        if entry.target != actor {
            entry.target = caster.item;
        }
        entry.entry.caster = caster.item.0;
        add_spell(magic, patches, &entry, now)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
