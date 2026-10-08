//! Resolve each load from the immutable DAT plus the current native override.
use bace_content::{
    ClothingPalette, ClothingPaletteEffect, ClothingPart, ClothingPatchV1, ClothingRange,
    ClothingSetup, ClothingTexture, PaletteSource,
};
use bace_dat::{ClothingTable, DatArchive};
pub(crate) fn from_dat(table: ClothingTable) -> Result<ClothingPatchV1, String> {
    let setups = table
        .setups
        .into_iter()
        .map(|(id, parts)| {
            Ok(ClothingSetup {
                id,
                parts: parts
                    .into_iter()
                    .map(|p| {
                        Ok(ClothingPart {
                            index: p
                                .index
                                .try_into()
                                .map_err(|_| "Clothing part exceeds 255")?,
                            model: p.model,
                            textures: p
                                .textures
                                .into_iter()
                                .map(|(old, new)| ClothingTexture { old, new })
                                .collect(),
                        })
                    })
                    .collect::<Result<_, String>>()?,
            })
        })
        .collect::<Result<_, String>>()?;
    let mut templates = table.templates;
    let palettes = table
        .template_order
        .into_iter()
        .map(|template| {
            let p = templates
                .remove(&template)
                .ok_or("Clothing template order references an absent key")?;
            Ok(ClothingPalette {
                template,
                icon: p.icon,
                effects: p
                    .palettes
                    .into_iter()
                    .map(|e| ClothingPaletteEffect {
                        source: PaletteSource::PaletteSet(e.palette_set),
                        ranges: e
                            .ranges
                            .into_iter()
                            .map(|(offset, colors)| ClothingRange { offset, colors })
                            .collect(),
                    })
                    .collect(),
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(ClothingPatchV1 {
        schema_version: 1,
        id: table.id,
        setups,
        palettes,
    })
}
pub(crate) fn resolve(
    archive: &mut DatArchive,
    id: u32,
    patch: Option<&ClothingPatchV1>,
) -> Result<Option<ClothingPatchV1>, String> {
    if id == 0 {
        return Ok(None);
    }
    let base = if archive.records().contains_key(&id) {
        Some(from_dat(
            ClothingTable::decode(&archive.read(id).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?,
        )?)
    } else {
        None
    };
    if let Some(patch) = patch.filter(|p| p.id == id) {
        // Validate custom references even in unselected setup/template variants.
        patch
            .validate_assets(|asset| archive.records().contains_key(&asset))
            .map_err(|e| e.to_string())?;
        bace_content::resolve_clothing(base.as_ref(), patch)
            .map(Some)
            .map_err(|e| e.to_string())
    } else {
        base.map(Some).ok_or_else(||format!("ClothingBase {id:08X} is absent from DAT. Open its override in ClothingBases first."))
    }
}
