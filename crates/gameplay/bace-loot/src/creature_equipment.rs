//! Pinned ACE Creature constructor, Monster_Inventory and Creature_Equipment.
//! Selection is prepared before allocating identities. DestinationType remains
//! ephemeral; copied item properties never encode a fabricated destination.
use crate::treasure_random::inclusive;
use crate::{TreasureError, TreasureRandom, WieldedTreasure, materialize_create_list};
use bace_content::WeenieV1;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug)]
pub struct PreparedCreatureEquipment {
    pub source: WeenieV1,
    /// Exact acquisition CreateList flags. None denotes a source without a row.
    pub source_destination: Option<u8>,
    pub wielded_location: u32,
    pub death_drop: bool,
    pub parent_index: Option<usize>,
    pub inventory_slot: u32,
    pub equip_order: Option<u32>,
}
#[derive(Clone)]
struct Item {
    generator_parent: Option<usize>,
    source: WeenieV1,
    source_destination: Option<u8>,
    inventory: bool,
    location: u32,
    drop: bool,
    parent_index: Option<usize>,
    slot: u32,
    equip_order: Option<u32>,
}
pub(crate) fn int(w: &WeenieV1, id: u32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(0, |p| p.value)
}
fn boolean(w: &WeenieV1, id: u32) -> bool {
    w.properties.bools.iter().any(|p| p.id == id && p.value)
}
/// Pinned ACE Container.TryAddToInventory compares the signed capacity with an
/// inventory count. Its authored -1 sentinel therefore has no available slots;
/// non-pack items can still fall through to a side container.
fn capacity(w: &WeenieV1, id: u32) -> Result<usize, TreasureError> {
    match int(w, id) {
        -1 => Ok(0),
        value if value >= 0 => Ok(value as usize),
        _ => Err(TreasureError::Bounds),
    }
}
fn loc(w: &WeenieV1) -> u32 {
    int(w, 9) as u32
}
fn coverage(w: &WeenieV1) -> u32 {
    int(w, 4) as u32
}
fn ranged(w: &WeenieV1) -> bool {
    matches!(int(w, 46), 16 | 32 | 128 | 1024)
}
fn parent(w: &WeenieV1, location: u32) -> Option<u32> {
    match location {
        0x100000 | 0x1000000 | 0x2000000 => Some(1),
        0x200000 => Some(if int(w, 1) == 2 { 2 } else { 3 }),
        0x400000 => Some(if matches!(int(w, 46), 16 | 32) { 4 } else { 1 }),
        _ => None,
    }
}
fn equip(items: &mut [Item], idx: usize, discard_failure: bool) {
    let location = loc(&items[idx].source);
    let desired = &items[idx].source;
    let blocked = items.iter().any(|old| {
        old.location != 0
            && match parent(desired, location) {
                Some(p) => old.location != 0x800000 && parent(&old.source, old.location) == Some(p),
                None if desired.weenie_type == 2 => coverage(&old.source) & coverage(desired) != 0,
                None => old.location & location != 0,
            }
    });
    let was_inventory = items[idx].inventory;
    let old_parent = items[idx].parent_index;
    let old_slot = items[idx].slot;
    if was_inventory {
        for old in items
            .iter_mut()
            .filter(|i| i.inventory && i.parent_index == old_parent && i.slot > old_slot)
        {
            old.slot -= 1;
        }
    }
    if !blocked {
        items[idx].equip_order = Some(
            items
                .iter()
                .filter_map(|i| i.equip_order)
                .max()
                .map_or(0, |n| n + 1),
        );
        items[idx].inventory = false;
        items[idx].location = location;
        items[idx].parent_index = None;
        items[idx].slot = 0;
    } else if discard_failure && items[idx].location == 0 {
        items[idx].inventory = false;
    } else if was_inventory {
        for (i, old) in items
            .iter_mut()
            .enumerate()
            .filter(|(i, old)| *i != idx && old.inventory && old.parent_index == old_parent)
        {
            let _ = i;
            old.slot += 1;
        }
        items[idx].slot = 0;
    }
}
fn find<R: TreasureRandom>(
    ids: &mut [usize],
    items: &[Item],
    random: &mut R,
) -> Result<Option<usize>, TreasureError> {
    for n in (1..ids.len()).rev() {
        let k = inclusive(random, 0, n as i32)? as usize;
        ids.swap(n, k);
    }
    Ok(ids
        .iter()
        .copied()
        .find(|i| !boolean(&items[*i].source, 130)))
}
fn phase<R: TreasureRandom>(
    source: &WeenieV1,
    items: &mut [Item],
    random: &mut R,
) -> Result<(), TreasureError> {
    let inventory = clothing_order(items, None);
    let mut clothing: Vec<_> = inventory
        .into_iter()
        .filter(|i| {
            items[*i].source.weenie_type == 2
                && (coverage(&items[*i].source) & 0x7e != 0
                    || loc(&items[*i].source) & 0x8000000 != 0)
        })
        .collect();
    crate::creature_equipment_sort::sort(&mut clothing, |a, b| {
        loc(&items[a].source)
            .count_ones()
            .cmp(&loc(&items[b].source).count_ones())
    });
    clothing.reverse();
    for mask in [6, 0x68, 0] {
        let candidate = clothing.iter().copied().find(|i| {
            if mask == 0 {
                loc(&items[*i].source) & 0x8000000 != 0
            } else {
                coverage(&items[*i].source) & mask != 0
            }
        });
        if let Some(i) = candidate {
            equip(items, i, true);
        }
    }
    let mut armor: Vec<_> = clothing_order(items, None)
        .into_iter()
        .filter(|i| items[*i].source.weenie_type == 2 && coverage(&items[*i].source) & 0x1ff00 != 0)
        .collect();
    crate::creature_equipment_sort::sort(&mut armor, |a, b| {
        loc(&items[a].source)
            .count_ones()
            .cmp(&loc(&items[b].source).count_ones())
    });
    armor.reverse();
    crate::creature_equipment_sort::sort(&mut armor, |a, b| {
        int(&items[a].source, 28).cmp(&int(&items[b].source, 28))
    });
    armor.reverse();
    for mask in [1, 0x202, 0x808, 0x1010, 0x20, 0x2040, 0x400, 0x4080, 0x100] {
        for i in armor
            .iter()
            .copied()
            .filter(|i| loc(&items[*i].source) & mask != 0)
            .collect::<Vec<_>>()
        {
            equip(items, i, true);
        }
    }
    let npc = !source
        .properties
        .bools
        .iter()
        .find(|p| p.id == 19)
        .is_none_or(|p| p.value)
        && int(source, 68) == 0;
    let mut weapons: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            i.inventory
                && i.parent_index.is_none()
                && (matches!(i.source.weenie_type, 3 | 4 | 6 | 35)
                    || (npc && loc(&i.source) & 0x3700000 != 0))
        })
        .map(|(i, _)| i)
        .collect();
    let mut selected = Vec::new();
    while let Some(index) = find(&mut weapons, items, random)? {
        let weapon = &items[index].source;
        if matches!(int(weapon, 46), 16 | 32 | 1024) {
            let mut ammo: Vec<_> = items
                .iter()
                .enumerate()
                .filter(|(_, i)| {
                    i.inventory
                        && i.parent_index.is_none()
                        && i.source.weenie_type == 5
                        && int(&i.source, 50) == int(weapon, 50)
                })
                .map(|(i, _)| i)
                .collect();
            if let Some(ammo) = find(&mut ammo, items, random)? {
                selected.extend([index, ammo]);
                break;
            }
            if npc {
                selected.push(index);
                break;
            }
            weapons.retain(|i| *i != index);
            continue;
        }
        selected.push(index);
        if int(source, 101) & 256 != 0
            && weapon.weenie_type == 6
            && int(weapon, 48) != 41
            && let Some(left) = weapons
                .iter()
                .copied()
                .find(|i| boolean(&items[*i].source, 130))
        {
            selected.push(left);
        }
        break;
    }
    let shields: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.inventory && i.parent_index.is_none() && int(&i.source, 51) == 4)
        .map(|(i, _)| i)
        .collect();
    if !shields.is_empty() {
        let shield = shields[inclusive(random, 0, shields.len() as i32 - 1)? as usize];
        if selected.first().is_none_or(|i| !ranged(&items[*i].source)) {
            selected.push(shield);
        }
    }
    for i in selected {
        if items[i].inventory && items[i].source.properties.ints.iter().any(|p| p.id == 9) {
            equip(items, i, false);
        }
    }
    Ok(())
}
fn add(
    source: &WeenieV1,
    scope: Option<usize>,
    items: &mut Vec<Item>,
    item: WeenieV1,
    drop: bool,
    source_destination: Option<u8>,
) -> Result<(), TreasureError> {
    if items.len() >= 1024 {
        return Err(TreasureError::Capacity);
    }
    let pack = boolean(&item, 81);
    let count = items
        .iter()
        .filter(|i| i.inventory && i.parent_index == scope && boolean(&i.source, 81) == pack)
        .count();
    let capacity = capacity(source, if pack { 7 } else { 6 })?;
    let mut parent_index = scope;
    if count >= capacity {
        if pack {
            return Ok(());
        }
        let mut containers: Vec<_> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i.inventory
                    && i.parent_index == scope
                    && matches!(
                        i.source.weenie_type,
                        10 | 12 | 14 | 15 | 20 | 21 | 55 | 56 | 57 | 61 | 69 | 71
                    )
            })
            .map(|(n, _)| n)
            .collect();
        // Source sorts Placement (animation frame), not PlacementPosition.
        // Every newly contained object has Placement.Resting, hence equal keys.
        crate::creature_equipment_sort::sort(&mut containers, |_, _| std::cmp::Ordering::Equal);
        parent_index = containers.into_iter().find(|n| {
            items
                .iter()
                .filter(|i| i.inventory && i.parent_index == Some(*n) && !boolean(&i.source, 81))
                .count()
                < int(&items[*n].source, 6).max(0) as usize
        });
        if parent_index.is_none() {
            return Ok(());
        }
    }
    for old in items
        .iter_mut()
        .filter(|i| i.inventory && i.parent_index == parent_index && boolean(&i.source, 81) == pack)
    {
        old.slot += 1;
    }
    items.push(Item {
        generator_parent: scope,
        source: item,
        source_destination,
        inventory: true,
        location: 0,
        drop,
        parent_index,
        slot: 0,
        equip_order: None,
    });
    Ok(())
}
/// Default trophy rate 1.0; caller supplies the explicit server Wield-drop policy.
/// First create-list selection/equip, then DID32 selection/equip, then inventory
/// treasure. Failures publish no items and leave the random cursor unchanged.
pub fn generate_creature_equipment<R: TreasureRandom>(
    source: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    wielded: Option<&WieldedTreasure>,
    inventory_treasure: &[WeenieV1],
    drop_plain_wield: bool,
    random: &mut R,
) -> Result<Vec<PreparedCreatureEquipment>, TreasureError> {
    if source.properties.create_list.len() > 4096 || inventory_treasure.len() > 1024 {
        return Err(TreasureError::Capacity);
    }
    let mut rng = random.clone();
    let mut items = Vec::new();
    let rows: Vec<_> = source
        .properties
        .create_list
        .iter()
        .filter(|r| r.destination_type & 2 != 0)
        .cloned()
        .collect();
    for index in crate::generate_create_list_selection(&rows, &mut rng)? {
        let row = &rows[index];
        if row.weenie_class_id == 0 {
            continue;
        }
        let template = templates
            .get(&row.weenie_class_id)
            .ok_or(TreasureError::MissingTemplate(row.weenie_class_id))?;
        add_tree(
            source,
            None,
            templates,
            0,
            &mut items,
            materialize_create_list(row, template)?,
            row.destination_type & 8 != 0 || drop_plain_wield,
            Some(destination_from_row(row)?),
        )?;
    }
    phase(source, &mut items, &mut rng)?;
    if let Some(wielded) = wielded {
        for item in wielded.generate(&mut rng)? {
            add_tree(source, None, templates, 0, &mut items, item, false, None)?;
        }
    }
    phase(source, &mut items, &mut rng)?;
    for item in inventory_treasure {
        add_tree(
            source,
            None,
            templates,
            0,
            &mut items,
            item.clone(),
            true,
            None,
        )?;
    }
    let output = export(items);
    *random = rng;
    Ok(output)
}
/// Append DID33 results after both equip passes without rerolling equipment.
pub fn append_creature_inventory(
    source: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    output: &mut Vec<PreparedCreatureEquipment>,
    generated: Vec<WeenieV1>,
) -> Result<(), TreasureError> {
    if output.len() > 1024 || generated.len() > 1024 {
        return Err(TreasureError::Capacity);
    }
    if output.iter().enumerate().any(|(n, i)| {
        i.parent_index
            .is_some_and(|p| p >= n || i.wielded_location != 0)
    }) {
        return Err(TreasureError::Bounds);
    }
    let mut items: Vec<_> = output
        .iter()
        .map(|i| Item {
            generator_parent: i.parent_index,
            source: i.source.clone(),
            source_destination: i.source_destination,
            inventory: i.wielded_location == 0,
            location: i.wielded_location,
            drop: i.death_drop,
            parent_index: i.parent_index,
            slot: i.inventory_slot,
            equip_order: i.equip_order,
        })
        .collect();
    for item in generated {
        add_tree(source, None, templates, 0, &mut items, item, true, None)?;
    }
    *output = export(items);
    Ok(())
}

