//! Static model inspection layouts from pinned ACE.DatLoader. These are not collision assets.
use crate::{DatError, DatTableLimits, table_reader::TableReader};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct ModelFrame {
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct ModelSetup {
    pub id: u32,
    pub flags: u32,
    pub parts: Vec<u32>,
    pub parents: Vec<u32>,
    pub scales: Vec<[f32; 3]>,
    pub placements: BTreeMap<u32, Vec<ModelFrame>>,
    pub default_animation: u32,
    pub default_motion: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uvs: Vec<[f32; 2]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPolygon {
    pub vertices: Vec<u16>,
    pub positive_uvs: Vec<u8>,
    pub negative_uvs: Vec<u8>,
    pub positive_surface: i16,
    pub negative_surface: i16,
    pub cull: u32,
    pub stippling: u8,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicsObject {
    pub id: u32,
    pub surfaces: Vec<u32>,
    pub vertices: BTreeMap<u16, ModelVertex>,
    pub polygons: BTreeMap<u16, ModelPolygon>,
}

pub(crate) fn reader(bytes: &[u8], prefix: u32) -> Result<(u32, TableReader<'_>), DatError> {
    let id = u32::from_le_bytes(
        bytes
            .get(..4)
            .ok_or(DatError::Format("truncated asset ID"))?
            .try_into()
            .map_err(|_| DatError::Format("asset ID"))?,
    );
    if id >> 24 != prefix {
        return Err(DatError::Format("wrong asset type"));
    }
    Ok((id, TableReader::new(bytes, id, DatTableLimits::default())?))
}
pub(crate) fn count(r: &mut TableReader<'_>, size: usize) -> Result<usize, DatError> {
    let n = r.u32()? as usize;
    r.count(n, size)?;
    r.reserve_entries(n)?;
    Ok(n)
}
fn vec3(r: &mut TableReader<'_>) -> Result<[f32; 3], DatError> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}
fn frame(r: &mut TableReader<'_>) -> Result<ModelFrame, DatError> {
    Ok(ModelFrame {
        origin: vec3(r)?,
        rotation: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
    })
}
impl ModelSetup {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 2)?;
        let flags = r.u32()?;
        if flags & !15 != 0 {
            return Err(DatError::Format("unknown Setup flags"));
        }
        let n = count(&mut r, 4)?;
        if n > 256 {
            return Err(DatError::Format("Setup part limit"));
        }
        let parts = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
        let parents = if flags & 1 != 0 {
            (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let scales = if flags & 2 != 0 {
            (0..n)
                .map(|_| vec3(&mut r))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![[1.0; 3]; n]
        };
        for _ in 0..2 {
            for _ in 0..count(&mut r, 36)? {
                r.u32()?;
                r.u32()?;
                frame(&mut r)?;
            }
        }
        let mut placements = BTreeMap::new();
        for _ in 0..count(&mut r, 8)? {
            let key = r.u32()?;
            r.count(n, 28)?;
            r.reserve_entries(n)?;
            let frames = (0..n)
                .map(|_| frame(&mut r))
                .collect::<Result<Vec<_>, _>>()?;
            for _ in 0..count(&mut r, 8)? {
                skip_hook(&mut r)?;
            }
            if placements.insert(key, frames).is_some() {
                return Err(DatError::Format("duplicate placement"));
            }
        }
        for _ in 0..count(&mut r, 20)? {
            for _ in 0..5 {
                r.f32()?;
            }
        }
        for _ in 0..count(&mut r, 16)? {
            for _ in 0..4 {
                r.f32()?;
            }
        }
        for _ in 0..12 {
            r.f32()?;
        }
        for _ in 0..count(&mut r, 48)? {
            r.u32()?;
            frame(&mut r)?;
            r.u32()?;
            for _ in 0..3 {
                r.f32()?;
            }
        }
        let default_animation = r.u32()?;
        r.u32()?;
        let default_motion = r.u32()?;
        r.u32()?;
        r.u32()?;
        r.finish()?;
        Ok(Self {
            id,
            flags,
            parts,
            parents,
            scales,
            placements,
            default_animation,
            default_motion,
        })
    }
}
fn skip_hook(r: &mut TableReader<'_>) -> Result<(), DatError> {
    let kind = r.u32()?;
    r.u32()?;
    let bytes = match kind {
        0 | 4 | 17 => 0,
        1 | 2 | 6 | 14 | 15 | 16 | 18 | 25 => 4,
        5 => {
            r.u16()?;
            r.known_id(0x01000000)?;
            return Ok(());
        }
        7 | 9 | 11 | 21 => 16,
        8 | 10 | 20 | 22 | 24 => 12,
        12 | 19 | 23 => 8,
        13 | 26 => 40,
        3 => 28,
        _ => return Err(DatError::Format("unsupported Setup animation hook")),
    };
    r.take(bytes)?;
    Ok(())
}
impl GraphicsObject {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        let (id, mut r) = reader(bytes, 1)?;
        let flags = r.u32()?;
        if flags & !15 != 0 {
            return Err(DatError::Format("unknown GfxObj flags"));
        }
        let surfaces = r.array(4, |r| r.u32())?;
        if r.u32()? != 1 {
            return Err(DatError::Format("unsupported vertex type"));
        }
        let mut vertices = BTreeMap::new();
        for _ in 0..count(&mut r, 28)? {
            let key = r.u16()?;
            let n = r.u16()? as usize;
            let position = vec3(&mut r)?;
            let normal = vec3(&mut r)?;
            r.count(n, 8)?;
            r.reserve_entries(n)?;
            let uvs = (0..n)
                .map(|_| Ok([r.f32()?, r.f32()?]))
                .collect::<Result<Vec<_>, DatError>>()?;
            if vertices
                .insert(
                    key,
                    ModelVertex {
                        position,
                        normal,
                        uvs,
                    },
                )
                .is_some()
            {
                return Err(DatError::Format("duplicate vertex"));
            }
        }
        if flags & 1 != 0 {
            polygons(&mut r)?;
            skip_bsp(&mut r, false, 0)?;
        }
        vec3(&mut r)?;
        let polygons = if flags & 2 != 0 {
            let p = polygons(&mut r)?;
            skip_bsp(&mut r, true, 0)?;
            p
        } else {
            BTreeMap::new()
        };
        if flags & 8 != 0 {
            r.u32()?;
        }
        r.finish()?;
        for polygon in polygons.values() {
            for (i, key) in polygon.vertices.iter().enumerate() {
                let v = vertices
                    .get(key)
                    .ok_or(DatError::Format("polygon references missing vertex"))?;
                for uvs in [&polygon.positive_uvs, &polygon.negative_uvs] {
                    if let Some(&uv) = uvs.get(i)
                        && usize::from(uv) >= v.uvs.len()
                    {
                        return Err(DatError::Format("polygon UV index out of bounds"));
                    }
                }
            }
            for s in [polygon.positive_surface, polygon.negative_surface] {
                if s >= 0 && s as usize >= surfaces.len() {
                    return Err(DatError::Format("polygon surface index out of bounds"));
                }
            }
        }
        Ok(Self {
            id,
            surfaces,
            vertices,
            polygons,
        })
    }
}
fn polygons(r: &mut TableReader<'_>) -> Result<BTreeMap<u16, ModelPolygon>, DatError> {
    let mut out = BTreeMap::new();
    for _ in 0..r.smart_count(12)? {
        let key = r.u16()?;
        let n = usize::from(r.u8()?);
        let stippling = r.u8()?;
        if n < 3 {
            return Err(DatError::Format("polygon has fewer than three points"));
        }
        let cull = r.u32()?;
        if cull > 3 {
            return Err(DatError::Format("unknown polygon culling mode"));
        }
        let positive_surface = r.u16()? as i16;
        let mut negative_surface = r.u16()? as i16;
        r.count(n, 2)?;
        r.reserve_entries(n)?;
        let vertices = (0..n).map(|_| r.u16()).collect::<Result<Vec<_>, _>>()?;
        let positive_uvs = if stippling & 4 == 0 {
            r.take(n)?.to_vec()
        } else {
            Vec::new()
        };
        let mut negative_uvs = if cull == 2 && stippling & 8 == 0 {
            r.take(n)?.to_vec()
        } else {
            Vec::new()
        };
        if cull == 1 {
            negative_surface = positive_surface;
            negative_uvs = positive_uvs.clone();
        }
        if out
            .insert(
                key,
                ModelPolygon {
                    vertices,
                    positive_uvs,
                    negative_uvs,
                    positive_surface,
                    negative_surface,
                    cull,
                    stippling,
                },
            )
            .is_some()
        {
            return Err(DatError::Format("duplicate polygon"));
        }
    }
    Ok(out)
}
fn skip_bsp(r: &mut TableReader<'_>, drawing: bool, depth: usize) -> Result<(), DatError> {
    if depth > 128 {
        return Err(DatError::Format("BSP nesting limit"));
    }
    r.reserve_entries(1)?;
    let kind = r.u32()?;
    if kind == 0x4c454146 {
        r.u32()?;
        if !drawing {
            r.u32()?;
            for _ in 0..4 {
                r.f32()?;
            }
            let n = count(r, 2)?;
            r.take(n * 2)?;
        }
        return Ok(());
    }
    let children = match kind {
        0x42506e6e | 0x4250496e | 0x4270494e | 0x42706e4e => 1,
        0x4250494e | 0x42506e4e | 0x504f5254 => 2,
        0x4270496e | 0x42706e6e | 0x42504f4c | 0x4250464c => 0,
        _ => return Err(DatError::Format("unknown BSP node")),
    };
    for _ in 0..4 {
        r.f32()?;
    }
    for _ in 0..children {
        skip_bsp(r, drawing, depth + 1)?;
    }
    if kind == 0x504f5254 && !drawing {
        return Ok(());
    }
    for _ in 0..4 {
        r.f32()?;
    }
    if drawing {
        let n = count(r, 2)?;
        let portals = if kind == 0x504f5254 { count(r, 4)? } else { 0 };
        r.take(n * 2)?;
        r.take(portals * 4)?;
    }
    Ok(())
}
