//! Checked texture resource envelopes; pixel decoding belongs to the preview consumer.
use crate::{
    DatError,
    model_assets::{count, reader},
};
#[derive(Clone, Debug, PartialEq)]
pub struct DatSurface {
    pub flags: u32,
    pub texture: u32,
    pub palette: u32,
    pub color: u32,
    pub translucency: f32,
    pub luminosity: f32,
    pub diffuse: f32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatTexture {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub bytes: Vec<u8>,
    pub palette: Option<u32>,
}
impl DatSurface {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        // Unlike the other resources, the surface starts with flags, not its DID.
        if bytes.len() < 4 {
            return Err(DatError::Format("truncated surface"));
        }
        let flags = u32::from_le_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| DatError::Format("surface flags"))?,
        );
        let mut r = crate::table_reader::TableReader::new(bytes, flags, Default::default())?;
        let (texture, palette, color) = if flags & 6 != 0 {
            (r.u32()?, r.u32()?, 0)
        } else {
            (0, 0, r.u32()?)
        };
        let translucency = r.f32()?;
        let luminosity = r.f32()?;
        let diffuse = r.f32()?;
        r.finish()?;
        Ok(Self {
            flags,
            texture,
            palette,
            color,
            translucency,
            luminosity,
            diffuse,
        })
    }
}
impl DatTexture {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 6)?;
        r.u32()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let format = r.u32()?;
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || u64::from(width) * u64::from(height) > 4_194_304
        {
            return Err(DatError::Format("texture dimension limit"));
        }
        let n = r.u32()? as usize;
        let bytes = r.take(n)?.to_vec();
        let palette = if format == 41 || format == 101 {
            Some(r.u32()?)
        } else {
            None
        };
        r.finish()?;
        Ok(Self {
            id,
            width,
            height,
            format,
            bytes,
            palette,
        })
    }
}
pub fn decode_texture_list(bytes: &[u8]) -> Result<Vec<u32>, DatError> {
    let (_, mut r) = reader(bytes, 5)?;
    r.u32()?;
    r.u8()?;
    let ids = (0..count(&mut r, 4)?)
        .map(|_| r.u32())
        .collect::<Result<_, _>>()?;
    r.finish()?;
    Ok(ids)
}
