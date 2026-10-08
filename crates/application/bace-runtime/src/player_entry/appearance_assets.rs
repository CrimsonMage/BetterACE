//! Bounded cold DAT closure for login appearance. The caller owns fingerprint
//! admission and executes these archive reads on its asset preparation worker.
use super::{appearance::EntryAppearanceAssets, object::Properties};
use bace_content::WeenieV1;
use bace_dat::{CharGen, ClothingTable, DatArchive, DatPaletteSet, ModelSetup};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default)]
pub struct PreparedEntryAppearanceAssets {
    pub setups: BTreeMap<u32, ModelSetup>,
    pub clothing: BTreeMap<u32, ClothingTable>,
    pub palettes: BTreeMap<u32, DatPaletteSet>,
}
impl PreparedEntryAppearanceAssets {
    pub fn borrowed<'a>(&'a self, chargen: &'a CharGen) -> EntryAppearanceAssets<'a> {
        EntryAppearanceAssets {
            chargen,
            setups: &self.setups,
            clothing: &self.clothing,
            palettes: &self.palettes,
        }
    }
    pub fn prepare(
        verified_portal: &mut DatArchive,
        sources: &[&WeenieV1],
    ) -> Result<Self, String> {
        if sources.len() > 4096 {
            return Err("entry appearance source capacity".into());
        }
        let mut result = Self::default();
        let mut setups = BTreeSet::new();
        let mut clothing = BTreeSet::new();
        let mut palettes = BTreeSet::new();
        let mut budget = 64 * 1024 * 1024usize;
        for source in sources {
            let p = Properties(source);
            if let Some(id) = p.did("Setup").filter(|id| *id != 0) {
                setups.insert(id);
            }
            if let Some(id) = p.did("ClothingBase") {
                clothing.insert(id);
            }
        }
        for id in setups {
            let bytes = read(verified_portal, id, &mut budget)?;
            let value = ModelSetup::decode(&bytes).map_err(|e| e.to_string())?;
            if value.id != id || value.parts.len() > 255 {
                return Err("entry setup identity/part capacity".into());
            }
            result.setups.insert(id, value);
        }
        for id in clothing {
            let bytes = read(verified_portal, id, &mut budget)?;
            let value = ClothingTable::decode(&bytes).map_err(|e| e.to_string())?;
            if value.id != id {
                return Err("entry clothing identity".into());
            }
            for template in value.templates.values() {
                for palette in &template.palettes {
                    palettes.insert(palette.palette_set);
                }
            }
            if palettes.len() > 4096 {
                return Err("entry palette closure capacity".into());
            }
            result.clothing.insert(id, value);
        }
        for id in palettes {
            let bytes = read(verified_portal, id, &mut budget)?;
            let value = DatPaletteSet::decode(&bytes).map_err(|e| e.to_string())?;
            if value.id != id {
                return Err("entry palette identity".into());
            }
            result.palettes.insert(id, value);
        }
        Ok(result)
    }
}
fn read(portal: &mut DatArchive, id: u32, budget: &mut usize) -> Result<Vec<u8>, String> {
    let bytes = portal.read(id).map_err(|e| e.to_string())?;
    *budget = budget
        .checked_sub(bytes.len())
        .ok_or("entry appearance byte capacity")?;
    Ok(bytes)
}
