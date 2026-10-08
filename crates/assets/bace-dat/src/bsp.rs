//! Pinned ACE BSPTree/BSPNode/BSPLeaf/BSPPortal layouts (AGPL-3.0-only).
//! Flat ownership avoids recursive-drop stack growth; parse depth is capped.
use crate::{
    DatError, DatTableLimits,
    env_cell::{fixed_count, vec3},
    table_reader::TableReader,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BspTreeKind {
    Drawing,
    Physics,
    Cell,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BspSphere {
    pub origin: [f32; 3],
    pub radius: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BspNode {
    /// Numeric DWORD spelling from the DAT, e.g. 0x4c454146 = LEAF.
    pub tag: u32,
    pub splitting_plane: Option<[f32; 4]>,
    pub positive: Option<usize>,
    pub negative: Option<usize>,
    pub sphere: Option<BspSphere>,
    pub polygons: Vec<u16>,
    pub portal_polygons: Vec<[i16; 2]>,
    pub leaf_index: Option<i32>,
    pub solid: Option<i32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct BspTree {
    pub kind: BspTreeKind,
    pub nodes: Vec<BspNode>,
}
fn sphere(r: &mut TableReader<'_>) -> Result<BspSphere, DatError> {
    Ok(BspSphere {
        origin: vec3(r)?,
        radius: r.f32()?,
    })
}
fn polygons(r: &mut TableReader<'_>, count: usize) -> Result<Vec<u16>, DatError> {
    fixed_count(r, count, 2)?;
    (0..count).map(|_| r.u16()).collect()
}
impl BspTree {
    pub fn decode(bytes: &[u8], kind: BspTreeKind) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, kind, DatTableLimits::default())
    }
    pub fn decode_with_limits(
        bytes: &[u8],
        kind: BspTreeKind,
        limits: DatTableLimits,
    ) -> Result<Self, DatError> {
        let mut r = TableReader::raw(bytes, limits)?;
        let tree = Self::read(&mut r, kind)?;
        r.finish()?;
        Ok(tree)
    }
    pub(crate) fn read(r: &mut TableReader<'_>, kind: BspTreeKind) -> Result<Self, DatError> {
        let mut tree = Self {
            kind,
            nodes: Vec::new(),
        };
        tree.node(r, 0)?;
        Ok(tree)
    }
    fn node(&mut self, r: &mut TableReader<'_>, depth: usize) -> Result<usize, DatError> {
        if depth >= 128 {
            return Err(DatError::Format("BSP depth limit"));
        }
        r.reserve_entries(1)?;
        let tag = r.u32()?;
        let index = self.nodes.len();
        self.nodes.push(BspNode {
            tag,
            splitting_plane: None,
            positive: None,
            negative: None,
            sphere: None,
            polygons: Vec::new(),
            portal_polygons: Vec::new(),
            leaf_index: None,
            solid: None,
        });
        if tag == 0x4c454146 {
            self.nodes[index].leaf_index = Some(r.u32()? as i32);
            if self.kind == BspTreeKind::Physics {
                self.nodes[index].solid = Some(r.u32()? as i32);
                self.nodes[index].sphere = Some(sphere(r)?);
                let count = r.u32()? as usize;
                self.nodes[index].polygons = polygons(r, count)?;
            }
            return Ok(index);
        }
        let (positive, negative) = match tag {
            0x42506e6e | 0x4250496e => (true, false),
            0x4270494e | 0x42706e4e => (false, true),
            0x4250494e | 0x42506e4e | 0x504f5254 => (true, true),
            0x4270496e | 0x42706e6e | 0x42504f4c | 0x4250464c => (false, false),
            _ => return Err(DatError::Format("unknown BSP node tag")),
        };
        self.nodes[index].splitting_plane = Some([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
        if positive {
            self.nodes[index].positive = Some(self.node(r, depth + 1)?);
        }
        if negative {
            self.nodes[index].negative = Some(self.node(r, depth + 1)?);
        }
        if tag == 0x504f5254 {
            if self.kind == BspTreeKind::Drawing {
                self.nodes[index].sphere = Some(sphere(r)?);
                let count = r.u32()? as usize;
                let portal_count = r.u32()? as usize;
                self.nodes[index].polygons = polygons(r, count)?;
                fixed_count(r, portal_count, 4)?;
                self.nodes[index].portal_polygons = (0..portal_count)
                    .map(|_| Ok([r.u16()? as i16, r.u16()? as i16]))
                    .collect::<Result<Vec<_>, DatError>>()?;
            }
        } else if self.kind != BspTreeKind::Cell {
            self.nodes[index].sphere = Some(sphere(r)?);
            if self.kind == BspTreeKind::Drawing {
                let count = r.u32()? as usize;
                self.nodes[index].polygons = polygons(r, count)?;
            }
        }
        Ok(index)
    }
}
