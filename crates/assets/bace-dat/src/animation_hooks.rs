//! Typed payloads from pinned ACE animation hooks, including melee attack cones.
use crate::{
    DatError, ModelFrame,
    env_cell::{frame, vec3},
    table_reader::TableReader,
};
#[derive(Clone, Debug, PartialEq)]
pub struct AttackCone {
    pub part_index: u32,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub radius: f32,
    pub height: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AnimationHookPayload {
    Empty,
    Id(u32),
    State(i32),
    Attack(AttackCone),
    ReplaceObject {
        raw_part_index: u16,
        part_index: u8,
        object_id: u32,
    },
    Transition {
        part: Option<u32>,
        start: f32,
        end: f32,
        time: f32,
    },
    Scale {
        end: f32,
        time: f32,
    },
    Particle {
        emitter_info_id: u32,
        part_index: u32,
        offset: ModelFrame,
        emitter_id: u32,
    },
    CallPes {
        pes: u32,
        pause: f32,
    },
    SoundTweaked {
        sound_id: u32,
        priority: f32,
        probability: f32,
        volume: f32,
    },
    Omega([f32; 3]),
    TextureVelocity {
        part: Option<u32>,
        uv: [f32; 2],
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationHook {
    pub kind: u32,
    pub direction: i32,
    pub payload: AnimationHookPayload,
}
impl AnimationHook {
    pub(crate) fn read(r: &mut TableReader<'_>) -> Result<Self, DatError> {
        let kind = r.u32()?;
        let direction = r.u32()? as i32;
        let payload = match kind {
            4 | 17 => AnimationHookPayload::Empty,
            1 | 2 | 14 | 15 | 16 | 18 => AnimationHookPayload::Id(r.u32()?),
            6 | 25 => AnimationHookPayload::State(r.u32()? as i32),
            3 => AnimationHookPayload::Attack(AttackCone {
                part_index: r.u32()?,
                left: [r.f32()?, r.f32()?],
                right: [r.f32()?, r.f32()?],
                radius: r.f32()?,
                height: r.f32()?,
            }),
            5 => {
                let raw_part_index = r.u16()?;
                AnimationHookPayload::ReplaceObject {
                    raw_part_index,
                    part_index: raw_part_index as u8,
                    object_id: r.known_id(0x01000000)?,
                }
            }
            7..=11 | 20 => {
                let part = if matches!(kind, 7 | 9 | 11) {
                    Some(r.u32()?)
                } else {
                    None
                };
                AnimationHookPayload::Transition {
                    part,
                    start: r.f32()?,
                    end: r.f32()?,
                    time: r.f32()?,
                }
            }
            12 => AnimationHookPayload::Scale {
                end: r.f32()?,
                time: r.f32()?,
            },
            13 | 26 => AnimationHookPayload::Particle {
                emitter_info_id: r.u32()?,
                part_index: r.u32()?,
                offset: frame(r)?,
                emitter_id: r.u32()?,
            },
            19 => AnimationHookPayload::CallPes {
                pes: r.u32()?,
                pause: r.f32()?,
            },
            21 => AnimationHookPayload::SoundTweaked {
                sound_id: r.u32()?,
                priority: r.f32()?,
                probability: r.f32()?,
                volume: r.f32()?,
            },
            22 => AnimationHookPayload::Omega(vec3(r)?),
            23 | 24 => {
                let part = if kind == 24 { Some(r.u32()?) } else { None };
                AnimationHookPayload::TextureVelocity {
                    part,
                    uv: [r.f32()?, r.f32()?],
                }
            }
            _ => return Err(DatError::Format("unsupported animation hook type")),
        };
        Ok(Self {
            kind,
            direction,
            payload,
        })
    }
}
