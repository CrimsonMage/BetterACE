//! Freeze validated crafting decisions; no random draws or world mutation here.
mod proficiency;
use bace_content::{Property, SparseProperties};
use bace_crafting::{
    CraftItem, CraftProposal, PropertyKey, PropertyKind, PropertyValue, SalvageProposal,
};
use bace_persistence::{
    CharacterLease, DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5, PlayerSaveV6,
    SaveCodecError, validate_item_source_destination_transition,
};
pub use proficiency::join_proficiency;
use std::collections::{BTreeMap, BTreeSet};

pub struct CraftingSavedItem {
    pub saved: ItemSaveV5,
    pub persisted_version: i64,
}
pub struct CraftingSavedPlayer {
    pub saved: PlayerSaveV6,
    pub persisted_version: i64,
}
pub struct CraftFreezeInput<'a> {
    pub proposal: &'a CraftProposal,
    pub player: &'a CraftingSavedPlayer,
    pub source: &'a CraftingSavedItem,
    pub target: &'a CraftingSavedItem,
    /// Complete reserved ancestor set; root actor and both items are mandatory.
    pub participants: &'a [u32],
    pub lease: CharacterLease,
    pub inventory: &'a bace_inventory::InventoryProposal,
    /// Other snapshots changed by inventory slot compaction/insertion.
    pub other_items: &'a [CraftingSavedItem],
}
pub struct SalvageBagTemplate {
    pub entity: EntitySaveV1,
    pub slot: u32,
}
pub struct SalvageFreezeInput<'a> {
    pub proposal: &'a SalvageProposal,
    pub player: &'a CraftingSavedPlayer,
    pub consumed: &'a [CraftingSavedItem],
    pub generated: &'a [SalvageBagTemplate],
    pub participants: &'a [u32],
    pub lease: CharacterLease,
    pub inventory: &'a bace_inventory::InventoryProposal,
    pub other_items: &'a [CraftingSavedItem],
}
fn invalid() -> SaveCodecError {
    SaveCodecError::Invalid("crafting frozen identity/revision or proposal mismatch")
}

/// Scalars and spellbook membership. Existing spell probabilities, appearance
/// and registries remain in the frozen aggregate and survive delta updates.
pub fn crafting_properties(props: &SparseProperties) -> BTreeMap<PropertyKey, PropertyValue> {
    let mut out = BTreeMap::new();
    macro_rules! copy {
        ($field:ident,$kind:ident) => {
            for p in &props.$field {
                out.insert(
                    PropertyKey {
                        kind: PropertyKind::$kind,
                        id: p.id,
                    },
                    PropertyValue::$kind(p.value.clone()),
                );
            }
        };
    }
    copy!(bools, Bool);
    copy!(ints, Int);
    copy!(int64s, Int64);
    copy!(floats, Float);
    copy!(strings, String);
    copy!(data_ids, DataId);
    copy!(instance_ids, InstanceId);
    for spell in &props.spell_book {
        out.insert(
            PropertyKey {
                kind: PropertyKind::SpellBook,
                id: spell.id as u32,
            },
            PropertyValue::SpellBook(true),
        );
    }
    out
}

