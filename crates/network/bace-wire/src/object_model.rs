//! WorldObject.SerializeModelData at the pinned ACE revision. The 0x11 header,
//! byte counts, typed packed IDs and final DWORD padding are protocol fields.
use crate::{Reader, WireError, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelPalette {
    pub palette_id: u32,
    pub offset: u8,
    pub length: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelTexture {
    pub part_index: u8,
    pub old_texture: u32,
    pub new_texture: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelPart {
    pub part_index: u8,
    pub animation_id: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectModel {
    /// Present exactly when subpalettes are present.
    pub palette_id: Option<u32>,
    pub palettes: Vec<ModelPalette>,
    pub textures: Vec<ModelTexture>,
    pub parts: Vec<ModelPart>,
}
impl ObjectModel {
    pub fn encode(&self, max_entries: usize) -> Result<Vec<u8>, WireError> {
        let mut writer = Writer::new();
        self.write(&mut writer, max_entries)?;
        Ok(writer.into_bytes())
    }
    pub fn decode(bytes: &[u8], max_entries: usize) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        let value = Self::read(&mut reader, max_entries)?;
        crate::envelope::finish(&reader)?;
        Ok(value)
    }
    pub(crate) fn write(&self, writer: &mut Writer, max_entries: usize) -> Result<(), WireError> {
        let total = self
            .palettes
            .len()
            .checked_add(self.textures.len())
            .and_then(|n| n.checked_add(self.parts.len()))
            .ok_or(WireError::LimitExceeded)?;
        if total > max_entries
            || [self.palettes.len(), self.textures.len(), self.parts.len()]
                .iter()
                .any(|count| *count > 255)
        {
            return Err(WireError::LimitExceeded);
        }
        if self.palette_id.is_some() == self.palettes.is_empty() {
            return Err(WireError::InvalidEncoding);
        }
        writer.bytes(&[
            0x11,
            self.palettes.len() as u8,
            self.textures.len() as u8,
            self.parts.len() as u8,
        ]);
        if let Some(id) = self.palette_id {
            known_type(writer, id, 0x04000000)?;
        }
        for palette in &self.palettes {
            known_type(writer, palette.palette_id, 0x04000000)?;
            writer.bytes(&[palette.offset, palette.length]);
        }
        for texture in &self.textures {
            writer.bytes(&[texture.part_index]);
            known_type(writer, texture.old_texture, 0x05000000)?;
            known_type(writer, texture.new_texture, 0x05000000)?;
        }
        for part in &self.parts {
            writer.bytes(&[part.part_index]);
            known_type(writer, part.animation_id, 0x01000000)?;
        }
        writer.align4();
        Ok(())
    }
    pub(crate) fn read(reader: &mut Reader<'_>, max_entries: usize) -> Result<Self, WireError> {
        let header = reader.take(4)?;
        if header[0] != 0x11 {
            return Err(WireError::InvalidEncoding);
        }
        let (palettes, textures, parts) = (
            usize::from(header[1]),
            usize::from(header[2]),
            usize::from(header[3]),
        );
        if palettes + textures + parts > max_entries {
            return Err(WireError::LimitExceeded);
        }
        let palette_id = if palettes != 0 {
            Some(read_known_type(reader, 0x04000000)?)
        } else {
            None
        };
        let mut result = Self {
            palette_id,
            ..Self::default()
        };
        // Minimum serialized sizes bound allocations before trusting counts.
        if palettes * 4 + textures * 5 + parts * 3 > reader.remaining() {
            return Err(WireError::Truncated);
        }
        for _ in 0..palettes {
            result.palettes.push(ModelPalette {
                palette_id: read_known_type(reader, 0x04000000)?,
                offset: reader.take(1)?[0],
                length: reader.take(1)?[0],
            });
        }
        for _ in 0..textures {
            result.textures.push(ModelTexture {
                part_index: reader.take(1)?[0],
                old_texture: read_known_type(reader, 0x05000000)?,
                new_texture: read_known_type(reader, 0x05000000)?,
            });
        }
        for _ in 0..parts {
            result.parts.push(ModelPart {
                part_index: reader.take(1)?[0],
                animation_id: read_known_type(reader, 0x01000000)?,
            });
        }
        reader.take((4 - reader.position() % 4) % 4)?;
        Ok(result)
    }
}
/// Preserve ACE's known-type packing for canonical IDs and raw 24-bit IDs;
/// reject a different type prefix rather than reproducing lossy subtraction.
pub(crate) fn known_type(writer: &mut Writer, id: u32, kind: u32) -> Result<(), WireError> {
    let value = if id & 0xff000000 == kind {
        id - kind
    } else if id <= 0x00ffffff {
        id
    } else {
        return Err(WireError::InvalidEncoding);
    };
    writer.packed_u32(value)
}
fn read_known_type(reader: &mut Reader<'_>, kind: u32) -> Result<u32, WireError> {
    let value = reader.packed_u32()?;
    if value > 0x00ffffff {
        return Err(WireError::InvalidEncoding);
    }
    Ok(kind | value)
}
