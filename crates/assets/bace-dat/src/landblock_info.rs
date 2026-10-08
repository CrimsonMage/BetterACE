//! Complete pinned ACE LandblockInfo/BuildInfo/CBldPortal layouts (47edade3).
//! Copyright ACE contributors; AGPL-3.0-only. Alignment is record-relative.
use crate::{
    DatError, DatTableLimits, ModelFrame, StaticObject,
    env_cell::{fixed_count, frame},
    table_reader::TableReader,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildingPortal {
    pub flags: u16,
    pub other_cell: u16,
    pub other_portal: u16,
    pub visible_cells: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingInfo {
    pub model: u32,
    pub frame: ModelFrame,
    pub leaves: u32,
    pub portals: Vec<BuildingPortal>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LandblockInfo {
    pub id: u32,
    pub cells: u32,
    pub objects: Vec<StaticObject>,
    pub pack_mask: u16,
    pub buildings: Vec<BuildingInfo>,
    pub restriction_bucket_size: Option<u16>,
    pub restrictions: BTreeMap<u32, u32>,
}
impl LandblockInfo {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated landblock info"))?
                .try_into()
                .map_err(|_| DatError::Format("landblock info ID"))?,
        );
        if id & 0xffff != 0xfffe {
            return Err(DatError::Format("wrong landblock info ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let cells = r.u32()?;
        if cells > 0xfefe {
            return Err(DatError::Format("landblock cell count"));
        }
        let count = r.u32()? as usize;
        fixed_count(&mut r, count, 32)?;
        let mut objects = Vec::with_capacity(count);
        for _ in 0..count {
            objects.push(StaticObject {
                id: r.u32()?,
                frame: frame(&mut r)?,
            });
        }
        let count = usize::from(r.u16()?);
        let pack_mask = r.u16()?;
        fixed_count(&mut r, count, 40)?;
        let mut buildings = Vec::with_capacity(count);
        for _ in 0..count {
            let model = r.u32()?;
            let frame = frame(&mut r)?;
            let leaves = r.u32()?;
            let count = r.u32()? as usize;
            fixed_count(&mut r, count, 8)?;
            let mut portals = Vec::with_capacity(count);
            for _ in 0..count {
                let flags = r.u16()?;
                let other_cell = r.u16()?;
                let other_portal = r.u16()?;
                let count = usize::from(r.u16()?);
                fixed_count(&mut r, count, 2)?;
                let visible_cells = (0..count).map(|_| r.u16()).collect::<Result<Vec<_>, _>>()?;
                r.align()?;
                portals.push(BuildingPortal {
                    flags,
                    other_cell,
                    other_portal,
                    visible_cells,
                });
            }
            buildings.push(BuildingInfo {
                model,
                frame,
                leaves,
                portals,
            });
        }
        let mut restrictions = BTreeMap::new();
        let restriction_bucket_size = if pack_mask & 1 != 0 {
            let count = usize::from(r.u16()?);
            let bucket = r.u16()?;
            fixed_count(&mut r, count, 8)?;
            for _ in 0..count {
                let key = r.u32()?;
                if restrictions.insert(key, r.u32()?).is_some() {
                    return Err(DatError::Format("duplicate restriction key"));
                }
            }
            Some(bucket)
        } else {
            None
        };
        r.finish()?;
        Ok(Self {
            id,
            cells,
            objects,
            pack_mask,
            buildings,
            restriction_bucket_size,
            restrictions,
        })
    }
}
