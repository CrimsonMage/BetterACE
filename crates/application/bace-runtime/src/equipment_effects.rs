//! Cold equipment sources and lossless valuable-operation overlays. The accepted
//! simulation patch, never recomputed spell effects, supplies frozen changes.
use bace_content::{SpellRowV1, WeenieV1};
use bace_dat::SpellTable;
use bace_simulation::{
    EquipmentEffectsPatch, PreparedEquipmentEffectInputs, PreparedEquipmentItemEffects,
    PreparedItemExperience,
};
use bace_storage_codec::ItemSaveV4;
use bace_types::EntityId;
use std::collections::{BTreeMap, BTreeSet};
mod freezing;
pub use freezing::{
    freeze_equipment_inventory, overlay_equipment_candidate, overlay_equipment_item_sources,
    overlay_equipment_player_registry,
};
pub fn prepare_equipment_effect_inputs(
    actor: EntityId,
    before_revision: u64,
    items: &[ItemSaveV4],
    existing: &[PreparedItemExperience],
    spells: &SpellTable,
    rows: &BTreeMap<u32, SpellRowV1>,
) -> Result<PreparedEquipmentEffectInputs, String> {
    if actor.0 == 0 || items.len() > 128 || existing.len() > 1024 {
        return Err("equipment effect input capacity".into());
    }
    let mut ids = BTreeSet::new();
    let mut prepared = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let id = EntityId(item.entity.object_id);
        if !ids.insert(id) || id == actor {
            return Err("equipment effect duplicate identity".into());
        }
        let source = &item.entity.state;
        let mut spell_ids = BTreeSet::new();
        if source.properties.spell_book.len() > 256 {
            return Err("equipment spell capacity".into());
        }
        for known in &source.properties.spell_book {
            let id = u32::try_from(known.id).map_err(|_| "invalid equipment spell ID")?;
            if id == 0 || !spell_ids.insert(id) {
                return Err("duplicate/zero equipment spell ID".into());
            }
        }

        let order = existing
            .iter()
            .find(|p| p.item == id)
            .map_or(index as u64 + 1, |p| p.equipment_order);
        let experience = crate::item_experience::prepare_item_experience(
            actor,
            id,
            item.entity.mutation_revision,
            order,
            source,
            spells,
            rows,
        )?;
        let proc = source
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 55)
            .map(|p| p.value);
        let spell_did = source
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 28)
            .map(|p| p.value);
        let mut normal = source.clone();
        normal
            .properties
            .spell_book
            .retain(|p| u32::try_from(p.id).ok() != proc);
        let removals = entries(actor, id, &normal, spells, rows)?;
        normal
            .properties
            .spell_book
            .retain(|p| u32::try_from(p.id).ok() != spell_did);
        let activations = entries(actor, id, &normal, spells, rows)?;
        let mana = crate::equipment_mana::prepare_equipment_mana_item(actor, id, source, spells)?;
        prepared.push(PreparedEquipmentItemEffects {
            mana,
            item: id,
            before_revision: item.entity.mutation_revision,
            removals,
            activations,
            experience,
            gear_health: source
                .properties
                .ints
                .iter()
                .find(|p| p.id == 379)
                .map(|p| p.value.max(0) as u32),
            current_mana: source
                .properties
                .ints
                .iter()
                .find(|p| p.id == 107)
                .map(|p| p.value),
            affecting: source
                .properties
                .bools
                .iter()
                .find(|p| p.id == 56)
                .map(|p| p.value),
            activation: crate::player_assets::prepare_item_activation_requirements(source)?,
        });
    }
    Ok(PreparedEquipmentEffectInputs {
        actor,
        before_revision,
        items: prepared,
    })
}
fn entries(
    actor: EntityId,
    item: EntityId,
    source: &WeenieV1,
    spells: &SpellTable,
    rows: &BTreeMap<u32, SpellRowV1>,
) -> Result<Vec<bace_simulation::PreparedGeneratorEnchantment>, String> {
    let mut entries = crate::generator_enchantments::prepare_generator_item_enchantments(
        actor, item, source, spells, rows,
    )?;
    for entry in &mut entries {
        let definition =
            crate::player_assets::player_enchantment_definition(entry.entry.spell, spells, rows)
                .ok_or("equipment spell definition missing")?;
        entry.entry.is_level8_aura = definition.is_level8_aura;
        entry.entry.is_set_spell = definition.is_set_spell;
    }
    Ok(entries)
}
