//! Native clothing-table behavior; ordered vectors preserve the first palette fallback.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingPatchV1 {
    pub schema_version: u16,
    pub id: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub setups: Vec<ClothingSetup>,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub palettes: Vec<ClothingPalette>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingSetup {
    pub id: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub parts: Vec<ClothingPart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingPart {
    pub index: u8,
    pub model: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub textures: Vec<ClothingTexture>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingTexture {
    pub old: u32,
    pub new: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingPalette {
    pub template: u32,
    #[serde(default)]
    pub icon: u32,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub effects: Vec<ClothingPaletteEffect>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingPaletteEffect {
    pub source: PaletteSource,
    #[serde(default, deserialize_with = "crate::bounded::vec")]
    pub ranges: Vec<ClothingRange>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClothingRange {
    pub offset: u32,
    pub colors: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteSource {
    Palette(u32),
    PaletteSet(u32),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClothingError {
    #[error("invalid clothing schema or asset identifier")]
    Identity,
    #[error("duplicate clothing setup/palette key")]
    Duplicate,
    #[error("clothing effect exceeds bounded or wire-representable limits")]
    Limit,
    #[error("referenced client asset {0:08x} is unavailable")]
    MissingAsset(u32),
    #[error("invalid shade or empty palette set")]
    Shade,
}

fn asset(id: u32, prefix: u32) -> bool {
    id >> 24 == prefix && id & 0x00ff_ffff != 0
}

impl ClothingPatchV1 {
    pub fn validate(&self) -> Result<(), ClothingError> {
        if self.schema_version != 1 || !asset(self.id, 0x10) {
            return Err(ClothingError::Identity);
        }
        if self.setups.len() > 4096 || self.palettes.len() > 4096 {
            return Err(ClothingError::Limit);
        }
        let mut setups = BTreeSet::new();
        let mut palettes = BTreeSet::new();
        for setup in &self.setups {
            if !setups.insert(setup.id) {
                return Err(ClothingError::Duplicate);
            }
            if !asset(setup.id, 2) {
                return Err(ClothingError::Identity);
            }
            if setup.parts.len() > 255
                || setup.parts.iter().map(|p| p.textures.len()).sum::<usize>() > 255
            {
                return Err(ClothingError::Limit);
            }
            for part in &setup.parts {
                if !asset(part.model, 1)
                    || part
                        .textures
                        .iter()
                        .any(|t| !asset(t.old, 5) || !asset(t.new, 5))
                {
                    return Err(ClothingError::Identity);
                }
            }
        }
        for palette in &self.palettes {
            if !palettes.insert(palette.template) {
                return Err(ClothingError::Duplicate);
            }
            if palette.icon != 0 && !asset(palette.icon, 6) {
                return Err(ClothingError::Identity);
            }
            if palette.effects.len() > 255
                || palette
                    .effects
                    .iter()
                    .map(|p| p.ranges.len())
                    .sum::<usize>()
                    > 255
            {
                return Err(ClothingError::Limit);
            }
            for effect in &palette.effects {
                match effect.source {
                    PaletteSource::Palette(id) if asset(id, 4) => {}
                    PaletteSource::PaletteSet(id) if asset(id, 15) => {}
                    _ => return Err(ClothingError::Identity),
                }
                if effect.ranges.iter().any(|r| {
                    r.offset % 8 != 0
                        || r.colors % 8 != 0
                        || r.offset / 8 > 255
                        || (r.colors != 2048 && r.colors / 8 > 255)
                }) {
                    return Err(ClothingError::Limit);
                }
            }
        }
        Ok(())
    }

    /// Check declared client assets before publication; pure and supplied by a validated index.
    pub fn validate_assets(
        &self,
        mut available: impl FnMut(u32) -> bool,
    ) -> Result<(), ClothingError> {
        self.validate()?;
        let mut check = |id| {
            if available(id) {
                Ok(())
            } else {
                Err(ClothingError::MissingAsset(id))
            }
        };
        for setup in &self.setups {
            check(setup.id)?;
            for part in &setup.parts {
                check(part.model)?;
                for texture in &part.textures {
                    check(texture.old)?;
                    check(texture.new)?;
                }
            }
        }
        for palette in &self.palettes {
            if palette.icon != 0 {
                check(palette.icon)?;
            }
            for effect in &palette.effects {
                check(match effect.source {
                    PaletteSource::Palette(id) | PaletteSource::PaletteSet(id) => id,
                })?;
            }
        }
        Ok(())
    }

    pub fn palette(&self, template: Option<u32>) -> Option<&ClothingPalette> {
        template
            .and_then(|id| self.palettes.iter().find(|p| p.template == id))
            .or_else(|| self.palettes.first())
    }
}

/// Always supply the ORIGINAL pinned DAT table. Previously resolved overlays are not a base.
pub fn resolve_clothing(
    base: Option<&ClothingPatchV1>,
    patch: &ClothingPatchV1,
) -> Result<ClothingPatchV1, ClothingError> {
    patch.validate()?;
    let mut result = match base {
        Some(base) if base.id == patch.id => {
            base.validate()?;
            base.clone()
        }
        Some(_) => return Err(ClothingError::Identity),
        None => ClothingPatchV1 {
            schema_version: 1,
            id: patch.id,
            setups: Vec::new(),
            palettes: Vec::new(),
        },
    };
    for setup in &patch.setups {
        if let Some(old) = result.setups.iter_mut().find(|old| old.id == setup.id) {
            *old = setup.clone();
        } else {
            result.setups.push(setup.clone());
        }
    }
    for palette in &patch.palettes {
        if let Some(old) = result
            .palettes
            .iter_mut()
            .find(|old| old.template == palette.template)
        {
            *old = palette.clone();
        } else {
            result.palettes.push(palette.clone());
        }
    }
    result.validate()?;
    Ok(result)
}

/// Pinned ACE PaletteSet.GetPaletteID formula; no client-supplied unchecked float indexing.
pub fn select_palette(
    source: PaletteSource,
    shade: f64,
    set: &[u32],
) -> Result<u32, ClothingError> {
    if !shade.is_finite() || !(0.0..=1.0).contains(&shade) {
        return Err(ClothingError::Shade);
    }
    match source {
        PaletteSource::Palette(id) if asset(id, 4) => Ok(id),
        PaletteSource::PaletteSet(id) if asset(id, 15) && !set.is_empty() => {
            let index = ((set.len() as f64 - 0.000001) * shade) as usize;
            let id = set[index.min(set.len() - 1)];
            if asset(id, 4) {
                Ok(id)
            } else {
                Err(ClothingError::Identity)
            }
        }
        _ => Err(ClothingError::Shade),
    }
}
