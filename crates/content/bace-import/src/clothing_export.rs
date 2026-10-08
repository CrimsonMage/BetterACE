//! Serialize ordered templates without routing through a sorted JSON object.
use bace_content::{ClothingPalette, ClothingPatchV1, ClothingSetup, PaletteSource};
use serde::{Serialize, Serializer, ser::SerializeMap};
pub fn export_clothing_json(patch: &ClothingPatchV1) -> Result<String, String> {
    patch.validate().map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&Export(patch)).map_err(|e| e.to_string())?;
    if text.len() > crate::clothing_import::MAX_CLOTHING_BYTES {
        return Err("ClothingBase exceeds 1 MiB".into());
    }
    Ok(text)
}
fn hex(n: u32) -> String {
    format!("0x{n:08X}")
}
struct Export<'a>(&'a ClothingPatchV1);
impl Serialize for Export<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(3))?;
        map.serialize_entry("Id", &hex(self.0.id))?;
        map.serialize_entry("ClothingBaseEffects", &Setups(&self.0.setups))?;
        map.serialize_entry("ClothingSubPalEffects", &Palettes(&self.0.palettes))?;
        map.end()
    }
}
struct Setups<'a>(&'a [ClothingSetup]);
impl Serialize for Setups<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for setup in self.0 {
            let parts:Vec<_>=setup.parts.iter().map(|p|serde_json::json!({"Index":p.index,"ModelId":hex(p.model),"CloTextureEffects":p.textures.iter().map(|t|serde_json::json!({"OldTexture":hex(t.old),"NewTexture":hex(t.new)})).collect::<Vec<_>>()})).collect();
            map.serialize_entry(
                &hex(setup.id),
                &serde_json::json!({"CloObjectEffects":parts}),
            )?;
        }
        map.end()
    }
}
struct Palettes<'a>(&'a [ClothingPalette]);
impl Serialize for Palettes<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for palette in self.0 {
            let effects:Vec<_>=palette.effects.iter().map(|e|{let id=match e.source{PaletteSource::Palette(id)|PaletteSource::PaletteSet(id)=>id};serde_json::json!({"PaletteSet":hex(id),"Ranges":e.ranges.iter().map(|r|serde_json::json!({"Offset":r.offset,"NumColors":r.colors})).collect::<Vec<_>>()})}).collect();
            map.serialize_entry(
                &palette.template.to_string(),
                &serde_json::json!({"Icon":hex(palette.icon),"CloSubPalettes":effects}),
            )?;
        }
        map.end()
    }
}
