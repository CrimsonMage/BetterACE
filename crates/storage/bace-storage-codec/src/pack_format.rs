use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const HEADER: usize = 128;
pub(crate) const ENTRY: usize = 72;
pub(crate) const DIRECTORY_ENTRY: usize = 80;
pub(crate) const PAGE_RECORDS: usize = 32;
pub(crate) const MAGIC: &[u8; 8] = b"BACEPACK";

/// Namespace is assigned by the owning domain; the codec does not interpret it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PackKey {
    pub namespace: u16,
    pub id: u64,
}

#[derive(Debug, Clone)]
pub struct PackRecord {
    pub key: PackKey,
    pub schema: u16,
    /// None is an explicit deletion, distinct from an empty record.
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackDescriptor {
    pub generation: [u8; 32],
    pub file_name: String,
    pub record_count: u64,
}

impl PackDescriptor {
    pub(crate) fn new(generation: [u8; 32], record_count: u64) -> Self {
        Self {
            file_name: format!("{}.bace", hex(&generation)),
            generation,
            record_count,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PackLimits {
    pub max_file_bytes: u64,
    pub max_record_bytes: usize,
    pub max_directory_bytes: usize,
    pub max_records: u64,
    pub max_segments: usize,
    pub max_scan_records: usize,
    pub max_scan_bytes: usize,
}

impl Default for PackLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 64 * 1024 * 1024 * 1024,
            max_record_bytes: 16 * 1024 * 1024,
            max_directory_bytes: 8 * 1024 * 1024,
            max_records: 3_000_000,
            max_segments: 64,
            max_scan_records: 1024,
            max_scan_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error("pack I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid pack: {0}")]
    Format(&'static str),
    #[error("pack integrity mismatch")]
    Integrity,
    #[error("pack limit exceeded: {0}")]
    Limit(&'static str),
    #[error("pack records must be strictly sorted and unique")]
    Order,
    #[error("immutable generation already exists: {0}")]
    AlreadyExists(String),
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").expect("String write cannot fail");
    }
    out
}
pub(crate) fn key_bytes(key: PackKey) -> [u8; 16] {
    let mut bytes = [0; 16];
    bytes[..2].copy_from_slice(&key.namespace.to_le_bytes());
    bytes[8..].copy_from_slice(&key.id.to_le_bytes());
    bytes
}
pub(crate) fn key(bytes: &[u8]) -> Result<PackKey, PackError> {
    if slice(bytes, 2, 6)?.iter().any(|b| *b != 0) {
        return Err(PackError::Format("key flags"));
    }
    Ok(PackKey {
        namespace: u16_at(bytes, 0)?,
        id: u64_at(bytes, 8)?,
    })
}
pub(crate) fn slice(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], PackError> {
    let end = offset
        .checked_add(len)
        .ok_or(PackError::Format("range overflow"))?;
    bytes
        .get(offset..end)
        .ok_or(PackError::Format("range outside file"))
}
pub(crate) fn usize_at(value: u64) -> Result<usize, PackError> {
    usize::try_from(value).map_err(|_| PackError::Limit("address space"))
}
pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, PackError> {
    Ok(u16::from_le_bytes(
        slice(bytes, offset, 2)?
            .try_into()
            .map_err(|_| PackError::Format("u16"))?,
    ))
}
pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, PackError> {
    Ok(u32::from_le_bytes(
        slice(bytes, offset, 4)?
            .try_into()
            .map_err(|_| PackError::Format("u32"))?,
    ))
}
pub(crate) fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, PackError> {
    Ok(u64::from_le_bytes(
        slice(bytes, offset, 8)?
            .try_into()
            .map_err(|_| PackError::Format("u64"))?,
    ))
}
