//! Palette and ClothingBase lookup layouts from pinned ACE.DatLoader.
use crate::{
    DatError,
    model_assets::{count, reader},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatPalette {
    pub id: u32,
    pub colors: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatPaletteSet {
    pub id: u32,
    pub palettes: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClothingPart {
    pub index: u32,
    pub model: u32,
    pub textures: Vec<(u32, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClothingPalette {
    pub palette_set: u32,
    pub ranges: Vec<(u32, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClothingTemplate {
    pub icon: u32,
    pub palettes: Vec<ClothingPalette>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClothingTable {
    pub id: u32,
    pub setups: BTreeMap<u32, Vec<ClothingPart>>,
    pub templates: BTreeMap<u32, ClothingTemplate>,
    /// Authored hash-table order: the first template is the ACE fallback.
    pub template_order: Vec<u32>,
}
impl DatPalette {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 4)?;
        let colors = (0..count(&mut r, 4)?)
            .map(|_| r.u32())
            .collect::<Result<_, _>>()?;
        r.finish()?;
        Ok(Self { id, colors })
    }
}
impl DatPaletteSet {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 15)?;
        let palettes = (0..count(&mut r, 4)?)
            .map(|_| r.u32())
            .collect::<Result<_, _>>()?;
        r.finish()?;
        Ok(Self { id, palettes })
    }
    pub fn at_shade(&self, shade: f64) -> Option<u32> {
        if !shade.is_finite() || !(0.0..=1.0).contains(&shade) || self.palettes.is_empty() {
            return None;
        }
        self.palettes
            .get(((self.palettes.len() as f64 - 0.000001) * shade) as usize)
            .copied()
    }
}
impl ClothingTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 16)?;
        let n = usize::from(r.u16()?);
        r.u16()?;
        r.count(n, 8)?;
        r.reserve_entries(n)?;
        let mut setups = BTreeMap::new();
        for _ in 0..n {
            let setup = r.u32()?;
            let mut parts = Vec::new();
            for _ in 0..count(&mut r, 12)? {
                let index = r.u32()?;
                let model = r.u32()?;
                let mut textures = Vec::new();
                for _ in 0..count(&mut r, 8)? {
                    textures.push((r.u32()?, r.u32()?));
                }
                parts.push(ClothingPart {
                    index,
                    model,
                    textures,
                });
            }
            if setups.insert(setup, parts).is_some() {
                return Err(DatError::Format("duplicate clothing setup"));
            }
        }
        let n = usize::from(r.u16()?);
        r.u16()?;
        r.count(n, 12)?;
        r.reserve_entries(n)?;
        let mut templates = BTreeMap::new();
        let mut template_order = Vec::with_capacity(n);
        for _ in 0..n {
            let key = r.u32()?;
            template_order.push(key);
            let icon = r.u32()?;
            let mut palettes = Vec::new();
            for _ in 0..count(&mut r, 8)? {
                let mut ranges = Vec::new();
                for _ in 0..count(&mut r, 8)? {
                    ranges.push((r.u32()?, r.u32()?));
                }
                palettes.push(ClothingPalette {
                    palette_set: r.u32()?,
                    ranges,
                });
            }
            if templates
                .insert(key, ClothingTemplate { icon, palettes })
                .is_some()
            {
                return Err(DatError::Format("duplicate clothing template"));
            }
        }
        r.finish()?;
        Ok(Self {
            id,
            setups,
            templates,
            template_order,
        })
    }
}
