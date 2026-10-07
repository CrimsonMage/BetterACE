//! Weenie pack domain contract: namespace 1, schema 1, payload = the existing
//! kind-1/schema-1 frozen WeenieV1 envelope. No gameplay structs are serialized.
use bace_storage_codec::{PackError, PackKey, PackLimits, PackManifest, PackRecord};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct PackBuild {
    pub file: PathBuf,
    pub manifest: PathBuf,
    pub records: u64,
}

/// Decode one source at a time into a bounded disk spool. The only batch-wide
/// resident index contains (ID, offset, length), not decoded templates/payloads.
/// Sources are consumed once, so changing source files cannot alter the spool.
/// The caller owns an unpublished output directory and cleanup on failure.
pub fn build_weenie_pack(
    sources: &[PathBuf],
    directory: &Path,
    cancel: &AtomicBool,
) -> Result<PackBuild, String> {
    build(sources, directory, cancel).map_err(|e| e.to_string())
}

fn build(
    sources: &[PathBuf],
    directory: &Path,
    cancel: &AtomicBool,
) -> Result<PackBuild, Box<dyn std::error::Error>> {
    if sources.is_empty() || sources.len() > 100_000 {
        return Err("Select between 1 and 100,000 native TOML sources.".into());
    }
    let limits = PackLimits {
        max_file_bytes: 8 * 1024 * 1024 * 1024,
        max_records: 100_000,
        ..Default::default()
    };
    let mut spool = tempfile::tempfile_in(directory)?;
    let mut index = Vec::with_capacity(sources.len());
    let mut offset = 0_u64;
    for source in sources {
        if cancel.load(Ordering::Relaxed) {
            return Err("Pack build cancelled.".into());
        }
        if !source.is_file() {
            return Err(format!("Not a regular source file: {}", source.display()).into());
        }
        let mut text = String::new();
        File::open(source)?
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        let template = crate::parse(&text).map_err(|e| format!("{}: {e}", source.display()))?;
        let bytes = crate::compile_template(&template)?;
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or("Spool overflow.")?;
        if end > limits.max_file_bytes {
            return Err("Pack payloads exceed the 8 GiB build limit.".into());
        }
        spool.write_all(&bytes)?;
        index.push((template.weenie_id, offset, bytes.len()));
        offset = end;
    }
    index.sort_unstable_by_key(|record| record.0);
    if index.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("Duplicate weenie ID in selected sources; no pack built.".into());
    }
    let records = index.into_iter().map(|(id, offset, length)| {
        if cancel.load(Ordering::Relaxed) {
            return Err(PackError::Format("build cancelled"));
        }
        spool.seek(SeekFrom::Start(offset))?;
        let mut bytes = vec![0; length];
        spool.read_exact(&mut bytes)?;
        Ok(PackRecord {
            key: PackKey {
                namespace: 1,
                id: u64::from(id),
            },
            schema: 1,
            value: Some(bytes),
        })
    });
    let descriptor = bace_storage_codec::compile_pack(directory, records, limits)?;
    let manifest = PackManifest {
        version: 1,
        generation: 1,
        base: descriptor.clone(),
        deltas: Vec::new(),
    };
    let manifest = bace_storage_codec::write_manifest(directory, &manifest, limits)?;
    Ok(PackBuild {
        file: directory.join(descriptor.file_name),
        manifest,
        records: descriptor.record_count,
    })
}
