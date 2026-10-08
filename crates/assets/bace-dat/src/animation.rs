//! Pinned ACE Animation/AnimationFrame layout. Immutable assets, no I/O/timing.
use crate::{
    AnimationHook, AnimationHookPayload, AttackCone, DatError, DatTableLimits, ModelFrame,
    env_cell::{fixed_count, frame},
    table_reader::TableReader,
};
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationFrame {
    pub parts: Vec<ModelFrame>,
    pub hooks: Vec<AnimationHook>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub id: u32,
    pub flags: u32,
    pub num_parts: u32,
    pub num_frames: u32,
    pub position_frames: Vec<ModelFrame>,
    pub frames: Vec<AnimationFrame>,
}
impl Animation {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let id = u32::from_le_bytes(
            bytes
                .get(..4)
                .ok_or(DatError::Format("truncated animation"))?
                .try_into()
                .map_err(|_| DatError::Format("animation ID"))?,
        );
        if id >> 24 != 3 {
            return Err(DatError::Format("wrong animation record ID"));
        }
        let mut r = TableReader::new(bytes, id, limits)?;
        let flags = r.u32()?;
        let num_parts = r.u32()?;
        let num_frames = r.u32()?;
        if num_parts as usize > limits.max_entries {
            return Err(DatError::Format("animation part limit"));
        }
        fixed_count(&mut r, num_frames as usize, 4)?;
        let position_frames = if flags & 1 != 0 {
            fixed_count(&mut r, num_frames as usize, 28)?;
            (0..num_frames)
                .map(|_| frame(&mut r))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let mut frames = Vec::new();
        for _ in 0..num_frames {
            fixed_count(&mut r, num_parts as usize, 28)?;
            let parts = (0..num_parts)
                .map(|_| frame(&mut r))
                .collect::<Result<Vec<_>, _>>()?;
            let count = r.u32()? as usize;
            fixed_count(&mut r, count, 8)?;
            let hooks = (0..count)
                .map(|_| AnimationHook::read(&mut r))
                .collect::<Result<Vec<_>, _>>()?;
            frames.push(AnimationFrame { parts, hooks });
        }
        r.finish()?;
        Ok(Self {
            id,
            flags,
            num_parts,
            num_frames,
            position_frames,
            frames,
        })
    }
    /// Authored frame positions and directions only. Scheduling remains gameplay's
    /// responsibility; this iterator does not invent animation timing or hits.
    pub fn attack_hooks(&self) -> impl Iterator<Item = (usize, i32, &AttackCone)> {
        self.frames.iter().enumerate().flat_map(|(index, frame)| {
            frame
                .hooks
                .iter()
                .filter_map(move |hook| match &hook.payload {
                    AnimationHookPayload::Attack(cone) => Some((index, hook.direction, cone)),
                    _ => None,
                })
        })
    }
}
