use super::*;
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeError as E, InventoryFreezeInput};
use bace_content::Property;
use bace_storage_codec::PlayerSaveV6;
fn frozen(
    entries: &[bace_magic::EnchantmentEntry],
) -> Result<Vec<bace_storage_codec::FrozenEnchantmentV1>, E> {
    entries
        .iter()
        .map(crate::enchantment_saves::freeze_enchantment)
        .collect::<Result<_, _>>()
        .map_err(|_| E::Identity)
}
/// Overlay a detached cold candidate used to rebuild the final physical profile.
/// Caller owns the candidate and discards it on error; this never adopts live state.
pub fn overlay_equipment_candidate(
    actor: EntityId,
    state: &mut bace_simulation::OwnedPlayerState,
    patch: &EquipmentEffectsPatch,
) -> Result<(), String> {
    if actor != patch.actor || state.character.progression.revision() != patch.before_revision {
        return Err("equipment candidate identity".into());
    }
    for change in &patch.registries {
        let next = patch
            .registry(change.actor)
            .map_err(|_| "equipment candidate registry")?
            .ok_or("equipment candidate registry missing")?;
        if change.actor == patch.actor {
            let current = state
                .enchantments
                .as_ref()
                .ok_or("equipment player registry missing")?;
            if current.entries() != change.before {
                return Err("equipment player registry before-image".into());
            }
            state.enchantments = Some(next);
        } else {
            let current = state
                .item_enchantments
                .iter_mut()
                .find(|(id, _)| *id == change.actor)
                .ok_or("equipment item registry missing")?;
            if current.1.entries() != change.before {
                return Err("equipment item registry before-image".into());
            }
            current.1 = next;
        }
    }
    if let Some(owner) = &mut state.equipment_mana {
        for (id, before, after) in &patch.mana {
            let position = owner.items.iter().position(|item| item.item == *id);
            if position.map(|i| &owner.items[i]) != before.as_ref() {
                return Err("equipment mana before-image".into());
            }
            if let Some(index) = position {
                owner.items.remove(index);
            }
            if let Some(after) = after {
                owner.items.push(after.clone());
            }
        }
    } else if patch
        .mana
        .iter()
        .any(|(_, before, after)| before.is_some() || after.is_some())
    {
        return Err("equipment mana owner missing".into());
    }
    for change in &patch.item_experience {
        let position = state
            .item_experience
            .iter()
            .position(|p| p.item == change.item);
        if position.map(|i| &state.item_experience[i]) != change.before.as_ref() {
            return Err("equipment item XP before-image".into());
        }
        if let Some(index) = position {
            state.item_experience.remove(index);
        }
        if let Some(next) = &change.after {
            state.item_experience.push(next.clone());
        }
    }
    Ok(())
}
/// Source property/enchantment overlay leaves aggregate revision advancement to
/// the inventory freezer, which joins all changes under one operation revision.
pub fn overlay_equipment_item_sources(
    items: &mut [FrozenInventoryItem],
    patch: &EquipmentEffectsPatch,
) -> Result<(), E> {
    if patch.properties.len() > 128 || patch.registries.len() > 129 {
        return Err(E::Capacity);
    }
    let mut seen = BTreeSet::new();
    if items.iter().any(|item| !seen.insert(item.entity.object_id)) {
        return Err(E::Identity);
    }
    let mut properties = Vec::new();
    seen.clear();
    for change in &patch.properties {
        if !seen.insert(change.item.0)
            || change.after_revision != change.before_revision.checked_add(1).ok_or(E::Overflow)?
        {
            return Err(E::Identity);
        }
        let index = items
            .iter()
            .position(|p| p.entity.object_id == change.item.0)
            .ok_or(E::Identity)?;
        let item = &items[index];
        if item.entity.mutation_revision != change.before_revision
            || item
                .entity
                .state
                .properties
                .ints
                .iter()
                .find(|p| p.id == 107)
                .map(|p| p.value)
                != change.mana_before
            || item
                .entity
                .state
                .properties
                .bools
                .iter()
                .find(|p| p.id == 56)
                .map(|p| p.value)
                != change.affecting_before
        {
            return Err(E::Identity);
        }
        properties.push((index, change));
    }
    let mut registries = Vec::new();
    seen.clear();
    for change in patch.registries.iter().filter(|p| p.actor != patch.actor) {
        if !seen.insert(change.actor.0) {
            return Err(E::Identity);
        }
        let index = items
            .iter()
            .position(|p| p.entity.object_id == change.actor.0)
            .ok_or(E::Identity)?;
        if items[index].enchantments != frozen(&change.before)? {
            return Err(E::Identity);
        }
        registries.push((index, frozen(&change.after)?));
    }
    for (index, change) in properties {
        replace(
            &mut items[index].entity.state.properties.ints,
            107,
            change.mana_after,
        );
        replace(
            &mut items[index].entity.state.properties.bools,
            56,
            change.affecting_after,
        );
    }
    for (index, entries) in registries {
        items[index].enchantments = entries;
    }

    Ok(())
}
/// Used while composing one complete player snapshot at its final revision.
pub fn overlay_equipment_player_registry(
    player: &mut PlayerSaveV6,
    patch: &EquipmentEffectsPatch,
) -> Result<(), E> {
    if player.player.entity.object_id != patch.actor.0 {
        return Err(E::Identity);
    }
    if let Some(change) = patch.registries.iter().find(|p| p.actor == patch.actor) {
        if player.enchantments != frozen(&change.before)? {
            return Err(E::Identity);
        }
        player.enchantments = frozen(&change.after)?;
    }
    Ok(())
}
pub fn freeze_equipment_inventory(
    input: InventoryFreezeInput<'_>,
    patch: &EquipmentEffectsPatch,
) -> Result<bace_persistence::PlacementOperation, E> {
    for property in &patch.properties {
        if !input.proposal.changes.iter().any(|c| {
            c.after.id == property.item
                && c.after.revision == property.after_revision
                && c.before.as_ref().map_or(
                    property.before_revision == 0 && c.after.revision == 1,
                    |before| before.revision == property.before_revision,
                )
        }) {
            return Err(E::Identity);
        }
    }
    for registry in patch.registries.iter().filter(|r| r.actor != patch.actor) {
        if !input.proposal.changes.iter().any(|change| {
            change.after.id == registry.actor
                && change.before.as_ref().is_some_and(|before| {
                    before.revision.checked_add(1) == Some(change.after.revision)
                })
        }) {
            return Err(E::Identity);
        }
    }
    let player = input
        .other_snapshots
        .iter()
        .find(|s| s.object_id == patch.actor.0)
        .ok_or(E::Identity)?;
    let player = PlayerSaveV6::decode(&player.bytes)?;
    if player.player.entity.mutation_revision
        != patch.before_revision.checked_add(1).ok_or(E::Overflow)?
    {
        return Err(E::Identity);
    }
    if let Some(change) = patch.registries.iter().find(|p| p.actor == patch.actor)
        && player.enchantments != frozen(&change.after)?
    {
        return Err(E::Identity);
    }
    let mut items = input.items.to_vec();
    overlay_equipment_item_sources(&mut items, patch)?;
    crate::game_inventory::freeze_inventory(InventoryFreezeInput {
        items: &items,
        ..input
    })
}
fn replace<T>(properties: &mut Vec<Property<T>>, id: u32, value: Option<T>) {
    if let Some(value) = value {
        if let Some(property) = properties.iter_mut().find(|p| p.id == id) {
            property.value = value;
        } else {
            properties.push(Property { id, value });
        }
    } else {
        properties.retain(|p| p.id != id);
    }
}
