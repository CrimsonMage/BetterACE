//! Build only from a single accepted capture and preserved durable metadata.
use super::*;
use bace_crafting::{ChanceInput, CraftContext, PropertyKey, PropertyKind, PropertyValue};
use bace_storage_codec::{ItemSaveV2, ItemSaveV4};
pub(super) fn items(
    online: &OnlinePlayerSaveService,
    snapshot: &bace_simulation::PlayerReadSnapshot,
    source: u32,
    target: u32,
) -> Result<(ItemSaveV4, ItemSaveV4), String> {
    let all = online.captured_inventory_baselines(snapshot)?;
    let get = |id| -> Result<ItemSaveV4, String> {
        let row = all
            .iter()
            .find(|r| r.entity.object_id == id)
            .ok_or("crafting item not in accepted owned inventory")?;
        Ok(bace_storage_codec::ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: row.entity.clone(),
                    placement: row.placement.clone().ok_or("crafting placement missing")?,
                },
                enchantments: row.enchantments.clone(),
            },
            construction: row.construction.clone(),
        })
    };
    Ok((get(source)?, get(target)?))
}
pub(super) fn build(
    snapshot: &bace_simulation::PlayerReadSnapshot,
    saved: &bace_storage_codec::PlayerSaveV6,
    source: &ItemSaveV4,
    target: &ItemSaveV4,
    native: &bace_crafting::NativeRecipe,
    operation: [u8; 16],
) -> Result<TinkerCommandInput, String> {
    let source = crate::crafting_saves::craft_item_snapshot(source, snapshot.binding().actor.0)
        .map_err(|e| e.to_string())?;
    let target = crate::crafting_saves::craft_item_snapshot(target, snapshot.binding().actor.0)
        .map_err(|e| e.to_string())?;
    let skill = snapshot
        .skill_values()
        .iter()
        .find(|s| s.skill == native.source.skill)
        .ok_or("authoritative tinkering skill projection missing")?;
    let properties =
        crate::crafting_saves::crafting_properties(&saved.player.entity.state.properties);
    let integer = |id| match properties.get(&PropertyKey {
        kind: PropertyKind::Int,
        id,
    }) {
        Some(PropertyValue::Int(v)) => {
            u32::try_from(*v).map_err(|_| "negative crafting augmentation".to_string())
        }
        _ => Ok(0),
    };
    let material = match source.properties.get(&PropertyKey {
        kind: PropertyKind::Int,
        id: 131,
    }) {
        Some(PropertyValue::Int(v)) => {
            u32::try_from(*v).map_err(|_| "negative crafting material")?
        }
        _ => 0,
    };
    let workmanship = |item: &bace_crafting::CraftItem| -> Result<f32, String> {
        let int = |id, default| match item.properties.get(&PropertyKey {
            kind: PropertyKind::Int,
            id,
        }) {
            Some(PropertyValue::Int(v)) => *v,
            _ => default,
        };
        let n = int(170, 1);
        if n <= 0 {
            return Err("invalid source workmanship item count".into());
        }
        let result = int(105, 0) as f32 / n as f32;
        if !(1.0..=10.0).contains(&result) {
            return Err(
                "legacy workmanship repair requires a separate durable item correction".into(),
            );
        }
        Ok(result)
    };
    let chance = ChanceInput {
        skill: skill.current,
        trained: matches!(
            skill.advancement,
            bace_gameplay_api::SkillAdvancement::Trained
                | bace_gameplay_api::SkillAdvancement::Specialized
        ),
        lum_craft: integer(343)?,
        tool_workmanship: workmanship(&source)?,
        target_workmanship: workmanship(&target)?,
        material,
        times_tinkered: target.times_tinkered,
        imbue: native.source.salvage_type == 2,
        imbue_augmentation: integer(236)? > 0,
        foolproof: bace_crafting::is_foolproof_tinker(source_template(source.id, snapshot)?),
    };
    Ok(TinkerCommandInput {
        context: CraftContext {
            actor: snapshot.binding().actor.0,
            actor_revision: snapshot.character().progression().revision(),
            character_random_id: snapshot
                .character()
                .rares()
                .ok_or("character random identity missing")?
                .random_identity,
            operation_id: operation,
            busy: false,
            peace_mode: snapshot.combat_mode() == Some(1),
            chance,
            properties,
        },
        source,
        target,
        recipe: Arc::new(native.recipe.clone()),
    })
}
fn source_template(id: u32, snapshot: &bace_simulation::PlayerReadSnapshot) -> Result<u32, String> {
    snapshot
        .items()
        .iter()
        .find(|i| i.id.0 == id)
        .map(|i| i.template)
        .ok_or("crafting source identity missing".into())
}
