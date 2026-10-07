//! ACE DDD message layouts. Asset retrieval, compression and update policy are
//! separate responsibilities; these codecs operate on explicit bounded inputs.
use crate::envelope::{expect_opcode, finish, message_writer};
use crate::opcode::GameMessageOpcode;
use crate::{Reader, WireError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DddDatabase {
    Portal,
    Language,
    Cell,
    HighRes,
}
impl DddDatabase {
    pub fn wire_identity(self) -> (u32, u32) {
        match self {
            Self::Portal => (0, 1),
            Self::Language => (1, 3),
            Self::Cell => (1, 2),
            Self::HighRes => (u32::from_le_bytes(*b"HiFi"), 1),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DddIteration {
    pub database: DddDatabase,
    pub iteration: u32,
    pub files: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DddBegin {
    pub total_file_size: u32,
    pub iterations: Vec<DddIteration>,
}
impl DddBegin {
    pub fn encode(&self, max_iterations: usize, max_files: usize) -> Result<Vec<u8>, WireError> {
        if self.iterations.len() > max_iterations {
            return Err(WireError::LimitExceeded);
        }
        let count = u32::try_from(self.iterations.len()).map_err(|_| WireError::LimitExceeded)?;
        let mut file_count = 0usize;
        for entry in &self.iterations {
            file_count = file_count
                .checked_add(entry.files.len())
                .ok_or(WireError::LimitExceeded)?;
            if entry.files.len() > u32::MAX as usize || file_count > max_files {
                return Err(WireError::LimitExceeded);
            }
        }
        let mut writer = message_writer(GameMessageOpcode::DDD_BeginDDD);
        writer.u32(self.total_file_size);
        writer.u32(count);
        // ACE explicitly groups in this order, preserving iteration insertion order.
        for database in [
            DddDatabase::Portal,
            DddDatabase::Language,
            DddDatabase::Cell,
            DddDatabase::HighRes,
        ] {
            for entry in self
                .iterations
                .iter()
                .filter(|entry| entry.database == database)
            {
                let (kind, id) = database.wire_identity();
                writer.u32(kind);
                writer.u32(id);
                writer.u32(entry.iteration);
                if database == DddDatabase::Cell {
                    writer.u32(0);
                }
                writer.u32(entry.files.len() as u32);
                for file in &entry.files {
                    writer.u32(*file);
                }
                if database != DddDatabase::Cell {
                    writer.u32(0);
                }
            }
        }
        Ok(writer.into_bytes())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DddData<'a> {
    pub database: DddDatabase,
    pub resource_type: u32,
    pub object_id: u32,
    pub iteration: u32,
    pub compressed: bool,
    /// Already prepared raw/zlib bytes. No decompression occurs in the codec.
    pub data: &'a [u8],
}
impl DddData<'_> {
    pub fn encode(&self, max_data_bytes: usize) -> Result<Vec<u8>, WireError> {
        if self.data.len() > max_data_bytes {
            return Err(WireError::LimitExceeded);
        }
        let length = self
            .data
            .len()
            .checked_add(4)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(WireError::LimitExceeded)?;
        let mut writer = message_writer(GameMessageOpcode::DDD_DataMessage);
        let (kind, id) = self.database.wire_identity();
        writer.u32(kind);
        writer.u32(id);
        writer.u32(self.resource_type);
        writer.u32(self.object_id);
        writer.u32(self.iteration);
        // BinaryWriter.Write(bool) is ONE byte; the following words are unaligned.
        writer.bytes(&[u8::from(self.compressed)]);
        writer.u32(3);
        writer.u32(length as u32);
        writer.bytes(self.data);
        Ok(writer.into_bytes())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DddRequestData {
    pub resource_type: u32,
    pub object_id: u32,
}
impl DddRequestData {
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::DDD_RequestDataMessage)?;
        let result = Self {
            resource_type: reader.u32()?,
            object_id: reader.u32()?,
        };
        finish(&reader)?;
        Ok(result)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DddControl {
    Interrogation {
        allow_highres: bool,
    },
    Error {
        resource_type: u32,
        object_id: u32,
        error_type: u32,
    },
    End,
}
impl DddControl {
    pub fn encode(self) -> Vec<u8> {
        let opcode = match self {
            Self::Interrogation { .. } => GameMessageOpcode::DDD_Interrogation,
            Self::Error { .. } => GameMessageOpcode::DDD_ErrorMessage,
            Self::End => GameMessageOpcode::DDD_EndDDD,
        };
        let mut writer = message_writer(opcode);
        match self {
            Self::Interrogation { allow_highres } => {
                for value in [1, 1, if allow_highres { 5 } else { 1 }, 2, 0, 1] {
                    writer.u32(value);
                }
            }
            Self::Error {
                resource_type,
                object_id,
                error_type,
            } => {
                writer.u32(resource_type);
                writer.u32(object_id);
                writer.u32(error_type);
            }
            Self::End => {}
        }
        writer.into_bytes()
    }
}