fn export(items: Vec<Item>) -> Vec<PreparedCreatureEquipment> {
    let mut index = vec![None; items.len()];
    let mut next = 0;
    for (n, i) in items.iter().enumerate() {
        if i.inventory || i.location != 0 {
            index[n] = Some(next);
            next += 1;
        }
    }
    items
        .into_iter()
        .filter(|i| i.inventory || i.location != 0)
        .map(|i| PreparedCreatureEquipment {
            death_drop: i.drop && int(&i.source, 33) != -2,
            source: i.source,
            source_destination: i.source_destination,
            wielded_location: i.location,
            parent_index: i.parent_index.and_then(|n| index[n]),
            inventory_slot: i.slot,
            equip_order: i.equip_order,
        })
        .collect()
}

fn destination_from_row(row: &bace_content::CreateListEntry) -> Result<u8, TreasureError> {
    let flags = u8::try_from(row.destination_type).map_err(|_| TreasureError::Bounds)?;
    if flags & !0x3f != 0 {
        return Err(TreasureError::Bounds);
    }
    Ok(flags)
}

#[allow(clippy::too_many_arguments)]
fn add_tree(
    source: &WeenieV1,
    scope: Option<usize>,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    depth: usize,
    items: &mut Vec<Item>,
    item: WeenieV1,
    drop: bool,
    source_destination: Option<u8>,
) -> Result<(), TreasureError> {
    if depth >= 16 {
        return Err(TreasureError::Capacity);
    }
    if item.validate(Default::default()).is_err() {
        return Err(TreasureError::InvalidTemplate(item.weenie_id));
    }
    let index = items.len();
    add(source, scope, items, item, drop, source_destination)?;
    if items.len() == index {
        return Ok(());
    }
    if !matches!(items[index].source.weenie_type, 14 | 20 | 21 | 55 | 56 | 57) {
        return Ok(());
    }
    let container = items[index].source.clone();
    for row in container
        .properties
        .create_list
        .iter()
        .filter(|r| matches!(r.destination_type, 1 | 9))
    {
        if row.weenie_class_id == 0 {
            continue;
        }
        let Some(template) = templates.get(&row.weenie_class_id) else {
            continue;
        };
        add_tree(
            &container,
            Some(index),
            templates,
            depth + 1,
            items,
            materialize_contain_list(row, template)?,
            false,
            Some(destination_from_row(row)?),
        )?;
    }
    Ok(())
}

