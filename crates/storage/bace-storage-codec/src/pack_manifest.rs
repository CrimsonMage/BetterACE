use crate::pack_format::{digest, hex, slice, u16_at, u32_at, u64_at};
use crate::{MappedPack, PackDescriptor, PackError, PackGeneration, PackLimits};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackManifest {
    pub version: u16,
    pub generation: u64,
    pub base: PackDescriptor,
    /// Oldest to newest; replacements/tombstones are resolved newest first.
    pub deltas: Vec<PackDescriptor>,
}

impl PackManifest {
    pub fn encode(&self, limits: PackLimits) -> Result<Vec<u8>, PackError> {
        if self.version != 1 {
            return Err(PackError::Format("manifest version"));
        }
        let count = self
            .deltas
            .len()
            .checked_add(1)
            .ok_or(PackError::Limit("segments"))?;
        if count > limits.max_segments || count > u32::MAX as usize {
            return Err(PackError::Limit("segments"));
        }
        let capacity = count
            .checked_mul(40)
            .and_then(|n| n.checked_add(56))
            .ok_or(PackError::Limit("manifest bytes"))?;
        let mut out = Vec::with_capacity(capacity);
        out.extend_from_slice(b"BACEGEN\0");
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.extend_from_slice(&self.generation.to_le_bytes());
        for descriptor in std::iter::once(&self.base).chain(&self.deltas) {
            if descriptor.file_name
                != PackDescriptor::new(descriptor.generation, descriptor.record_count).file_name
                || descriptor.record_count > limits.max_records
            {
                return Err(PackError::Format("manifest descriptor"));
            }
            out.extend_from_slice(&descriptor.generation);
            out.extend_from_slice(&descriptor.record_count.to_le_bytes());
        }
        out.extend_from_slice(&digest(&out));
        Ok(out)
    }

    pub fn decode(bytes: &[u8], limits: PackLimits) -> Result<Self, PackError> {
        if slice(bytes, 0, 8)? != b"BACEGEN\0" || u16_at(bytes, 8)? != 1 || u16_at(bytes, 10)? != 0
        {
            return Err(PackError::Format("manifest header"));
        }
        let count = u32_at(bytes, 12)? as usize;
        if count == 0 || count > limits.max_segments {
            return Err(PackError::Limit("segments"));
        }
        let prefix_len = count
            .checked_mul(40)
            .and_then(|n| n.checked_add(24))
            .ok_or(PackError::Limit("manifest bytes"))?;
        if prefix_len.checked_add(32) != Some(bytes.len()) {
            return Err(PackError::Format("manifest length"));
        }
        if digest(&bytes[..prefix_len]) != bytes[prefix_len..] {
            return Err(PackError::Integrity);
        }
        let mut descriptors = Vec::with_capacity(count);
        for entry in bytes[24..prefix_len].chunks_exact(40) {
            let hash = entry[..32]
                .try_into()
                .map_err(|_| PackError::Format("manifest digest"))?;
            let records = u64_at(entry, 32)?;
            if records > limits.max_records {
                return Err(PackError::Limit("manifest records"));
            }
            descriptors.push(PackDescriptor::new(hash, records));
        }
        let base = descriptors.remove(0);
        Ok(Self {
            version: 1,
            generation: u64_at(bytes, 16)?,
            base,
            deltas: descriptors,
        })
    }

    pub fn content_hash(&self, limits: PackLimits) -> Result<[u8; 32], PackError> {
        let encoded = self.encode(limits)?;
        Ok(digest(&encoded))
    }

    pub fn open(&self, directory: &Path, limits: PackLimits) -> Result<PackGeneration, PackError> {
        // Validate the caller-constructed manifest before touching files.
        self.encode(limits)?;
        let base = MappedPack::open(directory, &self.base, limits)?;
        let deltas = self
            .deltas
            .iter()
            .map(|descriptor| MappedPack::open(directory, descriptor, limits))
            .collect::<Result<Vec<_>, _>>()?;
        PackGeneration::new(self.generation, base, deltas, limits)
    }
}

/// Write a uniquely named immutable manifest, without replacing any accepted
/// manifest pointer. The application must durably journal acceptance separately.
pub fn write_manifest(
    directory: &Path,
    manifest: &PackManifest,
    limits: PackLimits,
) -> Result<PathBuf, PackError> {
    std::fs::create_dir_all(directory)?;
    let bytes = manifest.encode(limits)?;
    let path = directory.join(format!("{}.manifest", hex(&digest(&bytes))));
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    crate::pack_writer::persist_new(file, &path)?;
    Ok(path)
}

pub fn load_manifest(path: &Path, limits: PackLimits) -> Result<PackManifest, PackError> {
    let max = limits
        .max_segments
        .checked_mul(40)
        .and_then(|n| n.checked_add(56))
        .ok_or(PackError::Limit("manifest bytes"))?;
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > max as u64 {
        return Err(PackError::Limit("manifest bytes"));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(PackError::Limit("manifest bytes"));
    }
    PackManifest::decode(&bytes, limits)
}
