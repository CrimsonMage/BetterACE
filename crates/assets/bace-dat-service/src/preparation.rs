use crate::{DddError, DddLimits, RecordMetadata};
use bace_dat::DatArchive;
use bace_wire::{DddData, DddDatabase};
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

/// Prepared immutable record. Construction is restricted to bounded asset work.
/// Compression uses zlib format; compressed byte identity with .NET is not claimed.
pub struct PreparedRecord {
    metadata: RecordMetadata,
    bytes: Vec<u8>,
}
impl PreparedRecord {
    pub fn metadata(&self) -> RecordMetadata {
        self.metadata
    }
    pub fn payload(&self) -> &[u8] {
        &self.bytes
    }
    pub fn encode(&self, limits: DddLimits) -> Result<Vec<u8>, DddError> {
        Ok(DddData {
            database: self.metadata.database,
            resource_type: self.metadata.resource_type,
            object_id: self.metadata.object_id,
            iteration: self.metadata.iteration,
            compressed: self.metadata.compressed,
            data: &self.bytes,
        }
        .encode(limits.max_record_bytes)?)
    }
}
/// CPU-only preparation, bounded before compression. Exact ACE policy chooses
/// zlib only when compressed length + four-byte original length is smaller.
pub fn prepare_record(
    database: DddDatabase,
    object_id: u32,
    resource_type: u32,
    iteration: u32,
    raw: &[u8],
    limits: DddLimits,
) -> Result<PreparedRecord, DddError> {
    limits.validate()?;
    if raw.len() > limits.max_record_bytes {
        return Err(DddError::Capacity);
    }
    // A fixed-size output buffer prevents compressor output from growing past
    // the raw size. If incompressible, retain raw bytes without a second growth.
    let mut compressed = vec![0u8; raw.len()];
    let mut target = std::io::Cursor::new(compressed.as_mut_slice());
    let mut encoder = ZlibEncoder::new(&mut target, Compression::best());
    let outcome = encoder
        .write_all(raw)
        .and_then(|_| encoder.finish().map(|_| ()));
    let size = target.position() as usize;
    let bytes = match outcome {
        Ok(()) if size.saturating_add(4) < raw.len() => {
            let mut bytes = Vec::with_capacity(size + 4);
            bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&compressed[..size]);
            bytes
        }
        Ok(()) => raw.to_vec(),
        Err(error) if error.kind() == std::io::ErrorKind::WriteZero => raw.to_vec(),
        Err(error) => return Err(DddError::Compression(error)),
    };
    Ok(PreparedRecord {
        metadata: RecordMetadata {
            database,
            object_id,
            resource_type,
            iteration,
            raw_size: raw.len() as u32,
            transfer_size: bytes.len() as u32,
            compressed: bytes.len() < raw.len(),
        },
        bytes,
    })
}
/// Blocking asset worker entry point. Resource type comes from validated asset
/// metadata; it is not copied from an untrusted client request.
pub fn prepare_archive_record(
    archive: &mut DatArchive,
    database: DddDatabase,
    object_id: u32,
    resource_type: u32,
    limits: DddLimits,
) -> Result<PreparedRecord, DddError> {
    let record = *archive
        .records()
        .get(&object_id)
        .ok_or(DddError::NotFound)?;
    if record.size as usize > limits.max_record_bytes {
        return Err(DddError::Capacity);
    }
    let bytes = archive.read(object_id)?;
    prepare_record(
        database,
        object_id,
        resource_type,
        record.iteration,
        &bytes,
        limits,
    )
}

/// Cell DAT files are sent on demand. Their startup catalog uses exact raw
/// lengths without scanning 800,000+ payloads; this matching worker entry
/// point deliberately sends them uncompressed.
pub fn prepare_archive_record_uncompressed(
    archive: &mut DatArchive,
    database: DddDatabase,
    object_id: u32,
    resource_type: u32,
    limits: DddLimits,
) -> Result<PreparedRecord, DddError> {
    limits.validate()?;
    let record = *archive
        .records()
        .get(&object_id)
        .ok_or(DddError::NotFound)?;
    if record.size as usize > limits.max_record_bytes {
        return Err(DddError::Capacity);
    }
    let bytes = archive.read(object_id)?;
    Ok(PreparedRecord {
        metadata: RecordMetadata {
            database,
            object_id,
            resource_type,
            iteration: record.iteration,
            raw_size: bytes.len() as u32,
            transfer_size: bytes.len() as u32,
            compressed: false,
        },
        bytes,
    })
}
