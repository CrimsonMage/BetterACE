use crate::pack_format::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Compile already sorted domain records with bounded record/page buffers.
/// The directory must be application-owned; published files MUST remain
/// immutable, including from other processes, for their entire mapped lifetime.
pub fn compile_pack<I>(
    directory: &Path,
    records: I,
    limits: PackLimits,
) -> Result<PackDescriptor, PackError>
where
    I: IntoIterator<Item = Result<PackRecord, PackError>>,
{
    std::fs::create_dir_all(directory)?;
    let mut output = tempfile::NamedTempFile::new_in(directory)?;
    let mut index = tempfile::tempfile_in(directory)?;
    output.write_all(&[0; HEADER])?;
    let mut roots = Vec::new();
    let mut page = Vec::with_capacity(PAGE_RECORDS * ENTRY);
    let mut first = None;
    let mut last = None;
    let mut count = 0_u64;
    let mut payload_end = HEADER as u64;
    let mut index_bytes = 0_u64;
    for input in records {
        let record = input?;
        if last.is_some_and(|key| key >= record.key) {
            return Err(PackError::Order);
        }
        if count >= limits.max_records {
            return Err(PackError::Limit("record count"));
        }
        let payload = record.value.as_deref().unwrap_or_default();
        if payload.len() > limits.max_record_bytes {
            return Err(PackError::Limit("record bytes"));
        }
        payload_end = payload_end
            .checked_add(payload.len() as u64)
            .ok_or(PackError::Limit("file bytes"))?;
        if payload_end > limits.max_file_bytes {
            return Err(PackError::Limit("file bytes"));
        }
        let prospective_records = count + 1;
        let prospective_size = prospective_records
            .checked_mul(ENTRY as u64)
            .and_then(|n| {
                n.checked_add(
                    prospective_records.div_ceil(PAGE_RECORDS as u64) * DIRECTORY_ENTRY as u64,
                )
            })
            .and_then(|n| n.checked_add(payload_end));
        if prospective_size.is_none_or(|n| n > limits.max_file_bytes) {
            return Err(PackError::Limit("file bytes"));
        }
        output.write_all(payload)?;
        let mut entry = [0; ENTRY];
        entry[..16].copy_from_slice(&key_bytes(record.key));
        entry[16..18].copy_from_slice(&u16::from(record.value.is_none()).to_le_bytes());
        entry[18..20].copy_from_slice(&record.schema.to_le_bytes());
        entry[24..32].copy_from_slice(&(payload_end - payload.len() as u64).to_le_bytes());
        entry[32..40].copy_from_slice(&(payload.len() as u64).to_le_bytes());
        entry[40..].copy_from_slice(&digest(payload));
        first.get_or_insert(record.key);
        page.extend_from_slice(&entry);
        last = Some(record.key);
        count += 1;
        if page.len() == PAGE_RECORDS * ENTRY {
            append_page(
                &mut index,
                &mut roots,
                &page,
                first.take().ok_or(PackError::Order)?,
                record.key,
                &mut index_bytes,
                limits,
            )?;
            page.clear();
        }
    }
    if !page.is_empty() {
        append_page(
            &mut index,
            &mut roots,
            &page,
            first.ok_or(PackError::Order)?,
            last.ok_or(PackError::Order)?,
            &mut index_bytes,
            limits,
        )?;
    }
    // Index-page offsets were relative to the temporary index; fix only the
    // bounded root directory. No payload or full record index is kept in RAM.
    for root in roots.chunks_exact_mut(DIRECTORY_ENTRY) {
        let offset = u64_at(root, 32)?
            .checked_add(payload_end)
            .ok_or(PackError::Limit("file bytes"))?;
        root[32..40].copy_from_slice(&offset.to_le_bytes());
    }
    let directory_offset = payload_end
        .checked_add(index_bytes)
        .ok_or(PackError::Limit("file bytes"))?;
    let file_len = directory_offset
        .checked_add(roots.len() as u64)
        .ok_or(PackError::Limit("file bytes"))?;
    if file_len > limits.max_file_bytes {
        return Err(PackError::Limit("file bytes"));
    }
    index.seek(SeekFrom::Start(0))?;
    copy_bounded(&mut index, output.as_file_mut())?;
    output.write_all(&roots)?;
    let mut header = [0; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&1_u16.to_le_bytes());
    header[10..12].copy_from_slice(&(HEADER as u16).to_le_bytes());
    for (offset, value) in [
        (16, file_len),
        (24, count),
        (32, directory_offset),
        (40, roots.len() as u64),
        (48, (roots.len() / DIRECTORY_ENTRY) as u64),
    ] {
        header[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let mut hasher = Sha256::new();
    hasher.update(&header[..96]);
    hasher.update(&roots);
    let generation: [u8; 32] = hasher.finalize().into();
    header[96..].copy_from_slice(&generation);
    output.seek(SeekFrom::Start(0))?;
    output.write_all(&header)?;
    output.as_file().sync_all()?;
    let descriptor = PackDescriptor::new(generation, count);
    persist_new(output, &directory.join(&descriptor.file_name))?;
    Ok(descriptor)
}

fn append_page(
    index: &mut File,
    roots: &mut Vec<u8>,
    page: &[u8],
    first: PackKey,
    last: PackKey,
    index_bytes: &mut u64,
    limits: PackLimits,
) -> Result<(), PackError> {
    if roots
        .len()
        .checked_add(DIRECTORY_ENTRY)
        .is_none_or(|n| n > limits.max_directory_bytes)
    {
        return Err(PackError::Limit("root directory"));
    }
    let mut root = [0; DIRECTORY_ENTRY];
    root[..16].copy_from_slice(&key_bytes(first));
    root[16..32].copy_from_slice(&key_bytes(last));
    root[32..40].copy_from_slice(&index_bytes.to_le_bytes());
    root[40..44].copy_from_slice(&((page.len() / ENTRY) as u32).to_le_bytes());
    root[48..80].copy_from_slice(&digest(page));
    index.write_all(page)?;
    *index_bytes = index_bytes
        .checked_add(page.len() as u64)
        .ok_or(PackError::Limit("index bytes"))?;
    roots.extend_from_slice(&root);
    Ok(())
}

fn copy_bounded(input: &mut File, output: &mut File) -> Result<(), PackError> {
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        output.write_all(&buffer[..count])?;
    }
}

pub(crate) fn persist_new(file: tempfile::NamedTempFile, path: &Path) -> Result<(), PackError> {
    // Never open/truncate an existing generation, even if it has the same hash.
    // TempPath owns only a pathname, so close the writable file handle BEFORE
    // making the inode visible as a published generation.
    let temp = file.into_temp_path();
    let result = match temp.persist_noclobber(path) {
        Ok(()) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_identical(path, error.path.as_ref())
        }
        Err(error) => Err(PackError::Io(error.error)),
    };
    result?;
    // Unix directory fsync makes a newly linked generation durable. Windows
    // keeps file flushing but requires separate platform crash qualification;
    // do not pretend std exposes identical directory durability semantics.
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn verify_identical(existing: &Path, candidate: &Path) -> Result<(), PackError> {
    // Idempotent crash recovery: never rewrite an existing mapped generation.
    // Compare all bytes off-thread using fixed buffers, including payloads
    // which normal startup deliberately does not touch.
    let mut existing = File::open(existing)?;
    let mut candidate = File::open(candidate)?;
    if existing.metadata()?.len() != candidate.metadata()?.len() {
        return Err(PackError::Integrity);
    }
    let mut left = [0; 64 * 1024];
    let mut right = [0; 64 * 1024];
    loop {
        let count = candidate.read(&mut right)?;
        if count == 0 {
            return Ok(());
        }
        existing.read_exact(&mut left[..count])?;
        if left[..count] != right[..count] {
            return Err(PackError::Integrity);
        }
    }
}
