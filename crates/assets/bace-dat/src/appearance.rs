//! Checked character-generator appearance records from pinned ACE.DatLoader.
use crate::{DatError, table_reader::TableReader};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectDescription {
    /// ACE skips this marker; preserve it without treating it as a schema version.
    pub marker: u8,
    pub palette_id: Option<u32>,
    pub sub_palettes: Vec<SubPalette>,
    pub texture_changes: Vec<TextureChange>,
    pub animation_parts: Vec<AnimationPart>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubPalette {
    pub id: u32,
    pub offset: u32,
    pub colors: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureChange {
    pub part: u8,
    pub old_texture: u32,
    pub new_texture: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationPart {
    pub part: u8,
    pub id: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HairStyle {
    pub icon: u32,
    pub bald: bool,
    pub alternate_setup: u32,
    pub appearance: ObjectDescription,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EyeStrip {
    pub icon: u32,
    pub bald_icon: u32,
    pub appearance: ObjectDescription,
    pub bald_appearance: ObjectDescription,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceStrip {
    pub icon: u32,
    pub appearance: ObjectDescription,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationGear {
    pub name: String,
    pub clothing_table: u32,
    pub weenie: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationGender {
    pub name: String,
    pub scale: u32,
    pub setup: u32,
    pub sound_table: u32,
    pub icon: u32,
    pub base_palette: u32,
    pub skin_palette_set: u32,
    pub physics_table: u32,
    pub motion_table: u32,
    pub combat_table: u32,
    pub appearance: ObjectDescription,
    pub hair_colors: Vec<u32>,
    pub hair_styles: Vec<HairStyle>,
    pub eye_colors: Vec<u32>,
    pub eyes: Vec<EyeStrip>,
    pub noses: Vec<FaceStrip>,
    pub mouths: Vec<FaceStrip>,
    pub headgear: Vec<CreationGear>,
    pub shirts: Vec<CreationGear>,
    pub pants: Vec<CreationGear>,
    pub footwear: Vec<CreationGear>,
    pub clothing_colors: Vec<u32>,
}
impl ObjectDescription {
    pub(crate) fn read(reader: &mut TableReader<'_>) -> Result<Self, DatError> {
        reader.align()?;
        let marker = reader.u8()?;
        let palettes = usize::from(reader.u8()?);
        let textures = usize::from(reader.u8()?);
        let parts = usize::from(reader.u8()?);
        reader.reserve_entries(palettes + textures + parts)?;
        let palette_id = if palettes == 0 {
            None
        } else {
            Some(reader.known_id(0x04000000)?)
        };
        let mut sub_palettes = Vec::new();
        for _ in 0..palettes {
            let id = reader.known_id(0x04000000)?;
            let offset = u32::from(reader.u8()?) * 8;
            let count = u32::from(reader.u8()?);
            sub_palettes.push(SubPalette {
                id,
                offset,
                colors: if count == 0 { 2048 } else { count * 8 },
            });
        }
        let mut texture_changes = Vec::new();
        for _ in 0..textures {
            texture_changes.push(TextureChange {
                part: reader.u8()?,
                old_texture: reader.known_id(0x05000000)?,
                new_texture: reader.known_id(0x05000000)?,
            });
        }
        let mut animation_parts = Vec::new();
        for _ in 0..parts {
            animation_parts.push(AnimationPart {
                part: reader.u8()?,
                id: reader.known_id(0x01000000)?,
            });
        }
        reader.align()?;
        Ok(Self {
            marker,
            palette_id,
            sub_palettes,
            texture_changes,
            animation_parts,
        })
    }
}
impl CreationGender {
    pub(crate) fn read(reader: &mut TableReader<'_>) -> Result<Self, DatError> {
        Ok(Self {
            name: reader.dotnet_string()?,
            scale: reader.u32()?,
            setup: reader.u32()?,
            sound_table: reader.u32()?,
            icon: reader.u32()?,
            base_palette: reader.u32()?,
            skin_palette_set: reader.u32()?,
            physics_table: reader.u32()?,
            motion_table: reader.u32()?,
            combat_table: reader.u32()?,
            appearance: ObjectDescription::read(reader)?,
            hair_colors: reader.array(4, |r| r.u32())?,
            hair_styles: reader.array(13, |r| {
                Ok(HairStyle {
                    icon: r.u32()?,
                    bald: r.u8()? == 1,
                    alternate_setup: r.u32()?,
                    appearance: ObjectDescription::read(r)?,
                })
            })?,
            eye_colors: reader.array(4, |r| r.u32())?,
            eyes: reader.array(16, |r| {
                Ok(EyeStrip {
                    icon: r.u32()?,
                    bald_icon: r.u32()?,
                    appearance: ObjectDescription::read(r)?,
                    bald_appearance: ObjectDescription::read(r)?,
                })
            })?,
            noses: reader.array(8, face)?,
            mouths: reader.array(8, face)?,
            headgear: reader.array(9, gear)?,
            shirts: reader.array(9, gear)?,
            pants: reader.array(9, gear)?,
            footwear: reader.array(9, gear)?,
            clothing_colors: reader.array(4, |r| r.u32())?,
        })
    }
}
fn face(reader: &mut TableReader<'_>) -> Result<FaceStrip, DatError> {
    Ok(FaceStrip {
        icon: reader.u32()?,
        appearance: ObjectDescription::read(reader)?,
    })
}
fn gear(reader: &mut TableReader<'_>) -> Result<CreationGear, DatError> {
    Ok(CreationGear {
        name: reader.dotnet_string()?,
        clothing_table: reader.u32()?,
        weenie: reader.u32()?,
    })
}