pub fn freeze_craft(input: CraftFreezeInput<'_>) -> Result<PlacementOperation, SaveCodecError> {
    let p = input.proposal;
    let participants = participants(input.participants, p.actor, input.lease)?;
    if p.operation_id == [0; 16]
        || p.random_key_version == 0
        || input.player.persisted_version <= 0
        || input.player.saved.player.entity.object_id != p.actor
        || input.player.saved.player.entity.mutation_revision != p.expected_actor_revision
        || crafting_properties(&input.player.saved.player.entity.state.properties)
            != p.actor_before_properties
        || p.source_before.id == p.target_before.id
        || !participants.contains(&p.source_before.id)
        || !participants.contains(&p.target_before.id)
    {
        return Err(invalid());
    }
    let changed = p.actor_properties != p.actor_before_properties;
    if p.actor_revision
        != p.expected_actor_revision
            .checked_add(u64::from(changed))
            .ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    let mut snapshots = Vec::with_capacity(3);
    let mut changes = Vec::with_capacity(2);
    if changed {
        let mut player = input.player.saved.clone();
        apply_properties(
            &mut player.player.entity.state.properties,
            &p.actor_before_properties,
            &p.actor_properties,
            None,
        )?;
        player.player.entity.mutation_revision = p.actor_revision;
        snapshots.push(SaveSnapshot {
            object_id: p.actor,
            mutation_revision: p.actor_revision,
            expected_version: input.player.persisted_version,
            bytes: player.encode()?,
        });
    }
    for (saved, before, after) in [
        (input.source, &p.source_before, p.source_after.as_ref()),
        (input.target, &p.target_before, p.target_after.as_ref()),
    ] {
        if saved.persisted_version <= 0
            || saved.saved.entity.object_id != before.id
            || saved.saved.entity.mutation_revision != before.revision
            || before.owner != p.actor
            || crafting_properties(&saved.saved.entity.state.properties) != before.properties
            || craft_item_snapshot(&saved.saved, p.actor)? != *before
        {
            return Err(invalid());
        }
        let graph = input
            .inventory
            .changes
            .iter()
            .find(|c| c.after.id.0 == before.id);
        match (after, graph) {
            (Some(after), Some(graph))
                if graph.after.stack == after.stack
                    && graph.after.place != bace_inventory::ItemPlace::Removed
                    && graph.after.revision
                        == before.revision.checked_add(1).ok_or_else(invalid)? => {}
            (Some(after), None) if after == before => {}
            (None, Some(graph))
                if graph.after.stack == 0
                    && graph.after.place == bace_inventory::ItemPlace::Removed
                    && graph.after.revision
                        == before.revision.checked_add(1).ok_or_else(invalid)? => {}
            _ => return Err(invalid()),
        }
        let mut next = saved.saved.clone();
        let expected = durable(&next.placement);
        if !matches!(expected, DurableItemPlace::Contained { equipped: 0, .. }) {
            return Err(invalid());
        }
        if let Some(after) = after {
            if after.id != before.id
                || after.owner != before.owner
                || after.stack == 0
                || after.revision
                    != before
                        .revision
                        .checked_add(u64::from(after != before))
                        .ok_or_else(invalid)?
            {
                return Err(invalid());
            }
            apply_properties(
                &mut next.entity.state.properties,
                &before.properties,
                &after.properties,
                Some(after.times_tinkered),
            )?;
            if after.stack != before.stack {
                set(
                    &mut next.entity.state.properties.ints,
                    12,
                    i32::try_from(after.stack).map_err(|_| invalid())?,
                );
            }
            if after.times_tinkered != before.times_tinkered {
                set(
                    &mut next.entity.state.properties.ints,
                    171,
                    i32::try_from(after.times_tinkered).map_err(|_| invalid())?,
                );
            }
            if after.tinker_log != before.tinker_log {
                set(
                    &mut next.entity.state.properties.strings,
                    9007,
                    after
                        .tinker_log
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                );
            }
            next.entity.mutation_revision = after.revision;
        } else {
            next.entity.mutation_revision = before.revision.checked_add(1).ok_or_else(invalid)?;
            remove(&mut next);
        }
        validate_item_source_destination_transition(&saved.saved, &next)?;
        if next != saved.saved {
            changes.push(PlacementChange {
                item: before.id,
                expected: Some(expected),
                destination: durable(&next.placement),
            });
            snapshots.push(SaveSnapshot {
                object_id: before.id,
                mutation_revision: next.entity.mutation_revision,
                expected_version: saved.persisted_version,
                bytes: next.encode()?,
            });
        }
    }
    if snapshots.is_empty() {
        return Err(SaveCodecError::Invalid(
            "crafting operation has no durable mutation",
        ));
    }
    let staged = PlacementOperation {
        operation_id: format!("craft-{:032x}", u128::from_le_bytes(p.operation_id)),
        snapshots,
        participants: participants.into_iter().collect(),
        leases: vec![input.lease],
        changes,
        storage_views: vec![],
    };
    merge_inventory(
        staged,
        input.inventory,
        &[input.source, input.target],
        input.other_items,
        &[],
    )
}

