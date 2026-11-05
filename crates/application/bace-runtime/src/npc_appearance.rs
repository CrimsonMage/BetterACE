//! Creature.GenerateNewFace at the pinned official ACE source. Its random draw
//! source is deliberately server-owned; all selection bounds and absent-field
//! behavior follow ACE, while a stable instance seed avoids visibility rerolls.
use bace_content::{Property, WeenieV1};
use bace_dat::{CharGen, CreationGender, DatPaletteSet, ObjectDescription};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

fn key(kind: &str, name: &str) -> Option<u32> {
    // Frozen ACE.Entity Property* IDs; face preparation can run before the
    // accepted treasure enum catalog has been installed.
    Some(match (kind, name) {
        ("PropertyInt", "Gender") => 113,
        ("PropertyInt", "HeritageGroup") => 188,
        ("PropertyString", "Sex") => 3,
        ("PropertyString", "HeritageGroup") => 4,
        ("PropertyDataId", "PaletteBase") => 6,
        ("PropertyDataId", "EyesTexture") => 9,
        ("PropertyDataId", "NoseTexture") => 10,
        ("PropertyDataId", "MouthTexture") => 11,
        ("PropertyDataId", "DefaultEyesTexture") => 12,
        ("PropertyDataId", "DefaultNoseTexture") => 13,
        ("PropertyDataId", "DefaultMouthTexture") => 14,
        ("PropertyDataId", "HairPalette") => 15,
        ("PropertyDataId", "EyesPalette") => 16,
        ("PropertyDataId", "SkinPalette") => 17,
        ("PropertyDataId", "HeadObject") => 18,
        _ => return None,
    })
}
fn int(source: &WeenieV1, name: &str) -> Option<i32> {
    let id = key("PropertyInt", name)?;
    source
        .properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}
