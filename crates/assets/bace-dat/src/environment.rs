//! Pinned Environment/CellStruct/CVertexArray/SWVertex/Polygon layouts.
//! AGPL-3.0-only. Keeps all drawing and collision data; no geometry fabrication.
use crate::{
    BspTree, BspTreeKind, DatError, DatTableLimits, ModelPolygon, ModelVertex,
    env_cell::{fixed_count, vec3},
    table_reader::TableReader,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct CellGeometry {
    pub vertices: BTreeMap<u16, ModelVertex>,
    pub polygons: BTreeMap<u16, ModelPolygon>,
    pub portals: Vec<u16>,
    pub cell_bsp: BspTree,
    pub physics_polygons: BTreeMap<u16, ModelPolygon>,
    pub physics_bsp: BspTree,
    pub drawing_bsp: Option<BspTree>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    pub id: u32,
    pub cells: BTreeMap<u32, CellGeometry>,
}
fn polygons(
    r: &mut TableReader<'_>,
    count: usize,
) -> Result<BTreeMap<u16, ModelPolygon>, DatError> {
    fixed_count(r, count, 12)?;
    let mut out = BTreeMap::new();
    for _ in 0..count {
        let key = r.u16()?;
        let n = usize::from(r.u8()?);
        let stippling = r.u8()?;
        let cull = r.u32()?;
        let positive_surface = r.u16()? as i16;
        let mut negative_surface = r.u16()? as i16;
        fixed_count(r, n, 2)?;
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
            return Err(DatError::Format("duplicate environment polygon"));
        }
    }
    Ok(out)
}
fn vertices(r: &mut TableReader<'_>) -> Result<BTreeMap<u16, ModelVertex>, DatError> {
    if r.u32()? != 1 {
        return Err(DatError::Format("unsupported environment vertex type"));
    }
    let n = r.u32()? as usize;
    fixed_count(r, n, 28)?;
    let mut out = BTreeMap::new();
    for _ in 0..n {
        let key = r.u16()?;
        let n = usize::from(r.u16()?);
        let position = vec3(r)?;
        let normal = vec3(r)?;
        fixed_count(r, n, 8)?;
        let uvs = (0..n)
            .map(|_| Ok([r.f32()?, r.f32()?]))
            .collect::<Result<Vec<_>, DatError>>()?;
        if out
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
            return Err(DatError::Format("duplicate environment vertex"));
        }
    }
    Ok(out)
}
fn cell(r: &mut TableReader<'_>) -> Result<CellGeometry, DatError> {
    let num_polygons = r.u32()? as usize;
    let num_physics = r.u32()? as usize;
    let num_portals = r.u32()? as usize;
    let vertices = vertices(r)?;
    let polygons = polygons(r, num_polygons)?;
    fixed_count(r, num_portals, 2)?;
    let portals = (0..num_portals)
        .map(|_| r.u16())
        .collect::<Result<Vec<_>, _>>()?;
    r.align()?;
    let cell_bsp = BspTree::read(r, BspTreeKind::Cell)?;
    let physics_polygons = self::polygons(r, num_physics)?;
    let physics_bsp = BspTree::read(r, BspTreeKind::Physics)?;
    let drawing_bsp = if r.u32()? != 0 {
        Some(BspTree::read(r, BspTreeKind::Drawing)?)
    } else {
        None
    };
    r.align()?;
    for p in polygons.values().chain(physics_polygons.values()) {
        for (i, key) in p.vertices.iter().enumerate() {
            let v = vertices
                .get(key)
                .ok_or(DatError::Format("environment polygon missing vertex"))?;
            for uvs in [&p.positive_uvs, &p.negative_uvs] {
                if let Some(&uv) = uvs.get(i)
                    && usize::from(uv) >= v.uvs.len()
                {
                    return Err(DatError::Format("environment polygon UV bounds"));
                }
            }
        }
    }
    Ok(CellGeometry {
        vertices,
        polygons,
        portals,
        cell_bsp,
        physics_polygons,
        physics_bsp,
        drawing_bsp,
    })
}
impl Environment {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated environment"))?
                .try_into()
                .map_err(|_| DatError::Format("environment ID"))?,
        );
        if id >> 24 != 0x0d {
            return Err(DatError::Format("wrong environment record ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let count = r.u32()? as usize;
        fixed_count(&mut r, count, 24)?;
        let mut cells = BTreeMap::new();
        for _ in 0..count {
            let key = r.u32()?;
            if cells.insert(key, cell(&mut r)?).is_some() {
                return Err(DatError::Format("duplicate environment cell"));
            }
        }
        r.finish()?;
        Ok(Self { id, cells })
    }
}
