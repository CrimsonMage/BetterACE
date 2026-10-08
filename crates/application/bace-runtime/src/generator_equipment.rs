//! Immutable creature equipment preparation. Gameplay selection is owned by loot;
//! this adapter binds reserved identities to inventory and combat projections.
use bace_content::{TreasureWieldedRowV1, WeenieV1};
use bace_gameplay_api::GeneratorSpawnIntent;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_loot::{PreparedCreatureEquipment, WieldedTreasure};
use bace_random::RandomRoot;
use bace_simulation::PreparedNpcLoadout;
use bace_types::EntityId;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
/// Pinned default server policy: plain Wield rows do not drop at death.
pub fn materialize_creature_equipment(
    source: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    wielded: &[TreasureWieldedRowV1],
    root: &RandomRoot,
    intent: &GeneratorSpawnIntent,
) -> Result<Vec<PreparedCreatureEquipment>, String> {
    materialize_creature_equipment_with_inventory(
        source, templates, wielded, None, false, root, intent,
    )
}
/// DID33 dispatch must already be prepared with Death-first/Wielded-second rules.
/// Inventory treasure uses the same per-occurrence stream, after both equip passes.
pub fn materialize_creature_equipment_with_inventory(
    source: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    wielded: &[TreasureWieldedRowV1],
    inventory: Option<&crate::generator_treasure::PreparedGeneratorTreasure>,
    drop_plain_wield: bool,
    root: &RandomRoot,
    intent: &GeneratorSpawnIntent,
) -> Result<Vec<PreparedCreatureEquipment>, String> {
    let table = source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 32 && p.value != 0)
        .map(|p| p.value);
    let wielded = match table {
        Some(id) => {
            if wielded.is_empty() || wielded.iter().any(|r| r.treasure_type != id) {
                return Err("missing or mismatched creature wielded treasure".into());
            }
            Some(
                WieldedTreasure::prepare(wielded.to_vec(), |id| templates.get(&id).cloned())
                    .map_err(|e| format!("creature wielded treasure: {e:?}"))?,
            )
        }
        None => None,
    };
    let inventory_id = source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 33 && p.value != 0)
        .map(|p| p.value);
    let has_inventory = inventory_id.is_some();
    if has_inventory && inventory.and_then(|p| p.treasure_type()) != inventory_id {
        return Err("missing or mismatched prepared creature inventory treasure".into());
    }
    let mut rng = bace_spawning::generator_event_stream(root, intent, 0x20000)
        .map_err(|e| format!("creature random: {e:?}"))?;
    let mut result = bace_loot::generate_creature_equipment(
        source,
        templates,
        wielded.as_ref(),
        &[],
        drop_plain_wield,
        &mut rng,
    )
    .map_err(|e| format!("creature equipment: {e:?}"))?;
    if has_inventory {
        let generated = inventory
            .ok_or("missing inventory treasure")?
            .generate(&mut rng)
            .map_err(|e| format!("creature inventory treasure: {e:?}"))?;
        bace_loot::append_creature_inventory(source, templates, &mut result, generated)
            .map_err(|e| format!("creature inventory admission: {e:?}"))?;
    }
    Ok(result)
}
fn int(w: &WeenieV1, id: u32, default: i32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn positive(w: &WeenieV1, id: u32, default: i32) -> Result<u32, String> {
    u32::try_from(int(w, id, default)).map_err(|_| format!("negative creature item property {id}"))
}
/// ACE compares ItemCapacity/ContainerCapacity as signed counts. An authored -1
/// admits no direct slots; retain the original property on the source image.
fn inventory_capacity(w: &WeenieV1, id: u32) -> Result<u32, String> {
    match int(w, id, 0) {
        -1 => Ok(0),
        value if value >= 0 => Ok(value as u32),
        _ => Err(format!("invalid creature capacity property {id}")),
    }
}
fn boolean(w: &WeenieV1, id: u32) -> bool {
    w.properties.bools.iter().any(|p| p.id == id && p.value)
}
/// Every item ID must be a reserved child of this spawn admission. The returned
/// aggregate is installed atomically by the single simulation owner.
pub fn prepare_creature_loadout(
    actor: EntityId,
    source: &WeenieV1,
    items: &[(EntityId, PreparedCreatureEquipment)],
    prepared: &bace_simulation::GeneratedNpcTemplate,
    projectile_shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
) -> Result<PreparedNpcLoadout, String> {
    prepare_creature_loadout_with_enchantments(
        actor,
        source,
        items,
        prepared,
        projectile_shapes,
        None,
    )
}
#[derive(Clone, Copy)]
pub struct GeneratorItemSpellAssets<'a> {
    pub table: &'a bace_dat::SpellTable,
    pub rows: &'a BTreeMap<u32, bace_content::SpellRowV1>,
}
pub fn prepare_creature_loadout_with_enchantments(
    actor: EntityId,
    source: &WeenieV1,
    items: &[(EntityId, PreparedCreatureEquipment)],
    prepared: &bace_simulation::GeneratedNpcTemplate,
    projectile_shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
    spells: Option<GeneratorItemSpellAssets<'_>>,
) -> Result<PreparedNpcLoadout, String> {
    prepare_creature_loadout_impl(
        actor,
        source,
        items,
        prepared,
        projectile_shapes,
        spells,
        true,
    )
}
/// Restore existing custody without replaying source TryWieldObject spell casts.
/// The admission caller must supply exact restored registry images.
pub fn prepare_restored_creature_loadout(
    actor: EntityId,
    source: &WeenieV1,
    items: &[(EntityId, PreparedCreatureEquipment)],
    prepared: &bace_simulation::GeneratedNpcTemplate,
    projectile_shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
) -> Result<PreparedNpcLoadout, String> {
    prepare_creature_loadout_impl(
        actor,
        source,
        items,
        prepared,
        projectile_shapes,
        None,
        false,
    )
}
fn prepare_creature_loadout_impl(
    actor: EntityId,
    source: &WeenieV1,
    items: &[(EntityId, PreparedCreatureEquipment)],
    prepared: &bace_simulation::GeneratedNpcTemplate,
    projectile_shapes: &BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
    spells: Option<GeneratorItemSpellAssets<'_>>,
    replay_enchantments: bool,
) -> Result<PreparedNpcLoadout, String> {
    let base = prepared
        .physical
        .as_ref()
        .ok_or("missing prepared creature profile")?;
    if actor.0 == 0 || items.len() > 1024 || base.player {
        return Err("invalid creature loadout identity".into());
    }
    let mut seen = BTreeSet::new();
    let mut projected = Vec::with_capacity(items.len());
    let mut containers = Vec::new();
    let mut equipment = Vec::new();
    let mut death_parents = Vec::new();
    let mut death_indices = BTreeMap::new();
    let mut death_items = Vec::new();
    let mut death_ids = Vec::new();
    containers.push(InventoryContainer {
        id: actor,
        revision: 1,
        root_owner: None,
        slots: inventory_capacity(source, 6)?,
        pack_slots: inventory_capacity(source, 7)?,
        burden_limit: u64::MAX,
        accessible: false,
        open: false,
        generation: 1,
    });
    for (index, (id, item)) in items.iter().enumerate() {
        if id.0 == 0 || *id == actor || !seen.insert(*id) {
            return Err("duplicate creature equipment identity".into());
        }
        let w = &item.source;
        let pack = boolean(w, 81);
        let location = item.wielded_location;
        let slot = item.inventory_slot;
        let container = if let Some(parent) = item.parent_index {
            if parent >= index
                || location != 0
                || !items[parent]
                    .1
                    .source
                    .properties
                    .ints
                    .iter()
                    .any(|p| p.id == 6)
            {
                return Err("invalid prepared creature item parent".into());
            }
            items[parent].0
        } else {
            actor
        };
        let is_container = w.properties.ints.iter().any(|p| p.id == 6);
        let mut identity = w.clone();
        identity
            .properties
            .ints
            .retain(|p| !matches!(p.id, 5 | 12 | 19 | 10));
        identity
            .properties
            .instance_ids
            .retain(|p| !matches!(p.id, 2..=4));
        let hash = Sha256::digest(
            bace_content_tools::compile_template(&identity).map_err(|e| e.to_string())?,
        );
        let stackable = bace_loot::is_stackable(w.weenie_type);
        let unique = positive(w, 279, 0)?;
        if unique > 1 {
            return Err("unsupported multiple unique creature item bound".into());
        }
        projected.push(InventoryItem {
            structure: w
                .properties
                .ints
                .iter()
                .find(|p| p.id == 92)
                .map(|p| u32::try_from(p.value))
                .transpose()
                .map_err(|_| "invalid item structure")?,
            id: *id,
            revision: 1,
            template: w.weenie_id,
            stack_key: u64::from_le_bytes(hash[..8].try_into().map_err(|_| "invalid stack key")?),
            place: ItemPlace::Contained {
                container,
                slot,
                equipped: location,
            },
            stack: positive(w, 12, 1)?,
            maximum_stack: positive(w, 11, 1)?,
            unit_burden: positive(w, 13, if stackable { 0 } else { int(w, 5, 0) })?,
            unit_value: positive(w, 15, if stackable { 0 } else { int(w, 19, 0) })?,
            pack_slot: pack,
            is_container,
            attuned: int(w, 114, 0) != 0,
            trade_reserved: false,
            active_pet: false,
            unique: unique == 1,
            quest_allowed: true,
            valid_wield: int(w, 9, 0) as u32,
            incompatible_wield: 0,
            wield_requirements_met: true,
        });
        if is_container {
            containers.push(InventoryContainer {
                id: *id,
                revision: 1,
                root_owner: None,
                slots: inventory_capacity(w, 6)?,
                pack_slots: inventory_capacity(w, 7)?,
                burden_limit: u64::MAX,
                accessible: false,
                open: false,
                generation: 1,
            });
        }
        if location != 0 {
            equipment.push(bace_combat::preparation::PhysicalEquipment {
                entity: id.0,
                revision: 1,
                location,
                weenie: w,
            });
        }
        let death_parent = item
            .parent_index
            .and_then(|p| death_indices.get(&p).copied());
        if item.parent_index.is_none() && item.death_drop || death_parent.is_some() {
            death_indices.insert(index, death_items.len());
            death_items.push(w.clone());
            death_ids.push(*id);
            death_parents.push(death_parent);
        }
    }
    let mut enchantments = Vec::new();
    if replay_enchantments
        && source
            .properties
            .bools
            .iter()
            .find(|p| p.id == 19)
            .is_none_or(|p| p.value)
    {
        let mut equipped: Vec<_> = items
            .iter()
            .filter(|(_, i)| i.wielded_location != 0 && !i.source.properties.spell_book.is_empty())
            .collect();
        equipped.sort_by_key(|(_, i)| i.equip_order);
        for (id, item) in equipped {
            let assets = spells.ok_or("missing prepared generated item spell assets")?;
            enchantments.extend(
                crate::generator_enchantments::prepare_generator_item_enchantments(
                    actor,
                    *id,
                    &item.source,
                    assets.table,
                    assets.rows,
                )?,
            );
            if enchantments.len() > 1024 {
                return Err("generated item spell capacity".into());
            }
        }
    }
    let mut attributes = [0; 6];
    for (id, out) in attributes.iter_mut().enumerate() {
        let row = source
            .properties
            .attributes
            .iter()
            .find(|p| p.id == id as u32 + 1)
            .ok_or("missing creature attribute")?;
        *out = row
            .value
            .init_level
            .checked_add(row.value.level_from_cp)
            .ok_or("creature attribute overflow")?;
    }
    let mut profile =
        bace_combat::preparation::prepare_physical(bace_combat::preparation::PhysicalPreparation {
            actor: actor.0,
            revision: base.revision,
            content_hash: base.content_hash,
            player: false,
            weenie: source,
            equipment: &equipment,
            skills: &base.skills,
            attributes,
            base_attributes: attributes,
            qualities: &[],
            enchantments_complete: true,
            maneuvers: base.maneuvers.clone(),
            height: base.height,
            missile: crate::generator_equipment_combat::prepare_creature_missile(
                prepared,
                items,
                projectile_shapes,
            )?,
            melee_defense_modifier: base.melee_defense_modifier,
            missile_defense_modifier: base.missile_defense_modifier,
        })
        .map_err(|e| format!("creature combat equipment: {e:?}"))?;
    profile.range = base.range;
    Ok(PreparedNpcLoadout {
        combat_assets: None,
        death_motions: vec![],
        physical_motions: vec![],
        locomotion_styles: vec![],
        items: projected,
        containers,
        profile: Arc::new(profile),
        death_items,
        death_parents,
        death_ids,
        enchantments,
    })
}