fn string<'a>(source: &'a WeenieV1, name: &str) -> Option<&'a str> {
    let id = key("PropertyString", name)?;
    source
        .properties
        .strings
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value.as_str())
}
fn set_did(source: &mut WeenieV1, name: &str, value: Option<u32>) {
    let Some(value) = value else { return };
    let Some(id) = key("PropertyDataId", name) else {
        return;
    };
    if source.properties.data_ids.iter().all(|p| p.id != id) {
        source.properties.data_ids.push(Property { id, value });
        source.properties.data_ids.sort_by_key(|p| p.id);
    }
}
fn set_int(source: &mut WeenieV1, name: &str, value: i32) {
    let Some(id) = key("PropertyInt", name) else {
        return;
    };
    if source.properties.ints.iter().all(|p| p.id != id) {
        source.properties.ints.push(Property { id, value });
        source.properties.ints.sort_by_key(|p| p.id);
    }
}
fn heritage(source: &WeenieV1) -> Option<u32> {
    int(source, "HeritageGroup")
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| {
            let name = string(source, "HeritageGroup")?.replace('\'', "");
            [
                "Aluvian",
                "Gharundim",
                "Sho",
                "Viamontian",
                "Shadowbound",
                "Gearknight",
                "Tumerok",
                "Lugian",
                "Empyrean",
                "Penumbraen",
                "Undead",
                "Olthoi",
                "OlthoiAcid",
            ]
            .iter()
            .position(|v| v.eq_ignore_ascii_case(&name))
            .map(|i| i as u32 + 1)
        })
}
fn gender(source: &WeenieV1) -> Option<i32> {
    int(source, "Gender").or_else(|| match string(source, "Sex")? {
        s if s.eq_ignore_ascii_case("Male") => Some(1),
        s if s.eq_ignore_ascii_case("Female") => Some(2),
        _ => None,
    })
}
pub(crate) fn eligible_gender<'a>(
    source: &WeenieV1,
    chargen: &'a CharGen,
) -> Option<&'a CreationGender> {
    // The currently qualified visibility path covers the four human
    // heritages. Other ACE creature races need their own DAT fixtures.
    let group = heritage(source)?;
    if !(1..=4).contains(&group) {
        return None;
    }
    chargen
        .heritage_groups
        .get(&group)?
        .genders
        .get(&gender(source)?)
}
pub(crate) fn palette_sets(source: &WeenieV1, chargen: &CharGen) -> Vec<u32> {
    eligible_gender(source, chargen).map_or_else(Vec::new, |sex| {
        std::iter::once(sex.skin_palette_set)
            .chain(sex.hair_colors.iter().copied())
            .collect()
    })
}
struct Draw {
    entity: u32,
    incarnation: u64,
    ordinal: u64,
}
impl Draw {
    fn next(&mut self) -> u64 {
        let mut hash = Sha256::new();
        hash.update(b"BetterACE.npc.face.v1\0");
        hash.update(self.entity.to_le_bytes());
        hash.update(self.incarnation.to_le_bytes());
        hash.update(self.ordinal.to_le_bytes());
        self.ordinal += 1;
        u64::from_le_bytes(hash.finalize()[..8].try_into().expect("SHA256 prefix"))
    }
    fn index(&mut self, count: usize) -> Option<usize> {
        (count != 0).then(|| (self.next() % count as u64) as usize)
    }
    fn shade(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn texture(description: &ObjectDescription) -> Result<(u32, u32), String> {
    let row = description
        .texture_changes
        .first()
        .ok_or("NPC face texture absent")?;
    Ok((row.old_texture, row.new_texture))
}
pub(crate) fn generate(
    source: &WeenieV1,
    entity: u32,
    incarnation: u64,
    chargen: &CharGen,
    palettes: &BTreeMap<u32, DatPaletteSet>,
) -> Result<Option<WeenieV1>, String> {
    let Some(sex) = eligible_gender(source, chargen) else {
        return Ok(None);
    };
    let mut draw = Draw {
        entity,
        incarnation,
        ordinal: 0,
    };
    let style_count = sex.hair_styles.len().min(9); // ACE default npc_hairstyle_fullrange=false.
    let style = sex
        .hair_styles
        .get(draw.index(style_count).ok_or("NPC hair styles absent")?)
        .ok_or("NPC hair style absent")?;
    let hair_color = *sex
        .hair_colors
        .get(
            draw.index(sex.hair_colors.len())
                .ok_or("NPC hair colors absent")?,
        )
        .ok_or("NPC hair color absent")?;
    let hair_shade = draw.shade();
    let eye_color = *sex
        .eye_colors
        .get(
            draw.index(sex.eye_colors.len())
                .ok_or("NPC eye colors absent")?,
        )
        .ok_or("NPC eye color absent")?;
    let eyes = sex
        .eyes
        .get(draw.index(sex.eyes.len()).ok_or("NPC eyes absent")?)
        .ok_or("NPC eyes absent")?;
    let mouth = sex
        .mouths
        .get(draw.index(sex.mouths.len()).ok_or("NPC mouths absent")?)
        .ok_or("NPC mouth absent")?;
    let nose = sex
        .noses
        .get(draw.index(sex.noses.len()).ok_or("NPC noses absent")?)
        .ok_or("NPC nose absent")?;
    let skin_shade = draw.shade();
    let eye_texture = texture(if style.bald {
        &eyes.bald_appearance
    } else {
        &eyes.appearance
    })?;
    let mouth_texture = texture(&mouth.appearance)?;
    let nose_texture = texture(&nose.appearance)?;
    let skin = palettes
        .get(&sex.skin_palette_set)
        .and_then(|p| p.at_shade(skin_shade))
        .ok_or("NPC skin palette absent")?;
    let hair = palettes
        .get(&hair_color)
        .and_then(|p| p.at_shade(hair_shade))
        .ok_or("NPC hair palette absent")?;
    let mut result = source.clone();
    set_int(
        &mut result,
        "HeritageGroup",
        heritage(source).ok_or("NPC heritage absent")? as i32,
    );
    set_int(
        &mut result,
        "Gender",
        gender(source).ok_or("NPC gender absent")?,
    );
    if let Some(id) = key("PropertyDataId", "PaletteBase") {
        if let Some(row) = result.properties.data_ids.iter_mut().find(|p| p.id == id) {
            row.value = sex.base_palette;
        } else {
            result.properties.data_ids.push(Property {
                id,
                value: sex.base_palette,
            });
            result.properties.data_ids.sort_by_key(|p| p.id);
        }
    }
    set_did(&mut result, "EyesTexture", Some(eye_texture.1));
    set_did(&mut result, "DefaultEyesTexture", Some(eye_texture.0));
    set_did(&mut result, "MouthTexture", Some(mouth_texture.1));
    set_did(&mut result, "DefaultMouthTexture", Some(mouth_texture.0));
    set_did(&mut result, "NoseTexture", Some(nose_texture.1));
    set_did(&mut result, "DefaultNoseTexture", Some(nose_texture.0));
    set_did(
        &mut result,
        "HeadObject",
        (style.appearance.animation_parts.len() == 1)
            .then(|| style.appearance.animation_parts[0].id),
    );
    set_did(&mut result, "SkinPalette", Some(skin));
    set_did(&mut result, "HairPalette", Some(hair));
    set_did(&mut result, "EyesPalette", Some(eye_color));
    Ok(Some(result))
}
