//! Source appearance composition from Creature_Networking.CalculateObjDesc and
//! WorldObject_Networking.AddBaseModelData at the official ACE pin.
use super::object::{PreparedEntryModel, Properties};
use bace_content::WeenieV1;
use bace_dat::{CharGen, ClothingTable, DatPaletteSet, ModelSetup};
use bace_wire::{ModelPalette, ModelPart, ModelTexture, ObjectModel};
use std::collections::BTreeMap;

/// Fingerprint-verified immutable DAT closure. Missing referenced assets reject
/// entry instead of generating a partially dressed avatar.
pub struct EntryAppearanceAssets<'a> {
    pub chargen: &'a CharGen,
    pub setups: &'a BTreeMap<u32, ModelSetup>,
    pub clothing: &'a BTreeMap<u32, ClothingTable>,
    pub palettes: &'a BTreeMap<u32, DatPaletteSet>,
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerAppearanceOptions {
    pub show_helm: bool,
    pub show_cloak: bool,
    pub default_hair_texture: u32,
    pub hair_texture: u32,
}

pub fn prepare_player_model(
    source: &WeenieV1,
    equipment: &[&WeenieV1],
    options: PlayerAppearanceOptions,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedEntryModel, String> {
    creature_model(source, equipment, Some(options), assets)
}
pub fn prepare_creature_model(
    source: &WeenieV1,
    equipment: &[&WeenieV1],
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedEntryModel, String> {
    creature_model(source, equipment, None, assets)
}
fn creature_model(
    source: &WeenieV1,
    equipment: &[&WeenieV1],
    options: Option<PlayerAppearanceOptions>,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedEntryModel, String> {
    if equipment.len() > 128 {
        return Err("entry equipment capacity exceeded".into());
    }
    let mut model = base(source, options, assets)?;
    let p = Properties(source);
    let setup = p.did("Setup").ok_or("player appearance has no setup")?;
    let fallback = fallback_setup(setup);
    let mut coverage = Vec::new();
    let mut armor = Vec::new();
    let mut clothing = Vec::new();
    for item in equipment {
        let q = Properties(item);
        let item_type = q.int("ItemType").unwrap_or(0);
        let loc = q.int("CurrentWieldedLocation").unwrap_or(0) as u32;
        if item_type == 2 || loc & 0x7f21 != 0 {
            let priority = visual_priority(item, assets)?;
            let layer = match q.bool("TopLayerPriority") {
                Some(false) => 0,
                None => 1,
                Some(true) => 2,
            };
            armor.push((*item, layer, priority));
        } else if item_type == 4 {
            clothing.push(*item);
        }
    }
    armor.sort_by_key(|(_, layer, priority)| (*layer, *priority));
    clothing.sort_by_key(|v| Properties(v).int("ClothingPriority"));
    clothing.extend(armor.into_iter().map(|(v, _, _)| v));
    if clothing.is_empty()
        && (!source.properties.animation_parts.is_empty()
            || !source.properties.palettes.is_empty()
            || !source.properties.texture_maps.is_empty())
    {
        add_stored(&mut model, source);
        return finish(model, None);
    }
    for item in clothing {
        let q = Properties(item);
        let loc = q.int("CurrentWieldedLocation").unwrap_or(0) as u32;
        if (loc == 1 && options.is_some_and(|o| !o.show_helm))
            || (loc == 0x08000000 && options.is_some_and(|o| !o.show_cloak))
            || loc & 0x88007fff == 0
        {
            continue;
        }
        let Some(clo) = q.did("ClothingBase") else {
            let setup_id = q.did("Setup").ok_or("equipped appearance has no setup")?;
            let parts = &assets
                .setups
                .get(&setup_id)
                .ok_or("equipped setup absent from verified closure")?
                .parts;
            if parts.len() > 255 {
                return Err("setup part capacity exceeded".into());
            }
            for (i, id) in parts.iter().enumerate() {
                if i != 16 || *id != 0x010001ec {
                    model.parts.push(ModelPart {
                        part_index: i as u8,
                        animation_id: *id,
                    });
                }
            }
            for part in &model.parts {
                if !coverage.contains(&part.part_index) {
                    coverage.push(part.part_index);
                }
            }
            continue;
        };
        let table = assets
            .clothing
            .get(&clo)
            .ok_or("equipped clothing absent from verified closure")?;
        if let Some(parts) = table
            .setups
            .get(&setup)
            .or_else(|| table.setups.get(&fallback))
        {
            for part in parts {
                let index = u8::try_from(part.index).map_err(|_| "clothing part index overflow")?;
                coverage.push(index);
                let change = ModelPart {
                    part_index: index,
                    animation_id: part.model,
                };
                if let Some(i) = model.parts.iter().position(|v| *v == change) {
                    model.parts.remove(i);
                }
                model.parts.push(change);
                for (old, new) in &part.textures {
                    let change = ModelTexture {
                        part_index: index,
                        old_texture: *old,
                        new_texture: *new,
                    };
                    if !model.textures.contains(&change) {
                        model.textures.push(change);
                    }
                }
            }
            add_palettes(&mut model, item, table, assets)?;
        }
        bounded(&model)?;
    }
    let parts = &assets
        .setups
        .get(&setup)
        .ok_or("player setup absent from verified closure")?
        .parts;
    if parts.len() > 255 {
        return Err("player setup part capacity exceeded".into());
    }
    for (i, id) in parts.iter().enumerate() {
        if i != 16 && !coverage.contains(&(i as u8)) {
            model.parts.push(ModelPart {
                part_index: i as u8,
                animation_id: *id,
            });
        }
    }
    if coverage.is_empty() && p.did("ClothingBase").is_some() {
        return item_model(source, options, assets);
    }
    finish(model, None)
}

pub fn prepare_item_model(
    source: &WeenieV1,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedEntryModel, String> {
    item_model(source, None, assets)
}
fn item_model(
    source: &WeenieV1,
    player: Option<PlayerAppearanceOptions>,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<PreparedEntryModel, String> {
    let mut model = base(source, player, assets)?;
    let p = Properties(source);
    let mut icon = None;
    if let Some(id) = p.did("ClothingBase") {
        let table = assets
            .clothing
            .get(&id)
            .ok_or("item clothing absent from verified closure")?;
        if let Some(parts) = table.setups.get(&p.did("Setup").unwrap_or(0)) {
            for part in parts {
                let index = u8::try_from(part.index).map_err(|_| "clothing index overflow")?;
                model.parts.push(ModelPart {
                    part_index: index,
                    animation_id: part.model,
                });
                for (old, new) in &part.textures {
                    model.textures.push(ModelTexture {
                        part_index: index,
                        old_texture: *old,
                        new_texture: *new,
                    });
                }
            }
            if p.float("Shade").is_some() || p.int("PaletteTemplate").is_some() {
                icon = add_palettes(&mut model, source, table, assets)?
                    .filter(|v| *v != 0 && !p.bool("IgnoreCloIcons").unwrap_or(false));
            }
        }
    }
    finish(model, icon)
}
fn base(
    source: &WeenieV1,
    player: Option<PlayerAppearanceOptions>,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<ObjectModel, String> {
    let p = Properties(source);
    let mut model = ObjectModel::default();
    let heritage = p.int("HeritageGroup");
    let hair = p.int("Hairstyle");
    if let Some(head) = p
        .did("HeadObject")
        .filter(|_| hair.is_none() && heritage.is_some() && heritage != Some(6))
    {
        model.parts.push(ModelPart {
            part_index: 16,
            animation_id: head,
        });
    } else if let (Some(hair), Some(heritage), Some(gender)) = (hair, heritage, p.int("Gender")) {
        let sex = assets
            .chargen
            .heritage_groups
            .get(&(heritage as u32))
            .and_then(|h| h.genders.get(&gender))
            .ok_or("saved body style has no DAT gender")?;
        let index = usize::try_from(hair).map_err(|_| "negative saved body style")?;
        if let Some(style) = sex.hair_styles.get(index) {
            model.textures.extend(
                style
                    .appearance
                    .texture_changes
                    .iter()
                    .map(|v| ModelTexture {
                        part_index: v.part,
                        old_texture: v.old_texture,
                        new_texture: v.new_texture,
                    }),
            );
            model
                .parts
                .extend(style.appearance.animation_parts.iter().map(|v| ModelPart {
                    part_index: v.part,
                    animation_id: v.id,
                }));
        }
    }
    if let Some(player) = player {
        model.textures.push(ModelTexture {
            part_index: 16,
            old_texture: player.default_hair_texture,
            new_texture: player.hair_texture,
        });
    }
    for (name, offset, length) in [
        ("HairPalette", 0x18, 8),
        ("SkinPalette", 0, 0x18),
        ("EyesPalette", 0x20, 8),
    ] {
        if let Some(id) = p.did(name) {
            model.palettes.push(ModelPalette {
                palette_id: id,
                offset,
                length,
            });
        }
    }
    model.palette_id = Some(p.did("PaletteBase").unwrap_or(0));
    for (old, new) in [
        ("DefaultEyesTexture", "EyesTexture"),
        ("DefaultNoseTexture", "NoseTexture"),
        ("DefaultMouthTexture", "MouthTexture"),
    ] {
        if let (Some(old), Some(new)) = (p.did(old), p.did(new)) {
            model.textures.push(ModelTexture {
                part_index: 16,
                old_texture: old,
                new_texture: new,
            });
        }
    }
    bounded(&model)?;
    Ok(model)
}
fn add_stored(model: &mut ObjectModel, source: &WeenieV1) {
    model
        .parts
        .extend(source.properties.animation_parts.iter().map(|v| ModelPart {
            part_index: v.index,
            animation_id: v.animation_id,
        }));
    model
        .palettes
        .extend(source.properties.palettes.iter().map(|v| ModelPalette {
            palette_id: v.sub_palette_id,
            offset: v.offset as u8,
            length: v.length as u8,
        }));
    model
        .textures
        .extend(source.properties.texture_maps.iter().map(|v| ModelTexture {
            part_index: v.part_index,
            old_texture: v.old_texture,
            new_texture: v.new_texture,
        }));
}
fn add_palettes(
    model: &mut ObjectModel,
    source: &WeenieV1,
    table: &ClothingTable,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<Option<u32>, String> {
    if table.templates.is_empty() {
        return Ok(None);
    }
    let p = Properties(source);
    let key = p.int("PaletteTemplate").unwrap_or(0) as u32;
    let template = table
        .templates
        .get(&key)
        .or_else(|| {
            table
                .template_order
                .first()
                .and_then(|id| table.templates.get(id))
        })
        .ok_or("clothing template fallback order missing")?;
    let shade = p.float("Shade").unwrap_or(0.0) as f32;
    if !shade.is_finite() {
        return Err("nonfinite clothing shade".into());
    }
    for palette in &template.palettes {
        let set = assets
            .palettes
            .get(&palette.palette_set)
            .ok_or("clothing palette set missing from verified closure")?;
        // ACE's GetPaletteID returns zero for an out-of-range shade or empty set.
        let id = set.at_shade(f64::from(shade)).unwrap_or(0) as u16;
        for (offset, length) in &palette.ranges {
            model.palettes.push(ModelPalette {
                palette_id: u32::from(id),
                offset: (offset / 8) as u8,
                length: (length / 8) as u8,
            });
        }
    }
    bounded(model)?;
    Ok(Some(template.icon))
}
fn visual_priority(
    source: &WeenieV1,
    assets: &EntryAppearanceAssets<'_>,
) -> Result<Option<u32>, String> {
    let p = Properties(source);
    let loc = p.int("CurrentWieldedLocation").unwrap_or(0) as u32;
    if let Some(id) = p.did("ClothingBase").filter(|_| loc & 0x7f21 != 0) {
        let table = assets
            .clothing
            .get(&id)
            .ok_or("priority clothing missing from verified closure")?;
        if let Some(parts) = table.setups.get(&0x02000001) {
            let mut result = 0;
            for part in parts {
                result |= match part.index {
                    0 => 0x800,
                    1 | 5 => 0x100,
                    2 | 6 => 0x200,
                    3 | 4 | 7 | 8 => 0x10000,
                    9 => 0x400,
                    10 | 13 => 0x1000,
                    11 | 14 => 0x2000,
                    12 | 15 => 0x8000,
                    16 => 0x4000,
                    _ => 0,
                };
            }
            return Ok(Some(result));
        }
        return Ok(p.int("ClothingPriority").map(|v| v as u32));
    }
    Ok(p.int("VisualClothingPriority")
        .or_else(|| p.int("ClothingPriority"))
        .map(|v| v as u32))
}
fn bounded(model: &ObjectModel) -> Result<(), String> {
    if model.parts.len() > 255 || model.textures.len() > 255 || model.palettes.len() > 255 {
        Err("entry appearance byte-count capacity exceeded".into())
    } else {
        Ok(())
    }
}
fn finish(
    mut model: ObjectModel,
    icon_override: Option<u32>,
) -> Result<PreparedEntryModel, String> {
    bounded(&model)?;
    if model.palettes.is_empty() {
        model.palette_id = None;
    }
    model
        .encode(765)
        .map_err(|e| format!("invalid entry appearance: {e:?}"))?;
    Ok(PreparedEntryModel {
        model,
        icon_override,
    })
}
fn fallback_setup(id: u32) -> u32 {
    match id {
        0x02001972 | 0x02001a5f | 0x02001a6f => 0x0200196f,
        0x02001a5e | 0x2001a6ee => 0x02001970,
        0x02001971 | 0x02001a5d | 0x02001a70 => 0x0200196e,
        0x02001a5c | 0x02001a71 => 0x0200196d,
        0x02001a0f | 0x02001a9c | 0x02001a9e | 0x02001a9d | 0x02001a96 => 0x02001a0e,
        0x02001a0d | 0x02001aa0 | 0x02001a9f | 0x02001aa1 | 0x02001aa2 => 0x02001a0c,
        0x02001aa3 => 0x02000001,
        0x02001aa4 => 0x0200004e,
        _ => id,
    }
}
