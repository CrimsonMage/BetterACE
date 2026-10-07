//! Character UI output from ACE Network/GameMessages/Messages/GameMessageCharacter*.cs.
//! Values requiring clocks or account policy are supplied explicitly by callers.
use crate::envelope::{expect_opcode, finish, message_writer};
use crate::opcode::{CharacterError, GameMessageOpcode};
use crate::{Reader, WireError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterListEntry {
    pub object_id: u32,
    /// Final display name, including any authorized privilege prefix.
    pub name: String,
    pub seconds_disabled: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterList {
    pub characters: Vec<CharacterListEntry>,
    pub slot_count: u32,
    pub account: String,
    pub use_turbine_chat: bool,
    pub has_throne_of_destiny: bool,
}
impl CharacterList {
    pub fn encode(&self, max_characters: usize) -> Result<Vec<u8>, WireError> {
        if self.characters.len() > max_characters {
            return Err(WireError::LimitExceeded);
        }
        let count = u32::try_from(self.characters.len()).map_err(|_| WireError::LimitExceeded)?;
        let mut writer = message_writer(GameMessageOpcode::CharacterList);
        writer.u32(0);
        writer.u32(count);
        for character in &self.characters {
            writer.u32(character.object_id);
            writer.string16(&character.name)?;
            writer.u32(character.seconds_disabled);
        }
        writer.u32(0);
        writer.u32(self.slot_count);
        writer.string16(&self.account)?;
        writer.u32(u32::from(self.use_turbine_chat));
        writer.u32(u32::from(self.has_throne_of_destiny));
        Ok(writer.into_bytes())
    }
    pub fn decode(
        bytes: &[u8],
        max_characters: usize,
        max_string_bytes: usize,
    ) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::CharacterList)?;
        if reader.u32()? != 0 {
            return Err(WireError::InvalidEncoding);
        }
        let count = reader.u32()? as usize;
        if count > max_characters {
            return Err(WireError::LimitExceeded);
        }
        if count > reader.remaining() / 12 {
            return Err(WireError::Truncated);
        }
        let mut characters = Vec::with_capacity(count);
        for _ in 0..count {
            characters.push(CharacterListEntry {
                object_id: reader.u32()?,
                name: reader.string16(max_string_bytes)?,
                seconds_disabled: reader.u32()?,
            });
        }
        if reader.u32()? != 0 {
            return Err(WireError::InvalidEncoding);
        }
        let result = Self {
            characters,
            slot_count: reader.u32()?,
            account: reader.string16(max_string_bytes)?,
            use_turbine_chat: read_bool(&mut reader)?,
            has_throne_of_destiny: read_bool(&mut reader)?,
        };
        finish(&reader)?;
        Ok(result)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CharacterReply {
    Error(CharacterError),
    /// Upstream CharacterGenerationVerificationResponse::Ok = 1.
    Created {
        object_id: u32,
        name: String,
    },
    /// Raw failure code; 1 is reserved for the success payload.
    CreateFailed(u32),
    Restored {
        object_id: u32,
        name: String,
        seconds_disabled: u32,
    },
    Deleted,
    LoggedOff,
    WorldServerReady,
}
impl CharacterReply {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let opcode = match self {
            Self::Error(_) => GameMessageOpcode::CharacterError,
            Self::Created { .. } | Self::CreateFailed(_) | Self::Restored { .. } => {
                GameMessageOpcode::CharacterCreateResponse
            }
            Self::Deleted => GameMessageOpcode::CharacterDelete,
            Self::LoggedOff => GameMessageOpcode::CharacterLogOff,
            Self::WorldServerReady => GameMessageOpcode::CharacterEnterWorldServerReady,
        };
        let mut writer = message_writer(opcode);
        match self {
            Self::Error(error) => writer.u32(error.0),
            Self::Created { object_id, name }
            | Self::Restored {
                object_id, name, ..
            } => {
                writer.u32(1);
                writer.u32(*object_id);
                writer.string16(name)?;
                writer.u32(
                    if let Self::Restored {
                        seconds_disabled, ..
                    } = self
                    {
                        *seconds_disabled
                    } else {
                        0
                    },
                );
            }
            Self::CreateFailed(code) => {
                if *code == 1 {
                    return Err(WireError::InvalidEncoding);
                }
                writer.u32(*code);
            }
            Self::Deleted | Self::LoggedOff | Self::WorldServerReady => {}
        }
        Ok(writer.into_bytes())
    }
}
fn read_bool(reader: &mut Reader<'_>) -> Result<bool, WireError> {
    match reader.u32()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::InvalidEncoding),
    }
}