fn clothing_order(items: &[Item], parent: Option<usize>) -> Vec<usize> {
    let mut local: Vec<_> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i.inventory && i.parent_index == parent)
        .map(|(n, i)| (i.slot, n))
        .collect();
    local.sort();
    let mut output: Vec<_> = local.iter().map(|(_, n)| *n).collect();
    for (_, n) in local {
        if items[n].source.weenie_type == 21 {
            output.extend(clothing_order(items, Some(n)));
        }
    }
    output
}

/// Instantiate an already-selected create-list object and its Container contain
/// lists. Parent indices precede their children; no corpse/root capacity is guessed.
pub fn materialize_create_list_tree(
    row: &bace_content::CreateListEntry,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
) -> Result<Vec<PreparedCreatureEquipment>, TreasureError> {
    if row.weenie_class_id == 0 {
        return Ok(Vec::new());
    }
    let template = templates
        .get(&row.weenie_class_id)
        .ok_or(TreasureError::MissingTemplate(row.weenie_class_id))?;
    let mut owner = (**template).clone();
    owner.properties.ints.retain(|p| !matches!(p.id, 6 | 7));
    owner.properties.ints.extend([
        bace_content::Property { id: 6, value: 1024 },
        bace_content::Property { id: 7, value: 1024 },
    ]);
    let mut items = Vec::new();
    add_tree(
        &owner,
        None,
        templates,
        0,
        &mut items,
        materialize_create_list(row, template)?,
        true,
        Some(destination_from_row(row)?),
    )?;
    Ok(export(items))
}