pub fn freeze_salvage(input: SalvageFreezeInput<'_>) -> Result<PlacementOperation, SaveCodecError> {
    let p = input.proposal;
    let participants = participants(input.participants, p.actor, input.lease)?;
    if p.operation_id == [0; 16]
        || p.consumed.len() != input.consumed.len()
        || p.bags.len() != input.generated.len()
        || p.bags.len() > 300
        || input.player.persisted_version <= 0
        || input.player.saved.player.entity.object_id != p.actor
        || input.player.saved.player.entity.mutation_revision != p.expected_actor_revision
        || !participants.contains(&p.tool.id)
        || p.consumed.is_empty()
    {
        return Err(invalid());
    }
    let mut snapshots = Vec::new();
    let mut changes = Vec::new();
    let mut ids = BTreeSet::new();
    for (id, revision) in &p.consumed {
        let source = input
            .consumed
            .iter()
            .find(|s| s.saved.entity.object_id == *id)
            .ok_or_else(invalid)?;
        if !ids.insert(*id)
            || !participants.contains(id)
            || source.saved.entity.mutation_revision != *revision
            || source.persisted_version <= 0
        {
            return Err(invalid());
        }
        let graph = input
            .inventory
            .changes
            .iter()
            .find(|change| change.after.id.0 == *id)
            .ok_or_else(invalid)?;
        if graph
            .before
            .as_ref()
            .is_none_or(|before| before.revision != *revision)
            || graph.after.stack != 0
            || graph.after.place != bace_inventory::ItemPlace::Removed
            || graph.after.revision != revision.checked_add(1).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        let mut next = source.saved.clone();
        let expected = durable(&next.placement);
        if !matches!(expected, DurableItemPlace::Contained { equipped: 0, .. }) {
            return Err(invalid());
        }
        next.entity.mutation_revision = revision.checked_add(1).ok_or_else(invalid)?;
        remove(&mut next);
        validate_item_source_destination_transition(&source.saved, &next)?;
        snapshots.push(SaveSnapshot {
            object_id: *id,
            mutation_revision: next.entity.mutation_revision,
            expected_version: source.persisted_version,
            bytes: next.encode()?,
        });
        changes.push(PlacementChange {
            item: *id,
            expected: Some(expected),
            destination: DurableItemPlace::Removed,
        });
    }
    let mut slots = BTreeSet::new();
    for (bag, template) in p.bags.iter().zip(input.generated) {
        if template.entity.state.weenie_id != bag.template
            || !ids.insert(template.entity.object_id)
            || template.entity.object_id == p.actor
            || template.entity.object_id == p.tool.id
            || !slots.insert(template.slot)
            || bag.units == 0
            || bag.units > 100
            || bag.num_items == 0
        {
            return Err(invalid());
        }
        let graph = input
            .inventory
            .changes
            .iter()
            .find(|change| change.after.id.0 == template.entity.object_id)
            .ok_or_else(invalid)?;
        if graph.before.is_some()
            || graph.after.stack != 1
            || graph.after.revision != 1
            || graph.after.template != bag.template
            || graph.after.unit_value != bag.value
            || graph.after.structure != Some(bag.units)
            || !matches!(graph.after.place, bace_inventory::ItemPlace::Contained { container, equipped: 0, .. } if container.0 == p.actor)
        {
            return Err(invalid());
        }
        let mut entity = template.entity.clone();
        entity.mutation_revision = 1;
        entity
            .state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3].contains(&p.id));
        entity.state.properties.positions.retain(|p| p.id != 1);
        entity
            .state
            .properties
            .ints
            .retain(|p| ![10, 53].contains(&p.id));
        for (id, value) in [
            (91, 100),
            (92, bag.units),
            (105, bag.raw_workmanship),
            (170, bag.num_items),
            (131, bag.material),
            (19, bag.value),
            (53, template.slot),
        ] {
            set(
                &mut entity.state.properties.ints,
                id,
                i32::try_from(value).map_err(|_| invalid())?,
            );
        }
        set(
            &mut entity.state.properties.strings,
            1,
            format!("Salvage ({})", bag.units),
        );
        set(&mut entity.state.properties.instance_ids, 1, p.actor);
        set(&mut entity.state.properties.instance_ids, 2, p.actor);
        let placement = ItemPlacementV2::Contained {
            container: p.actor,
            slot: template.slot,
            pack_slot: false,
            equipped: 0,
        };
        let saved = ItemSaveV5::migrate_v4(ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity,
            placement: placement.clone(),
        })?)?;
        snapshots.push(SaveSnapshot {
            object_id: saved.entity.object_id,
            mutation_revision: 1,
            expected_version: 0,
            bytes: saved.encode()?,
        });
        changes.push(PlacementChange {
            item: saved.entity.object_id,
            expected: None,
            destination: durable(&placement),
        });
    }
    let mut participants = participants;
    participants.extend(ids);
    let staged = PlacementOperation {
        operation_id: format!("salvage-{:032x}", u128::from_le_bytes(p.operation_id)),
        snapshots,
        participants: participants.into_iter().collect(),
        leases: vec![input.lease],
        changes,
        storage_views: vec![],
    };
    let originals: Vec<_> = input.consumed.iter().collect();
    merge_inventory(
        staged,
        input.inventory,
        &originals,
        input.other_items,
        input.generated,
    )
}

