//! Complete pinned SetupModel layout retained for collision preparation.
//! Source ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only.
use crate::{
    AnimationFrame, AnimationHook, BspSphere, DatError, DatTableLimits, ModelFrame,
    env_cell::{fixed_count, frame, vec3},
    table_reader::TableReader,
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionCylinder {
    pub origin: [f32; 3],
    pub radius: f32,
    pub height: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SetupLocation {
    pub part_id: i32,
    pub frame: ModelFrame,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SetupLight {
    pub frame: ModelFrame,
    pub color: u32,
    pub intensity: f32,
    pub falloff: f32,
    pub cone_angle: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionSetup {
    pub id: u32,
    pub flags: u32,
    pub parts: Vec<u32>,
    pub parents: Option<Vec<u32>>,
    pub default_scales: Option<Vec<[f32; 3]>>,
    pub holding_locations: BTreeMap<i32, SetupLocation>,
    pub connection_points: BTreeMap<i32, SetupLocation>,
    pub placements: BTreeMap<i32, AnimationFrame>,
    pub cylinders: Vec<CollisionCylinder>,
    pub spheres: Vec<BspSphere>,
    pub height: f32,
    pub radius: f32,
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub sorting_sphere: BspSphere,
    pub selection_sphere: BspSphere,
    pub lights: BTreeMap<i32, SetupLight>,
    pub default_animation: u32,
    pub default_script: u32,
    pub default_motion_table: u32,
    pub default_sound_table: u32,
    pub default_script_table: u32,
}
fn count(r: &mut TableReader<'_>, minimum: usize) -> Result<usize, DatError> {
    let count = r.u32()? as usize;
    fixed_count(r, count, minimum)?;
    Ok(count)
}
fn sphere(r: &mut TableReader<'_>) -> Result<BspSphere, DatError> {
    Ok(BspSphere {
        origin: vec3(r)?,
        radius: r.f32()?,
    })
}
fn locations(r: &mut TableReader<'_>) -> Result<BTreeMap<i32, SetupLocation>, DatError> {
    let mut values = BTreeMap::new();
    for _ in 0..count(r, 36)? {
        let key = r.u32()? as i32;
        let value = SetupLocation {
            part_id: r.u32()? as i32,
            frame: frame(r)?,
        };
        if values.insert(key, value).is_some() {
            return Err(DatError::Format("duplicate setup location"));
        }
    }
    Ok(values)
}
impl CollisionSetup {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated collision setup"))?
                .try_into()
                .map_err(|_| DatError::Format("setup ID"))?,
        );
        if id >> 24 != 2 {
            return Err(DatError::Format("wrong collision setup record ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let flags = r.u32()?;
        let n = count(&mut r, 4)?;
        let parts = (0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?;
        let parents = if flags & 1 != 0 {
            fixed_count(&mut r, n, 4)?;
            Some((0..n).map(|_| r.u32()).collect::<Result<Vec<_>, _>>()?)
        } else {
            None
        };
        let default_scales = if flags & 2 != 0 {
            fixed_count(&mut r, n, 12)?;
            Some(
                (0..n)
                    .map(|_| vec3(&mut r))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };
        let holding_locations = locations(&mut r)?;
        let connection_points = locations(&mut r)?;
        let mut placements = BTreeMap::new();
        for _ in 0..count(&mut r, 8)? {
            let key = r.u32()? as i32;
            fixed_count(&mut r, n, 28)?;
            let parts = (0..n)
                .map(|_| frame(&mut r))
                .collect::<Result<Vec<_>, _>>()?;
            let count = count(&mut r, 8)?;
            let hooks = (0..count)
                .map(|_| AnimationHook::read(&mut r))
                .collect::<Result<Vec<_>, _>>()?;
            if placements
                .insert(key, AnimationFrame { parts, hooks })
                .is_some()
            {
                return Err(DatError::Format("duplicate setup placement"));
            }
        }
        let mut cylinders = Vec::new();
        for _ in 0..count(&mut r, 20)? {
            cylinders.push(CollisionCylinder {
                origin: vec3(&mut r)?,
                radius: r.f32()?,
                height: r.f32()?,
            });
        }
        let mut spheres = Vec::new();
        for _ in 0..count(&mut r, 16)? {
            spheres.push(sphere(&mut r)?);
        }
        let height = r.f32()?;
        let radius = r.f32()?;
        let step_up_height = r.f32()?;
        let step_down_height = r.f32()?;
        let sorting_sphere = sphere(&mut r)?;
        let selection_sphere = sphere(&mut r)?;
        let mut lights = BTreeMap::new();
        for _ in 0..count(&mut r, 48)? {
            let key = r.u32()? as i32;
            let light = SetupLight {
                frame: frame(&mut r)?,
                color: r.u32()?,
                intensity: r.f32()?,
                falloff: r.f32()?,
                cone_angle: r.f32()?,
            };
            if lights.insert(key, light).is_some() {
                return Err(DatError::Format("duplicate setup light"));
            }
        }
        let default_animation = r.u32()?;
        let default_script = r.u32()?;
        let default_motion_table = r.u32()?;
        let default_sound_table = r.u32()?;
        let default_script_table = r.u32()?;
        r.finish()?;
        Ok(Self {
            id,
            flags,
            parts,
            parents,
            default_scales,
            holding_locations,
            connection_points,
            placements,
            cylinders,
            spheres,
            height,
            radius,
            step_up_height,
            step_down_height,
            sorting_sphere,
            selection_sphere,
            lights,
            default_animation,
            default_script,
            default_motion_table,
            default_sound_table,
            default_script_table,
        })
    }
}
