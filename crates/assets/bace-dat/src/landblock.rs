use crate::DatError;

/// Pinned ACE.DatLoader/FileTypes/CellLandblock.cs layout. This is decoded
/// terrain data; height interpretation and collision triangulation are not
/// implemented by this data structure.
#[derive(Debug, Clone, PartialEq)]
pub struct Landblock {
    pub id: u32,
    pub has_objects: bool,
    pub terrain: [u16; 81],
    pub heights: [u8; 81],
}

impl Landblock {
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        if bytes.len() != 252 {
            return Err(DatError::Format("landblock must be 252 bytes"));
        }
        let id = u32::from_le_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| DatError::Format("landblock id"))?,
        );
        if id & 0xffff != 0xffff {
            return Err(DatError::Format("landblock ID suffix"));
        }
        let objects = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| DatError::Format("landblock flags"))?,
        );
        let mut terrain = [0; 81];
        for (i, value) in terrain.iter_mut().enumerate() {
            *value = u16::from_le_bytes([bytes[8 + i * 2], bytes[9 + i * 2]]);
        }
        let mut heights = [0; 81];
        heights.copy_from_slice(&bytes[170..251]);
        Ok(Self {
            id,
            has_objects: objects == 1,
            terrain,
            heights,
        })
    }
}