fn merge_inventory(
    staged: PlacementOperation,
    inventory: &bace_inventory::InventoryProposal,
    originals: &[&CraftingSavedItem],
    others: &[CraftingSavedItem],
    generated: &[SalvageBagTemplate],
) -> Result<PlacementOperation, SaveCodecError> {
    use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput, freeze_inventory};
    let mut frozen = Vec::new();
    for change in &inventory.changes {
        let id = change.after.id.0;
        let source = originals
            .iter()
            .copied()
            .chain(others.iter())
            .find(|s| s.saved.entity.object_id == id);
        let staged_item = staged
            .snapshots
            .iter()
            .find(|s| s.object_id == id)
            .map(|s| ItemSaveV5::decode(&s.bytes))
            .transpose()?;
        let item = if let Some(source) = source {
            if change
                .before
                .as_ref()
                .is_none_or(|v| v.revision != source.saved.entity.mutation_revision)
            {
                return Err(invalid());
            }
            let mut next = staged_item.unwrap_or_else(|| source.saved.clone());
            // Inventory freezer validates the old revision before assigning the
            // confirmed graph revision and preserving prepared scalar deltas.
            next.entity.mutation_revision = source.saved.entity.mutation_revision;
            FrozenInventoryItem {
                corpse: None,
                construction: next.construction.clone(),
                entity: next.entity.clone(),
                enchantments: next.enchantments.clone(),
                placement: Some(source.saved.placement.clone()),
                source_destination: source.saved.source_destination,
                persisted_version: source.persisted_version,
            }
        } else {
            let template = generated
                .iter()
                .find(|t| t.entity.object_id == id)
                .ok_or_else(invalid)?;
            if change.before.is_some() || template.entity.state.weenie_id != change.after.template {
                return Err(invalid());
            }
            let next = staged_item.ok_or_else(invalid)?;
            FrozenInventoryItem {
                corpse: None,
                construction: next.construction.clone(),
                entity: next.entity.clone(),
                enchantments: next.enchantments.clone(),
                placement: None,
                source_destination: None,
                persisted_version: 0,
            }
        };
        frozen.push(item);
    }
    if staged
        .changes
        .iter()
        .any(|s| !inventory.changes.iter().any(|c| c.after.id.0 == s.item))
    {
        return Err(invalid());
    }
    let other_snapshots: Vec<_> = staged
        .snapshots
        .iter()
        .filter(|s| !staged.changes.iter().any(|c| c.item == s.object_id))
        .cloned()
        .collect();
    let mut result = freeze_inventory(InventoryFreezeInput {
        operation_id: &staged.operation_id,
        proposal: inventory,
        items: &frozen,
        other_snapshots: &other_snapshots,
        leases: &staged.leases,
        storage_views: &[],
        admitted_positions: &BTreeMap::new(),
    })
    .map_err(|_| invalid())?;
    let mut all: BTreeSet<_> = result.participants.iter().copied().collect();
    all.extend(staged.participants);
    if all.len() > 1024 {
        return Err(invalid());
    }
    result.participants = all.into_iter().collect();
    Ok(result)
}

