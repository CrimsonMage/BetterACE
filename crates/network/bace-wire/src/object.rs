//! Complete object create/update composition and model updates. Inputs are
//! immutable wire projections; this module never invents or mutates world state.
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode;
use crate::{ObjectGameData, ObjectModel, PhysicsDescription, WireError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectCodecLimits {
    pub max_message_bytes: usize,
    pub max_model_entries: usize,
    pub max_children: usize,
    pub max_restrictions: usize,
    pub max_motion_commands: usize,
    pub max_string_bytes: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectDescription {
    pub object_id: u32,
    pub model: ObjectModel,
    pub physics: PhysicsDescription,
    pub game: ObjectGameData,
}
impl ObjectDescription {
    pub fn encode_create(&self, limits: ObjectCodecLimits) -> Result<Vec<u8>, WireError> {
        self.encode(GameMessageOpcode::ObjectCreate, limits)
    }
    pub fn encode_update(&self, limits: ObjectCodecLimits) -> Result<Vec<u8>, WireError> {
        self.encode(GameMessageOpcode::UpdateObject, limits)
    }
    fn encode(
        &self,
        opcode: GameMessageOpcode,
        limits: ObjectCodecLimits,
    ) -> Result<Vec<u8>, WireError> {
        let mut writer = message_writer(opcode);
        writer.u32(self.object_id);
        self.model.write(&mut writer, limits.max_model_entries)?;
        self.physics
            .write(&mut writer, limits.max_children, limits.max_motion_commands)?;
        self.game.write(
            &mut writer,
            limits.max_string_bytes,
            limits.max_restrictions,
        )?;
        if writer.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(writer.into_bytes())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceUpdate {
    pub object_id: u32,
    pub model: ObjectModel,
    pub instance_sequence: u16,
    pub visual_sequence: u16,
}
impl AppearanceUpdate {
    pub fn encode(&self, max_model_entries: usize) -> Result<Vec<u8>, WireError> {
        let mut writer = message_writer(GameMessageOpcode::ObjDescEvent);
        writer.u32(self.object_id);
        self.model.write(&mut writer, max_model_entries)?;
        writer.u16(self.instance_sequence);
        writer.u16(self.visual_sequence);
        Ok(writer.into_bytes())
    }
}
