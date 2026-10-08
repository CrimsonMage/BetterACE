//! Cold item-XP preparation and frozen item overlays. No XP is recomputed while
//! freezing, and unrelated properties/enchantments remain in the V3 aggregate.
use crate::game_inventory::{InventoryFreezeError, InventoryFreezeInput, freeze_inventory};
use bace_character::{ItemExperience, ItemExperienceStyle};
use bace_content::{Property, SpellRowV1, WeenieV1};
use bace_dat::SpellTable;
use bace_simulation::{
    InventoryTicket, ItemExperienceReward, PreparedItemExperience, PreparedItemSet,
};
use bace_types::EntityId;
use std::collections::BTreeMap;

pub fn prepare_item_experience(
    actor: EntityId,
    item: EntityId,
    revision: u64,
    equipment_order: u64,
    source: &WeenieV1,
    spells: &SpellTable,
    rows: &BTreeMap<u32, SpellRowV1>,
) -> Result<PreparedItemExperience, String> {
    if actor.0 == 0 || item.0 == 0 || actor == item || equipment_order == 0 {
        return Err("invalid item XP identity/order".into());
    }
    let experience = read_experience(source, revision)?;
    let name = source
        .properties
        .strings
        .iter()
        .find(|p| p.id == 1)
        .map(|p| p.value.clone())
        .ok_or("item XP name missing")?;
    if name.len() > 1024 {
        return Err("item XP name limit".into());
    }
    let set = source
        .properties
        .ints
        .iter()
        .find(|p| p.id == 265)
        .filter(|p| p.value != 0)
        .map(|p| -> Result<PreparedItemSet, String> {
            let id = u32::try_from(p.value).map_err(|_| "invalid equipment set")?;
            let mut tiers = BTreeMap::new();
            if let Some(source_tiers) = spells.sets.get(&id) {
                if source_tiers.len() > 256 {
                    return Err("equipment set tier limit".into());
                }
                for (&tier, ids) in source_tiers {
                    if ids.len() > 256 {
                        return Err("equipment set spell limit".into());
                    }
                    let mut template = source.clone();
                    template.properties.spell_book = ids
                        .iter()
                        .map(|&id| i32::try_from(id).map(|id| Property { id, value: 1. }))
                        .collect::<Result<_, _>>()
                        .map_err(|_| "set spell width")?;
                    let entries =
                        crate::generator_enchantments::prepare_generator_item_enchantments(
                            actor, item, &template, spells, rows,
                        )?;
                    tiers.insert(tier, entries);
                }
            }
            Ok(PreparedItemSet { id, tiers })
        })
        .transpose()?;
    Ok(PreparedItemExperience {
        item,
        actor,
        name,
        experience,
        set,
        set_uses_item_levels: source
            .properties
            .ints
            .iter()
            .any(|p| p.id == 320 && p.value > 0),
        equipment_order,
    })
}
pub(crate) fn read_experience(
    source: &WeenieV1,
    revision: u64,
) -> Result<Option<ItemExperience>, String> {
    let i = |id| {
        source
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    let l = |id| {
        source
            .properties
            .int64s
            .iter()
            .find(|p| p.id == id)
            .map_or(0, |p| p.value)
    };
    // Exact HasItemLevel gate. Partial nonpositive metadata is not levelable.
    if l(5) <= 0 || i(319) <= 0 || i(320) <= 0 {
        return Ok(None);
    }
    let style = match i(320) {
        1 => ItemExperienceStyle::Fixed,
        2 => ItemExperienceStyle::ScalesWithLevel,
        3 => ItemExperienceStyle::FixedPlusBase,
        _ => return Err("invalid item XP style".into()),
    };
    let xp = ItemExperience {
        total: u64::try_from(l(4)).map_err(|_| "negative item XP")?,
        base: l(5) as u64,
        maximum_level: i(319) as u32,
        style,
        revision,
    };
    xp.level().map_err(|_| "invalid item XP range")?;
    Ok(Some(xp))
}

pub fn freeze_item_experience(
    input: InventoryFreezeInput<'_>,
    reward: &ItemExperienceReward,
    ticket: &InventoryTicket,
) -> Result<bace_persistence::PlacementOperation, InventoryFreezeError> {
    freeze_item_experience_with_vitae(input, reward, ticket, None)
}
pub fn freeze_item_experience_with_vitae(
    input: InventoryFreezeInput<'_>,
    reward: &ItemExperienceReward,
    ticket: &InventoryTicket,
    vitae: Option<&bace_magic::VitaeMutation>,
) -> Result<bace_persistence::PlacementOperation, InventoryFreezeError> {
    if input.proposal != &ticket.proposal
        || ticket.actor != reward.actor
        || reward.changes.len() > 64
        || reward.registries.len() > 65
    {
        return Err(InventoryFreezeError::Identity);
    }
    let mut items = input.items.to_vec();
    let mut seen = std::collections::BTreeSet::new();
    for &(id, change) in &reward.changes {
        if !seen.insert(id) {
            return Err(InventoryFreezeError::Identity);
        }
        let item = items
            .iter_mut()
            .find(|item| item.entity.object_id == id.0)
            .ok_or(InventoryFreezeError::Identity)?;
        if read_experience(&item.entity.state, item.entity.mutation_revision)
            .map_err(|_| InventoryFreezeError::Identity)?
            != Some(change.before)
            || change
                .before
                .propose_xp(reward.amount)
                .map_err(|_| InventoryFreezeError::Identity)?
                != change
            || !ticket
                .proposal
                .changes
                .iter()
                .any(|c| c.after.id == id && c.after.revision == change.after.revision)
        {
            return Err(InventoryFreezeError::Identity);
        }
        let total =
            i64::try_from(change.after.total).map_err(|_| InventoryFreezeError::Overflow)?;
        let props = &mut item.entity.state.properties.int64s;
        if let Some(property) = props.iter_mut().find(|p| p.id == 4) {
            property.value = total;
        } else {
            props.push(Property {
                id: 4,
                value: total,
            });
            props.sort_by_key(|p| p.id);
        }
    }
    for patch in &reward.registries {
        let after = patch
            .after
            .iter()
            .map(crate::enchantment_saves::freeze_enchantment)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| InventoryFreezeError::Identity)?;
        if patch.actor == reward.actor {
            // Player XP/vitae/UI composition owns the final aggregate. Require
            // the exact set-registry patch in it, instead of silently losing it.
            let snapshot = input
                .other_snapshots
                .iter()
                .find(|p| p.object_id == reward.actor.0)
                .ok_or(InventoryFreezeError::Identity)?;
            let player = bace_storage_codec::PlayerSaveV6::decode(&snapshot.bytes)?;
            let expected = if let Some(vitae) = vitae {
                if vitae.before_revision() != patch.after_revision
                    || !vitae
                        .entries()
                        .iter()
                        .filter(|e| e.spell != 666)
                        .eq(patch.after.iter().filter(|e| e.spell != 666))
                {
                    return Err(InventoryFreezeError::Identity);
                }
                vitae
                    .entries()
                    .iter()
                    .map(crate::enchantment_saves::freeze_enchantment)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| InventoryFreezeError::Identity)?
            } else {
                after
            };
            if player.enchantments != expected {
                return Err(InventoryFreezeError::Identity);
            }
        } else {
            let item = items
                .iter_mut()
                .find(|item| item.entity.object_id == patch.actor.0)
                .ok_or(InventoryFreezeError::Identity)?;
            let before = patch
                .before
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| InventoryFreezeError::Identity)?;
            if item.enchantments != before {
                return Err(InventoryFreezeError::Identity);
            }
            item.enchantments = after;
        }
    }
    freeze_inventory(InventoryFreezeInput {
        items: &items,
        ..input
    })
}
/// Rejoin cold metadata to the exact loaded item revisions. Existing callers
/// need no DAT extras for ordinary non-set, nonlevelable equipped objects.
pub(crate) fn restore_item_experience(
    loaded: &crate::game_login::LoadedPlayer,
    prepared: &[PreparedItemExperience],
) -> Result<Vec<PreparedItemExperience>, bace_storage_codec::SaveCodecError> {
    use bace_storage_codec::{ItemPlacementV2, SaveCodecError as E};
    let invalid = || E::Invalid("missing or stale prepared item XP metadata");
    if prepared.len() > 1024 {
        return Err(invalid());
    }
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for p in prepared {
        let item = loaded
            .inventory
            .iter()
            .find(|i| i.entity.object_id == p.item.0)
            .ok_or_else(invalid)?;
        if !seen.insert(p.item)
            || p.actor != loaded.binding.actor
            || read_experience(&item.entity.state, item.entity.mutation_revision)
                .map_err(|_| invalid())?
                != p.experience
        {
            return Err(invalid());
        }
        let set = item
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|property| property.id == 265)
            .map_or(0, |property| property.value);
        if u32::try_from(set).map_err(|_| invalid())? != p.set.as_ref().map_or(0, |set| set.id)
            || p.set_uses_item_levels
                != item
                    .entity
                    .state
                    .properties
                    .ints
                    .iter()
                    .any(|p| p.id == 320 && p.value > 0)
        {
            return Err(invalid());
        }
        result.push(p.clone());
    }
    for (index, item) in loaded.inventory.iter().enumerate() {
        if !matches!(item.placement,ItemPlacementV2::Contained{equipped,..} if equipped!=0)
            || seen.contains(&EntityId(item.entity.object_id))
        {
            continue;
        }
        let source = &item.entity.state;
        if read_experience(source, item.entity.mutation_revision)
            .map_err(|_| invalid())?
            .is_some()
            || source
                .properties
                .ints
                .iter()
                .any(|p| p.id == 265 && p.value != 0)
        {
            return Err(invalid());
        }
        result.push(PreparedItemExperience {
            item: EntityId(item.entity.object_id),
            actor: loaded.binding.actor,
            name: source
                .properties
                .strings
                .iter()
                .find(|p| p.id == 1)
                .map_or_else(String::new, |p| p.value.clone()),
            experience: None,
            set: None,
            set_uses_item_levels: source
                .properties
                .ints
                .iter()
                .any(|p| p.id == 320 && p.value > 0),
            equipment_order: index as u64 + 1,
        });
    }
    let mut order = std::collections::BTreeSet::new();
    if result
        .iter()
        .any(|p| p.equipment_order == 0 || !order.insert(p.equipment_order))
    {
        return Err(invalid());
    }
    Ok(result)
}