/// Container.GenerateContainList is not Creature.CreateListSelect: it never
/// rolls probability and applies positive Shade even for ContainTreasure rows.
fn materialize_contain_list(
    row: &bace_content::CreateListEntry,
    template: &WeenieV1,
) -> Result<WeenieV1, TreasureError> {
    if !row.shade.is_finite() {
        return Err(TreasureError::Bounds);
    }
    let mut item = template.clone();
    if row.stack_size > 1 {
        crate::set_treasure_stack(&mut item, row.stack_size)?;
    }
    if row.palette > 0 {
        set_property(&mut item.properties.ints, 3, i32::from(row.palette));
    }
    if row.shade > 0.0 {
        set_property(&mut item.properties.floats, 12, f64::from(row.shade));
    }
    Ok(item)
}
fn set_property<T>(values: &mut Vec<bace_content::Property<T>>, id: u32, value: T) {
    if let Some(p) = values.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        values.push(bace_content::Property { id, value });
        values.sort_by_key(|p| p.id);
    }
}
#[derive(Clone, Debug)]
pub struct PreparedContainerItem {
    pub source: WeenieV1,
    pub source_destination: Option<u8>,
    pub parent_index: Option<usize>,
    /// Original generating container; side-pack capacity fallback may change
    /// the accepted inventory parent without changing source GeneratorID.
    pub generator_parent_index: Option<usize>,
    pub inventory_slot: u32,
}
/// Preserve an already-mutated root and construct its source container contents.
/// Root is index0; every parent precedes its descendants, with bounded recursion.
pub fn materialize_container_tree(
    root: WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
) -> Result<Vec<PreparedContainerItem>, TreasureError> {
    let mut owner = root.clone();
    owner.properties.ints.retain(|p| !matches!(p.id, 6 | 7));
    owner.properties.ints.extend([
        bace_content::Property { id: 6, value: 1024 },
        bace_content::Property { id: 7, value: 1024 },
    ]);
    let mut items = Vec::new();
    add_tree(&owner, None, templates, 0, &mut items, root, false, None)?;
    Ok(items
        .into_iter()
        .map(|i| PreparedContainerItem {
            source: i.source,
            source_destination: i.source_destination,
            parent_index: i.parent_index,
            generator_parent_index: i.generator_parent,
            inventory_slot: i.slot,
        })
        .collect())
}
