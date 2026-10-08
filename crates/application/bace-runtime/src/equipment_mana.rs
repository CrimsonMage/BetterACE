//! Verified cold inputs for source mana consumers; transient state is reset only
//! on reconstruction, never when a valuable operation captures its baseline.
use bace_content::WeenieV1;
use bace_dat::SpellTable;
use bace_simulation::{EquipmentManaItem, EquipmentManaRecovery};
use bace_types::EntityId;
use std::collections::BTreeSet;
pub fn prepare_equipment_mana(
    actor: EntityId,
    source: &WeenieV1,
    items: &[(EntityId, &WeenieV1)],
    spells: &SpellTable,
) -> Result<EquipmentManaRecovery, String> {
    if items.len() > 1024 {
        return Err("equipment mana capacity".into());
    }
    let heartbeat = source
        .properties
        .floats
        .iter()
        .find(|p| p.id == 1)
        .map_or(5., |p| p.value);
    if !heartbeat.is_finite() {
        return Err("equipment heartbeat not finite".into());
    }
    let heartbeat = heartbeat.max(0.);
    let augmentation = source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 339)
        .map_or(0, |p| p.value);
    let rating = u32::try_from(augmentation)
        .ok()
        .and_then(|v| v.checked_mul(5))
        .ok_or("equipment mana augmentation")?;
    prepare_values(actor, heartbeat, rating, items, spells)
}
pub fn prepare_equipment_mana_item(
    actor: EntityId,
    id: EntityId,
    source: &WeenieV1,
    spells: &SpellTable,
) -> Result<Option<EquipmentManaItem>, String> {
    Ok(prepare_values(actor, 5., 0, &[(id, source)], spells)?
        .items
        .pop())
}
fn prepare_values(
    actor: EntityId,
    heartbeat: f64,
    rating: u32,
    items: &[(EntityId, &WeenieV1)],
    spells: &SpellTable,
) -> Result<EquipmentManaRecovery, String> {
    let mut prepared = Vec::new();
    for &(item_id, s) in items {
        let current = s
            .properties
            .ints
            .iter()
            .find(|p| p.id == 107)
            .map(|p| p.value);
        let maximum = s
            .properties
            .ints
            .iter()
            .find(|p| p.id == 108)
            .map(|p| p.value);
        let rate = s
            .properties
            .floats
            .iter()
            .find(|p| p.id == 5)
            .map(|p| p.value);
        let affecting = s
            .properties
            .bools
            .iter()
            .find(|p| p.id == 56)
            .map(|p| p.value);
        // Non-consumers still need activation ownership when they carry mana.
        if current.is_none() && maximum.is_none() && rate.is_none() && affecting.is_none() {
            continue;
        }
        let mut removals = Vec::new();
        let mut seen = BTreeSet::new();
        for known in &s.properties.spell_book {
            let id = u32::try_from(known.id).map_err(|_| "equipment mana spell ID")?;
            if id == 0 || !seen.insert(id) || seen.len() > 256 {
                return Err("equipment mana spell capacity/duplicate".into());
            }
            let spell = spells
                .spells
                .get(&id)
                .ok_or("equipment mana spell missing")?;
            let target =
                if spell.school == 4 && ![152, 154, 156, 158, 195, 695].contains(&spell.category) {
                    item_id
                } else {
                    actor
                };
            removals.push((target, id));
        }
        prepared.push(EquipmentManaItem {
            item: item_id,
            name: s
                .properties
                .strings
                .iter()
                .find(|p| p.id == 1)
                .map_or_else(String::new, |p| p.value.clone()),
            current,
            maximum,
            rate,
            affecting,
            removals,
            accumulator: 0.,
            warned: false,
            removal_remaining: None,
        });
    }
    let result = EquipmentManaRecovery {
        fresh: true,
        heartbeat,
        rating,
        heartbeat_remaining: heartbeat,
        items: prepared,
    };
    result.validate(actor).map_err(str::to_owned)?;
    Ok(result)
}
/// Overlay only accepted Biota fields. Accumulator/warning/delay intentionally
/// have no frozen schema representation because ACE reconstructs them empty.
pub fn overlay_equipment_mana(source: &mut WeenieV1, item: &EquipmentManaItem) {
    set(&mut source.properties.ints, 107, item.current);
    set(&mut source.properties.bools, 56, item.affecting);
}
fn set<T>(values: &mut Vec<bace_content::Property<T>>, id: u32, value: Option<T>) {
    match value {
        Some(value) => {
            if let Some(p) = values.iter_mut().find(|p| p.id == id) {
                p.value = value
            } else {
                values.push(bace_content::Property { id, value })
            }
        }
        None => values.retain(|p| p.id != id),
    }
}

/// Durability hardening for ACE's unpersisted two-second action: a reconstructed
/// zero-mana, non-affecting item cannot retain one of its own known enchantments.
/// The simulation must complete these exact caster removals before entry capture.
pub fn prepare_equipment_mana_cleanup(
    actor: EntityId,
    state: &mut EquipmentManaRecovery,
    player: &bace_magic::EnchantmentRegistry,
    items: &[(EntityId, bace_magic::EnchantmentRegistry)],
) -> Result<usize, String> {
    if !state.fresh || items.len() > 1024 {
        return Err("equipment cleanup requires cold reconstruction".into());
    }
    state.validate(actor).map_err(str::to_owned)?;
    let mut count = 0;
    for item in &mut state.items {
        if item.current != Some(0) || item.affecting == Some(true) {
            continue;
        }
        let dangling = item.removals.iter().any(|(target, spell)| {
            let registry = if *target == actor {
                Some(player)
            } else {
                items.iter().find(|(id, _)| id == target).map(|(_, r)| r)
            };
            registry.is_some_and(|r| {
                r.entries()
                    .iter()
                    .any(|entry| entry.caster == item.item.0 && entry.spell == *spell)
            })
        });
        if dangling {
            item.removal_remaining = Some(0.);
            count += 1;
        }
    }
    Ok(count)
}
