//! CustomClothingBase legacy JSON boundary. Native saves use ClothingPatchV1 TOML.
//! Format reference: OptimShi/CustomClothingBase 122145d0f6d0c159183f0f229aae611085a67bf1.
use crate::clothing_json::{self as json, Node};
use bace_content::{
    ClothingPalette, ClothingPaletteEffect, ClothingPart, ClothingPatchV1, ClothingRange,
    ClothingSetup, ClothingTexture, PaletteSource,
};
pub const MAX_CLOTHING_BYTES: usize = 1024 * 1024;
pub fn import_clothing_json(text: &str) -> Result<ClothingPatchV1, String> {
    if text.len() > MAX_CLOTHING_BYTES {
        return Err("ClothingBase exceeds 1 MiB".into());
    }
    let n: Node =
        serde_json::from_slice(&json::without_trailing_commas(text)).map_err(|e| e.to_string())?;
    let mut root = json::object(n)?;
    let id = json::uint(&mut root, "Id")?;
    let setups = json::take(&mut root, "ClothingBaseEffects")
        .map(json::object)
        .transpose()?
        .unwrap_or_default();
    let palettes = json::take(&mut root, "ClothingSubPalEffects")
        .map(json::object)
        .transpose()?
        .unwrap_or_default();
    json::finish(root)?;
    let mut patch = ClothingPatchV1 {
        schema_version: 1,
        id,
        setups: Vec::new(),
        palettes: Vec::new(),
    };
    for (key, value) in setups {
        let mut value = json::object(value)?;
        let mut parts = Vec::new();
        for part in json::list(&mut value, "CloObjectEffects")? {
            let mut part = json::object(part)?;
            let index = json::uint(&mut part, "Index")?
                .try_into()
                .map_err(|_| "Part index exceeds 255")?;
            let model = json::uint(&mut part, "ModelId")?;
            let mut textures = Vec::new();
            for texture in json::list(&mut part, "CloTextureEffects")? {
                let mut texture = json::object(texture)?;
                textures.push(ClothingTexture {
                    old: json::uint(&mut texture, "OldTexture")?,
                    new: json::uint(&mut texture, "NewTexture")?,
                });
                json::finish(texture)?;
            }
            json::finish(part)?;
            parts.push(ClothingPart {
                index,
                model,
                textures,
            });
        }
        json::finish(value)?;
        patch.setups.push(ClothingSetup {
            id: json::parse_id(&key)?,
            parts,
        });
    }
    for (key, value) in palettes {
        let mut value = json::object(value)?;
        let icon = json::uint(&mut value, "Icon")?;
        let mut effects = Vec::new();
        for effect in json::list(&mut value, "CloSubPalettes")? {
            let mut effect = json::object(effect)?;
            let id = json::uint(&mut effect, "PaletteSet")?;
            let source = match id >> 24 {
                4 => PaletteSource::Palette(id),
                15 => PaletteSource::PaletteSet(id),
                _ => {
                    return Err(
                        "PaletteSet must reference a palette (04) or palette set (0F)".into(),
                    );
                }
            };
            let mut ranges = Vec::new();
            for range in json::list(&mut effect, "Ranges")? {
                let mut range = json::object(range)?;
                ranges.push(ClothingRange {
                    offset: json::uint(&mut range, "Offset")?,
                    colors: json::uint(&mut range, "NumColors")?,
                });
                json::finish(range)?;
            }
            json::finish(effect)?;
            effects.push(ClothingPaletteEffect { source, ranges });
        }
        json::finish(value)?;
        patch.palettes.push(ClothingPalette {
            template: json::parse_id(&key)?,
            icon,
            effects,
        });
    }
    patch.validate().map_err(|e| e.to_string())?;
    Ok(patch)
}
