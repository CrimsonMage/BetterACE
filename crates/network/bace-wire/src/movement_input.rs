//! Untrusted ACE client movement observations. This parser neither accepts poses
//! nor derives jump impulses/contact. Authoritative physics must validate inputs.
use crate::position::{MovementEpochs, WirePosition, read_vector};
use crate::{Reader, WireError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientJump {
    pub extent: f32,
    pub reported_velocity: [f32; 3],
    pub epochs: MovementEpochs,
    pub object_id: u32,
    pub spell_id: u32,
    pub trailing_bytes: usize,
}
impl ClientJump {
    pub fn decode(payload: &[u8], max_bytes: usize) -> Result<Self, WireError> {
        let mut reader = bounded(payload, max_bytes)?;
        Ok(Self {
            extent: reader.f32()?,
            reported_velocity: read_vector(&mut reader)?,
            epochs: MovementEpochs::decode(&mut reader)?,
            object_id: reader.u32()?,
            spell_id: reader.u32()?,
            trailing_bytes: reader.remaining(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientAutonomousPosition {
    pub reported_position: WirePosition,
    pub epochs: MovementEpochs,
    pub reported_contact: bool,
    pub trailing_bytes: usize,
}
impl ClientAutonomousPosition {
    pub fn decode(payload: &[u8], max_bytes: usize) -> Result<Self, WireError> {
        let mut reader = bounded(payload, max_bytes)?;
        let reported_position = WirePosition::decode(&mut reader)?;
        let epochs = MovementEpochs::decode(&mut reader)?;
        let reported_contact = reader.take(1)?[0] != 0;
        align(&mut reader)?;
        Ok(Self {
            reported_position,
            epochs,
            reported_contact,
            trailing_bytes: reader.remaining(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionCommandItem {
    pub raw_command: u16,
    pub sequence: u16,
    pub autonomous: bool,
    pub speed: f32,
}
impl MotionCommandItem {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let raw_command = reader.u16()?;
        let sequence = reader.u16()?;
        Ok(Self {
            raw_command,
            sequence: sequence & 0x7fff,
            autonomous: sequence & 0x8000 != 0,
            speed: reader.f32()?,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct RawMotionState {
    pub current_hold_key: Option<u32>,
    pub current_style: Option<u32>,
    pub forward_command: Option<u32>,
    pub forward_hold_key: Option<u32>,
    pub forward_speed: Option<f32>,
    pub sidestep_command: Option<u32>,
    pub sidestep_hold_key: Option<u32>,
    pub sidestep_speed: Option<f32>,
    pub turn_command: Option<u32>,
    pub turn_hold_key: Option<u32>,
    pub turn_speed: Option<f32>,
    /// All wire commands are preserved; downstream policy must authorize them.
    pub commands: Vec<MotionCommandItem>,
}
impl RawMotionState {
    fn decode(reader: &mut Reader<'_>, max_commands: usize) -> Result<Self, WireError> {
        let packed = reader.u32()?;
        // ACE casts (packed >> 11) to ushort. Do not silently discard high count bits.
        if packed >> 27 != 0 {
            return Err(WireError::LimitExceeded);
        }
        let count = (packed >> 11) as usize;
        if count > max_commands {
            return Err(WireError::LimitExceeded);
        }
        let mut result = Self {
            current_hold_key: word(reader, packed, 1)?,
            current_style: word(reader, packed, 2)?,
            forward_command: word(reader, packed, 4)?,
            forward_hold_key: word(reader, packed, 8)?,
            forward_speed: scalar(reader, packed, 0x10)?,
            sidestep_command: word(reader, packed, 0x20)?,
            sidestep_hold_key: word(reader, packed, 0x40)?,
            sidestep_speed: scalar(reader, packed, 0x80)?,
            turn_command: word(reader, packed, 0x100)?,
            turn_hold_key: word(reader, packed, 0x200)?,
            turn_speed: scalar(reader, packed, 0x400)?,
            commands: Vec::new(),
        };
        if count > reader.remaining() / 8 {
            return Err(WireError::Truncated);
        }
        result.commands = (0..count)
            .map(|_| MotionCommandItem::decode(reader))
            .collect::<Result<_, _>>()?;
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ClientMoveToState {
    pub motion: RawMotionState,
    pub reported_position: WirePosition,
    pub epochs: MovementEpochs,
    pub contact_long_jump: u8,
    pub trailing_bytes: usize,
}
impl ClientMoveToState {
    pub fn decode(
        payload: &[u8],
        max_bytes: usize,
        max_commands: usize,
    ) -> Result<Self, WireError> {
        let mut reader = bounded(payload, max_bytes)?;
        let motion = RawMotionState::decode(&mut reader, max_commands)?;
        let reported_position = WirePosition::decode(&mut reader)?;
        let epochs = MovementEpochs::decode(&mut reader)?;
        let contact_long_jump = reader.take(1)?[0];
        align(&mut reader)?;
        Ok(Self {
            motion,
            reported_position,
            epochs,
            contact_long_jump,
            trailing_bytes: reader.remaining(),
        })
    }
    pub fn reported_contact(&self) -> bool {
        self.contact_long_jump & 1 != 0
    }
    pub fn reported_standing_long_jump(&self) -> bool {
        self.contact_long_jump & 2 != 0
    }
}
fn word(reader: &mut Reader<'_>, flags: u32, mask: u32) -> Result<Option<u32>, WireError> {
    if flags & mask != 0 {
        Ok(Some(reader.u32()?))
    } else {
        Ok(None)
    }
}
fn scalar(reader: &mut Reader<'_>, flags: u32, mask: u32) -> Result<Option<f32>, WireError> {
    if flags & mask != 0 {
        Ok(Some(reader.f32()?))
    } else {
        Ok(None)
    }
}
fn bounded(bytes: &[u8], limit: usize) -> Result<Reader<'_>, WireError> {
    if bytes.len() > limit {
        Err(WireError::LimitExceeded)
    } else {
        Ok(Reader::new(bytes))
    }
}
fn align(reader: &mut Reader<'_>) -> Result<(), WireError> {
    reader.take((4 - reader.position() % 4) % 4)?;
    Ok(())
}