fn participants(
    ids: &[u32],
    actor: u32,
    lease: CharacterLease,
) -> Result<BTreeSet<u32>, SaveCodecError> {
    let set: BTreeSet<_> = ids.iter().copied().collect();
    if ids.len() > 1024
        || set.len() != ids.len()
        || set.contains(&0)
        || set.contains(&u32::MAX)
        || !set.contains(&actor)
        || lease.character_id != actor
        || lease.epoch <= 0
        || lease.state != bace_persistence::OwnershipState::Online
    {
        return Err(invalid());
    }
    Ok(set)
}
fn apply_properties(
    props: &mut SparseProperties,
    before: &BTreeMap<PropertyKey, PropertyValue>,
    after: &BTreeMap<PropertyKey, PropertyValue>,
    item_tinker_count: Option<u32>,
) -> Result<(), SaveCodecError> {
    for key in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        if before.get(key) == after.get(key) {
            continue;
        }
        if item_tinker_count.is_some()
            && (key.kind == PropertyKind::InstanceId && [1, 2, 3].contains(&key.id)
                || key.kind == PropertyKind::Int && [10, 12, 53].contains(&key.id)
                || key.kind == PropertyKind::String && key.id == 9007)
        {
            return Err(SaveCodecError::Invalid(
                "recipe modifies reserved identity or counter property",
            ));
        }
        if let Some(count) = item_tinker_count
            && key.kind == PropertyKind::Int
            && key.id == 171
            && after.get(key)
                != Some(&PropertyValue::Int(
                    i32::try_from(count).map_err(|_| invalid())?,
                ))
        {
            return Err(SaveCodecError::Invalid(
                "recipe tinker count differs from accepted item",
            ));
        }
        match (key.kind, after.get(key)) {
            (PropertyKind::Bool, Some(PropertyValue::Bool(v))) => set(&mut props.bools, key.id, *v),
            (PropertyKind::Int, Some(PropertyValue::Int(v))) => set(&mut props.ints, key.id, *v),
            (PropertyKind::Int64, Some(PropertyValue::Int64(v))) => {
                set(&mut props.int64s, key.id, *v)
            }
            (PropertyKind::Float, Some(PropertyValue::Float(v))) => {
                set(&mut props.floats, key.id, *v)
            }
            (PropertyKind::String, Some(PropertyValue::String(v))) => {
                set(&mut props.strings, key.id, v.clone())
            }
            (PropertyKind::DataId, Some(PropertyValue::DataId(v))) => {
                set(&mut props.data_ids, key.id, *v)
            }
            (PropertyKind::InstanceId, Some(PropertyValue::InstanceId(v))) => {
                set(&mut props.instance_ids, key.id, *v)
            }
            (PropertyKind::SpellBook, Some(PropertyValue::SpellBook(true))) => {
                let id = i32::try_from(key.id).map_err(|_| invalid())?;
                if id == 0 {
                    return Err(invalid());
                }
                // ACE BiotaExtensions.GetOrAddKnownSpell preserves an existing
                // probability; RecipeManager.AddSpell supplies its 2.0 default.
                if !props.spell_book.iter().any(|spell| spell.id == id) {
                    if props.spell_book.len() >= 4096 {
                        return Err(invalid());
                    }
                    props.spell_book.push(Property { id, value: 2.0 });
                }
            }
            (kind, None) => match kind {
                PropertyKind::Bool => props.bools.retain(|p| p.id != key.id),
                PropertyKind::Int => props.ints.retain(|p| p.id != key.id),
                PropertyKind::Int64 => props.int64s.retain(|p| p.id != key.id),
                PropertyKind::Float => props.floats.retain(|p| p.id != key.id),
                PropertyKind::String => props.strings.retain(|p| p.id != key.id),
                PropertyKind::DataId => props.data_ids.retain(|p| p.id != key.id),
                PropertyKind::InstanceId => props.instance_ids.retain(|p| p.id != key.id),
                PropertyKind::SpellBook => props.spell_book.retain(|p| p.id as u32 != key.id),
            },
            _ => return Err(invalid()),
        }
    }
    Ok(())
}
fn set<T>(items: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(p) = items.iter_mut().find(|p| p.id == id) {
        p.value = value;
    } else {
        items.push(Property { id, value });
        items.sort_by_key(|p| p.id);
    }
}
fn remove(item: &mut ItemSaveV4) {
    item.placement = ItemPlacementV2::Removed;
    item.entity
        .state
        .properties
        .instance_ids
        .retain(|p| ![1, 2, 3].contains(&p.id));
    item.entity
        .state
        .properties
        .ints
        .retain(|p| ![10, 53].contains(&p.id));
    item.entity.state.properties.positions.retain(|p| p.id != 1);
}
fn durable(place: &ItemPlacementV2) -> DurableItemPlace {
    match place {
        ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } => DurableItemPlace::Contained {
            container: *container,
            slot: *slot,
            pack_slot: *pack_slot,
            equipped: *equipped,
        },
        ItemPlacementV2::World(p) => DurableItemPlace::World {
            cell: p.obj_cell_id,
        },
        ItemPlacementV2::Removed => DurableItemPlace::Removed,
    }
}

