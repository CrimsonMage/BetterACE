//! ACE MovementData/InterpretedMotionState output layouts. Callers supply
//! authoritative motion and counters; no client movement is accepted here.
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode;
use crate::position::write_vector;
use crate::{MotionCommandItem, WireError, Writer};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InterpretedMotion {
    pub current_style: Option<u16>,
    pub forward_command: Option<u16>,
    pub sidestep_command: Option<u16>,
    pub turn_command: Option<u16>,
    pub forward_speed: Option<f32>,
    pub sidestep_speed: Option<f32>,
    pub turn_speed: Option<f32>,
    pub commands: Vec<MotionCommandItem>,
}
impl InterpretedMotion {
    fn write(&self, writer: &mut Writer, max_commands: usize) -> Result<(), WireError> {
        if self.commands.len() > max_commands || self.commands.len() > (u32::MAX >> 7) as usize {
            return Err(WireError::LimitExceeded);
        }
        if self
            .commands
            .iter()
            .any(|command| command.sequence > 0x7fff)
        {
            return Err(WireError::InvalidEncoding);
        }
        let mut flags = (self.commands.len() as u32) << 7;
        for (present, mask) in [
            (self.current_style.is_some(), 1),
            (self.forward_command.is_some(), 2),
            (self.forward_speed.is_some(), 4),
            (self.sidestep_command.is_some(), 8),
            (self.sidestep_speed.is_some(), 0x10),
            (self.turn_command.is_some(), 0x20),
            (self.turn_speed.is_some(), 0x40),
        ] {
            if present {
                flags |= mask;
            }
        }
        writer.u32(flags);
        for value in [
            self.current_style,
            self.forward_command,
            self.sidestep_command,
            self.turn_command,
        ]
        .into_iter()
        .flatten()
        {
            writer.u16(value);
        }
        for value in [self.forward_speed, self.sidestep_speed, self.turn_speed]
            .into_iter()
            .flatten()
        {
            writer.f32(value);
        }
        for command in &self.commands {
            writer.u16(command.raw_command);
            writer.u16(command.sequence | (u16::from(command.autonomous) << 15));
            writer.f32(command.speed);
        }
        writer.align4();
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveToParameters {
    pub flags: u32,
    pub distance_to_object: f32,
    pub min_distance: f32,
    pub fail_distance: f32,
    pub speed: f32,
    pub walk_run_threshold: f32,
    pub desired_heading: f32,
}
impl MoveToParameters {
    fn write(self, writer: &mut Writer) {
        writer.u32(self.flags);
        for value in [
            self.distance_to_object,
            self.min_distance,
            self.fail_distance,
            self.speed,
            self.walk_run_threshold,
            self.desired_heading,
        ] {
            writer.f32(value);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnToParameters {
    pub flags: u32,
    pub speed: f32,
    pub desired_heading: f32,
}
impl TurnToParameters {
    fn write(self, writer: &mut Writer) {
        writer.u32(self.flags);
        writer.f32(self.speed);
        writer.f32(self.desired_heading);
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum MotionBody {
    State {
        state: InterpretedMotion,
        sticky_object: Option<u32>,
    },
    MoveToObject {
        target: u32,
        cell: u32,
        origin: [f32; 3],
        parameters: MoveToParameters,
        run_rate: f32,
    },
    MoveToPosition {
        cell: u32,
        origin: [f32; 3],
        parameters: MoveToParameters,
        run_rate: f32,
    },
    TurnToObject {
        target: u32,
        desired_heading: f32,
        parameters: TurnToParameters,
    },
    TurnToHeading {
        parameters: TurnToParameters,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionUpdate {
    pub object_id: u32,
    pub instance_sequence: u16,
    pub movement_sequence: u16,
    pub server_control_sequence: u16,
    pub autonomous: bool,
    pub motion_flags: u8,
    pub current_style: u16,
    pub body: MotionBody,
}
impl MotionUpdate {
    pub fn encode(&self, max_commands: usize) -> Result<Vec<u8>, WireError> {
        if self.motion_flags & !3 != 0 {
            return Err(WireError::UnsupportedFlags(u32::from(
                self.motion_flags & !3,
            )));
        }
        if let MotionBody::State { sticky_object, .. } = &self.body
            && (self.motion_flags & 1 != 0) != sticky_object.is_some()
        {
            return Err(WireError::InvalidEncoding);
        }
        let mut writer = message_writer(GameMessageOpcode::Motion);
        writer.u32(self.object_id);
        writer.u16(self.instance_sequence);
        writer.u16(self.movement_sequence);
        writer.u16(self.server_control_sequence);
        writer.bytes(&[u8::from(self.autonomous)]);
        writer.align4();
        write_movement(
            &mut writer,
            self.motion_flags,
            self.current_style,
            &self.body,
            max_commands,
        )?;
        Ok(writer.into_bytes())
    }
}

/// MovementData without per-message counters, embedded in PhysicsDesc.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementDescription {
    pub autonomous: bool,
    pub motion_flags: u8,
    pub current_style: u16,
    pub body: MotionBody,
}
impl MovementDescription {
    pub fn encode(&self, max_commands: usize) -> Result<Vec<u8>, WireError> {
        let mut writer = Writer::new();
        write_movement(
            &mut writer,
            self.motion_flags,
            self.current_style,
            &self.body,
            max_commands,
        )?;
        Ok(writer.into_bytes())
    }
}
fn write_movement(
    writer: &mut Writer,
    motion_flags: u8,
    current_style: u16,
    body: &MotionBody,
    max_commands: usize,
) -> Result<(), WireError> {
    if motion_flags & !3 != 0 {
        return Err(WireError::UnsupportedFlags(u32::from(motion_flags & !3)));
    }
    if let MotionBody::State { sticky_object, .. } = body
        && (motion_flags & 1 != 0) != sticky_object.is_some()
    {
        return Err(WireError::InvalidEncoding);
    }
    let movement_type = match body {
        MotionBody::State { .. } => 0,
        MotionBody::MoveToObject { .. } => 6,
        MotionBody::MoveToPosition { .. } => 7,
        MotionBody::TurnToObject { .. } => 8,
        MotionBody::TurnToHeading { .. } => 9,
    };
    writer.bytes(&[movement_type, motion_flags]);
    writer.u16(current_style);
    match body {
        MotionBody::State {
            state,
            sticky_object,
        } => {
            state.write(writer, max_commands)?;
            if let Some(object) = sticky_object {
                writer.u32(*object);
            }
        }
        MotionBody::MoveToObject {
            target,
            cell,
            origin,
            parameters,
            run_rate,
        } => {
            writer.u32(*target);
            writer.u32(*cell);
            write_vector(writer, *origin);
            parameters.write(writer);
            writer.f32(*run_rate);
        }
        MotionBody::MoveToPosition {
            cell,
            origin,
            parameters,
            run_rate,
        } => {
            writer.u32(*cell);
            write_vector(writer, *origin);
            parameters.write(writer);
            writer.f32(*run_rate);
        }
        MotionBody::TurnToObject {
            target,
            desired_heading,
            parameters,
        } => {
            writer.u32(*target);
            writer.f32(*desired_heading);
            parameters.write(writer);
        }
        MotionBody::TurnToHeading { parameters } => parameters.write(writer),
    }
    Ok(())
}
