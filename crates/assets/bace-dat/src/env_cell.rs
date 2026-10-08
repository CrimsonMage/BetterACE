//! Pinned ACE.DatLoader EnvCell/CellPortal/Stab layouts (AGPL-3.0-only).
use crate::{DatError, DatTableLimits, ModelFrame, table_reader::TableReader};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellPortal {
    pub flags: u16,
    pub polygon_id: u16,
    pub other_cell_id: u16,
    pub other_portal_id: u16,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StaticObject {
    pub id: u32,
    pub frame: ModelFrame,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EnvCell {
    pub id: u32,
    pub flags: u32,
    /// Upstream skips this repeated cell word; retained without extra semantics.
    pub repeated_id: u32,
    pub surfaces: Vec<u32>,
    pub environment_id: u32,
    pub cell_structure: u16,
    pub position: ModelFrame,
    pub portals: Vec<CellPortal>,
    pub visible_cells: Vec<u16>,
    pub static_objects: Vec<StaticObject>,
    pub restriction_object: Option<u32>,
}
pub(crate) fn vec3(r: &mut TableReader<'_>) -> Result<[f32; 3], DatError> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}
pub(crate) fn frame(r: &mut TableReader<'_>) -> Result<ModelFrame, DatError> {
    Ok(ModelFrame {
        origin: vec3(r)?,
        rotation: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
    })
}
pub(crate) fn fixed_count(
    r: &mut TableReader<'_>,
    count: usize,
    minimum: usize,
) -> Result<(), DatError> {
    r.count(count, minimum)?;
    r.reserve_entries(count)
}
impl EnvCell {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated EnvCell"))?
                .try_into()
                .map_err(|_| DatError::Format("EnvCell ID"))?,
        );
        if !(0x0100..=0xfffd).contains(&(id & 0xffff)) {
            return Err(DatError::Format("not an EnvCell record ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let flags = r.u32()?;
        let repeated_id = r.u32()?;
        let num_surfaces = usize::from(r.u8()?);
        let num_portals = usize::from(r.u8()?);
        let num_visible = usize::from(r.u16()?);
        fixed_count(&mut r, num_surfaces, 2)?;
        let surfaces = (0..num_surfaces)
            .map(|_| Ok(0x08000000 | u32::from(r.u16()?)))
            .collect::<Result<Vec<_>, DatError>>()?;
        let environment_id = 0x0d000000 | u32::from(r.u16()?);
        let cell_structure = r.u16()?;
        let position = frame(&mut r)?;
        fixed_count(&mut r, num_portals, 8)?;
        let portals = (0..num_portals)
            .map(|_| {
                Ok(CellPortal {
                    flags: r.u16()?,
                    polygon_id: r.u16()?,
                    other_cell_id: r.u16()?,
                    other_portal_id: r.u16()?,
                })
            })
            .collect::<Result<Vec<_>, DatError>>()?;
        fixed_count(&mut r, num_visible, 2)?;
        let visible_cells = (0..num_visible)
            .map(|_| r.u16())
            .collect::<Result<Vec<_>, _>>()?;
        let mut static_objects = Vec::new();
        if flags & 2 != 0 {
            let count = r.u32()? as usize;
            fixed_count(&mut r, count, 32)?;
            for _ in 0..count {
                static_objects.push(StaticObject {
                    id: r.u32()?,
                    frame: frame(&mut r)?,
                });
            }
        }
        let restriction_object = if flags & 8 != 0 { Some(r.u32()?) } else { None };
        r.finish()?;
        Ok(Self {
            id,
            flags,
            repeated_id,
            surfaces,
            environment_id,
            cell_structure,
            position,
            portals,
            visible_cells,
            static_objects,
            restriction_object,
        })
    }
}