pub fn craft_item_snapshot(
    saved: &ItemSaveV4,
    root_owner: u32,
) -> Result<CraftItem, SaveCodecError> {
    let scalar = crafting_properties(&saved.entity.state.properties);
    let number = |id, default| -> Result<u32, SaveCodecError> {
        match scalar.get(&PropertyKey {
            kind: PropertyKind::Int,
            id,
        }) {
            Some(PropertyValue::Int(v)) => u32::try_from(*v).map_err(|_| invalid()),
            None => Ok(default),
            _ => Err(invalid()),
        }
    };
    let log = match scalar.get(&PropertyKey {
        kind: PropertyKind::String,
        id: 9007,
    }) {
        Some(PropertyValue::String(v)) if !v.is_empty() => v
            .split(',')
            .map(|x| x.parse::<u32>().map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => vec![],
    };
    Ok(CraftItem {
        id: saved.entity.object_id,
        owner: root_owner,
        revision: saved.entity.mutation_revision,
        stack: number(12, 1)?,
        equipped: matches!(saved.placement,ItemPlacementV2::Contained{equipped,..} if equipped!=0),
        in_trade: false,
        reserved: false,
        times_tinkered: number(171, 0)?,
        tinker_log: log,
        properties: scalar,
    })
}

#[cfg(test)]
mod tests;
