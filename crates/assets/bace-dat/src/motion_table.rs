//! Pinned MotionTable/MotionData/AnimData binary layouts; no gameplay formulas.
use crate::{
    DatError, DatTableLimits,
    env_cell::{fixed_count, vec3},
    table_reader::TableReader,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationSegment {
    pub animation_id: u32,
    pub low_frame: i32,
    pub high_frame: i32,
    pub framerate: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionData {
    pub bitfield: u8,
    pub flags: u8,
    pub animations: Vec<AnimationSegment>,
    pub velocity: Option<[f32; 3]>,
    pub omega: Option<[f32; 3]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionTable {
    pub id: u32,
    pub default_style: u32,
    pub style_defaults: BTreeMap<u32, u32>,
    pub cycles: BTreeMap<u32, MotionData>,
    pub modifiers: BTreeMap<u32, MotionData>,
    pub links: BTreeMap<u32, BTreeMap<u32, MotionData>>,
}
fn count(r: &mut TableReader<'_>, minimum: usize) -> Result<usize, DatError> {
    let count = r.u32()? as usize;
    fixed_count(r, count, minimum)?;
    Ok(count)
}
fn data(r: &mut TableReader<'_>) -> Result<MotionData, DatError> {
    let animations = usize::from(r.u8()?);
    let bitfield = r.u8()?;
    let flags = r.u8()?;
    r.align()?;
    fixed_count(r, animations, 16)?;
    let animations = (0..animations)
        .map(|_| {
            Ok(AnimationSegment {
                animation_id: r.u32()?,
                low_frame: r.u32()? as i32,
                high_frame: r.u32()? as i32,
                framerate: r.f32()?,
            })
        })
        .collect::<Result<Vec<_>, DatError>>()?;
    let velocity = if flags & 1 != 0 { Some(vec3(r)?) } else { None };
    let omega = if flags & 2 != 0 { Some(vec3(r)?) } else { None };
    Ok(MotionData {
        bitfield,
        flags,
        animations,
        velocity,
        omega,
    })
}
fn map(r: &mut TableReader<'_>) -> Result<BTreeMap<u32, MotionData>, DatError> {
    let mut values = BTreeMap::new();
    for _ in 0..count(r, 8)? {
        let key = r.u32()?;
        if values.insert(key, data(r)?).is_some() {
            return Err(DatError::Format("duplicate motion key"));
        }
    }
    Ok(values)
}
impl MotionTable {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated motion table"))?
                .try_into()
                .map_err(|_| DatError::Format("motion ID"))?,
        );
        if id >> 24 != 9 {
            return Err(DatError::Format("wrong motion table record ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let default_style = r.u32()?;
        let mut style_defaults = BTreeMap::new();
        for _ in 0..count(&mut r, 8)? {
            let key = r.u32()?;
            if style_defaults.insert(key, r.u32()?).is_some() {
                return Err(DatError::Format("duplicate motion style"));
            }
        }
        let cycles = map(&mut r)?;
        let modifiers = map(&mut r)?;
        let mut links = BTreeMap::new();
        for _ in 0..count(&mut r, 8)? {
            let key = r.u32()?;
            if links.insert(key, map(&mut r)?).is_some() {
                return Err(DatError::Format("duplicate motion link"));
            }
        }
        r.finish()?;
        Ok(Self {
            id,
            default_style,
            style_defaults,
            cycles,
            modifiers,
            links,
        })
    }
}
