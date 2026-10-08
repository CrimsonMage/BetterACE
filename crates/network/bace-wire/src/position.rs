//! Explicit ACE position layouts. A decoded client position is an observation,
//! never an accepted physical state. Quaternion component order is W, X, Y, Z.
use crate::envelope::{expect_opcode, finish, message_writer};
use crate::opcode::GameMessageOpcode;
use crate::{Reader, WireError, Writer};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WirePosition {
    pub cell: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
}
impl WirePosition {
    pub fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            cell: reader.u32()?,
            origin: read_vector(reader)?,
            rotation: [reader.f32()?, reader.f32()?, reader.f32()?, reader.f32()?],
        })
    }
    pub fn write(self, writer: &mut Writer) {
        writer.u32(self.cell);
        write_vector(writer, self.origin);
        for component in self.rotation {
            writer.f32(component);
        }
    }
}
/// ACE GameMessagePrivateUpdatePosition (0x02DB): byte property sequence,
/// DWORD PositionType, then the uncompressed 32-byte Position value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrivatePositionUpdate {
    pub sequence: u8,
    pub position_type: u32,
    pub position: WirePosition,
}
impl PrivatePositionUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::PrivateUpdatePosition);
        writer.bytes(&[self.sequence]);
        writer.u32(self.position_type);
        self.position.write(&mut writer);
        writer.into_bytes()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::PrivateUpdatePosition)?;
        let value = Self {
            sequence: reader.take(1)?[0],
            position_type: reader.u32()?,
            position: WirePosition::decode(&mut reader)?,
        };
        finish(&reader)?;
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementEpochs {
    pub instance: u16,
    pub server_control: u16,
    pub teleport: u16,
    pub force_position: u16,
}
impl MovementEpochs {
    pub(crate) fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            instance: reader.u16()?,
            server_control: reader.u16()?,
            teleport: reader.u16()?,
            force_position: reader.u16()?,
        })
    }
    pub(crate) fn write(self, writer: &mut Writer) {
        for value in [
            self.instance,
            self.server_control,
            self.teleport,
            self.force_position,
        ] {
            writer.u16(value);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionPack {
    pub position: WirePosition,
    pub velocity: Option<[f32; 3]>,
    pub placement: Option<u32>,
    pub grounded: bool,
    pub instance_sequence: u16,
    pub position_sequence: u16,
    pub teleport_sequence: u16,
    pub force_position_sequence: u16,
}
impl PositionPack {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write(&mut writer);
        writer.into_bytes()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        let value = Self::read(&mut reader)?;
        finish(&reader)?;
        Ok(value)
    }
    pub(crate) fn write(self, writer: &mut Writer) {
        let mut flags = u32::from(self.velocity.is_some())
            | (u32::from(self.placement.is_some()) << 1)
            | (u32::from(self.grounded) << 2);
        for (index, value) in self.position.rotation.iter().enumerate() {
            if *value == 0.0 {
                flags |= 8 << index;
            }
        }
        writer.u32(flags);
        writer.u32(self.position.cell);
        write_vector(writer, self.position.origin);
        for (index, value) in self.position.rotation.iter().enumerate() {
            if flags & (8 << index) == 0 {
                writer.f32(*value);
            }
        }
        if let Some(velocity) = self.velocity {
            write_vector(writer, velocity);
        }
        if let Some(placement) = self.placement {
            writer.u32(placement);
        }
        for value in [
            self.instance_sequence,
            self.position_sequence,
            self.teleport_sequence,
            self.force_position_sequence,
        ] {
            writer.u16(value);
        }
    }
    fn read(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let flags = reader.u32()?;
        if flags & !0x7f != 0 {
            return Err(WireError::UnsupportedFlags(flags & !0x7f));
        }
        let cell = reader.u32()?;
        let origin = read_vector(reader)?;
        let mut rotation = [0.0; 4];
        for (index, value) in rotation.iter_mut().enumerate() {
            if flags & (8 << index) == 0 {
                *value = reader.f32()?;
            }
        }
        let velocity = if flags & 1 != 0 {
            Some(read_vector(reader)?)
        } else {
            None
        };
        let placement = if flags & 2 != 0 {
            Some(reader.u32()?)
        } else {
            None
        };
        Ok(Self {
            position: WirePosition {
                cell,
                origin,
                rotation,
            },
            velocity,
            placement,
            grounded: flags & 4 != 0,
            instance_sequence: reader.u16()?,
            position_sequence: reader.u16()?,
            teleport_sequence: reader.u16()?,
            force_position_sequence: reader.u16()?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionUpdate {
    pub object_id: u32,
    pub pack: PositionPack,
}
impl PositionUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::UpdatePosition);
        writer.u32(self.object_id);
        self.pack.write(&mut writer);
        writer.into_bytes()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::UpdatePosition)?;
        let value = Self {
            object_id: reader.u32()?,
            pack: PositionPack::read(&mut reader)?,
        };
        finish(&reader)?;
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorUpdate {
    pub object_id: u32,
    pub velocity: [f32; 3],
    pub omega: [f32; 3],
    pub instance_sequence: u16,
    pub vector_sequence: u16,
}
impl VectorUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::VectorUpdate);
        writer.u32(self.object_id);
        write_vector(&mut writer, self.velocity);
        write_vector(&mut writer, self.omega);
        writer.u16(self.instance_sequence);
        writer.u16(self.vector_sequence);
        writer.into_bytes()
    }
}
/// Server output form omits the cell and writes a full DWORD contact value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutonomousPositionOutput {
    pub object_id: u32,
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
    pub epochs: MovementEpochs,
}
impl AutonomousPositionOutput {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::AutonomousPosition);
        writer.u32(self.object_id);
        write_vector(&mut writer, self.origin);
        for value in self.rotation {
            writer.f32(value);
        }
        self.epochs.write(&mut writer);
        writer.u32(1);
        writer.into_bytes()
    }
}
pub(crate) fn read_vector(reader: &mut Reader<'_>) -> Result<[f32; 3], WireError> {
    Ok([reader.f32()?, reader.f32()?, reader.f32()?])
}
pub(crate) fn write_vector(writer: &mut Writer, vector: [f32; 3]) {
    for value in vector {
        writer.f32(value);
    }
}
