//! Bounded RegionDesc header/LandDefs projection. The remaining RegionDesc
//! sections are explicitly outside this decoder; no full-record claim is made.
use crate::{DatError, DatTableLimits, table_reader::TableReader};
#[derive(Clone, Debug)]
pub struct RegionLand {
    pub region_number: u32,
    pub version: u32,
    pub name: String,
    pub block_length: i32,
    pub block_width: i32,
    pub square_length: f32,
    pub landblock_length: i32,
    pub vertices_per_cell: i32,
    pub maximum_object_height: f32,
    pub sky_height: f32,
    pub road_width: f32,
    pub heights: [f32; 256],
}
impl RegionLand {
    pub fn decode_prefix(bytes: &[u8]) -> Result<Self, DatError> {
        let mut r = TableReader::new(bytes, 0x13000000, DatTableLimits::default())?;
        let region_number = r.u32()?;
        let version = r.u32()?;
        let name = r.pstring()?;
        r.align()?;
        let block_length = r.u32()? as i32;
        let block_width = r.u32()? as i32;
        let square_length = r.f32()?;
        let landblock_length = r.u32()? as i32;
        let vertices_per_cell = r.u32()? as i32;
        let maximum_object_height = r.f32()?;
        let sky_height = r.f32()?;
        let road_width = r.f32()?;
        let mut heights = [0.0; 256];
        for height in &mut heights {
            *height = r.f32()?;
        }
        Ok(Self {
            region_number,
            version,
            name,
            block_length,
            block_width,
            square_length,
            landblock_length,
            vertices_per_cell,
            maximum_object_height,
            sky_height,
            road_width,
            heights,
        })
    }
}
