use crate::{MappedPack, PackError, PackKey, PackLimits, PackLookup};
use std::sync::Arc;

/// Immutable overlay. Deltas are ordered oldest to newest. Atomic installation
/// and durable publication order belong to the application, not this codec.
pub struct PackGeneration {
    revision: u64,
    base: Arc<MappedPack>,
    deltas: Vec<Arc<MappedPack>>,
}

impl PackGeneration {
    pub fn new(
        revision: u64,
        base: Arc<MappedPack>,
        deltas: Vec<Arc<MappedPack>>,
        limits: PackLimits,
    ) -> Result<Self, PackError> {
        if deltas.len()
            >= limits
                .max_segments
                .min(crate::pack_format::MAX_ACTIVE_PACKS)
        {
            return Err(PackError::Limit("generation segments"));
        }
        Ok(Self {
            revision,
            base,
            deltas,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Keys physically written by the newest immutable delta, including
    /// tombstones and derived indexes. Call on a blocking content worker after
    /// durable acceptance; this never opens or decodes the whole base world.
    pub fn newest_delta_keys(&self) -> Result<Vec<PackKey>, PackError> {
        let Some(delta) = self.deltas.last() else {
            return Ok(Vec::new());
        };
        let mut keys = Vec::new();
        let mut after = None;
        loop {
            let page = delta.scan(after, 256)?;
            if page.is_empty() {
                break;
            }
            if keys.len().saturating_add(page.len()) > 4098 {
                return Err(PackError::Limit("publication changed keys"));
            }
            after = page.last().map(|(key, _)| *key);
            keys.extend(page.into_iter().map(|(key, _)| key));
        }
        Ok(keys)
    }
    /// Bounded cold-worker overlay scan. Includes tombstones so every returned
    /// last key is a progressing exclusive cursor, even across deleted ranges.
    /// At most one page-sized batch is retained per immutable active segment.
    pub fn scan(
        &self,
        after: Option<PackKey>,
        limit: usize,
    ) -> Result<Vec<(PackKey, PackLookup)>, PackError> {
        if limit == 0 || limit > 1024 {
            return Err(PackError::Limit("generation scan records"));
        }
        let mut rows = std::collections::BTreeMap::new();
        // A per-segment byte bound can shorten a scan before `limit`; never
        // advance past an unobserved row from that segment.
        let mut last_complete = None;
        for segment in std::iter::once(&self.base).chain(&self.deltas) {
            let batch = segment.scan(after, limit)?;
            if let Some((last, _)) = batch.last() {
                last_complete = Some(last_complete.map_or(*last, |old: PackKey| old.min(*last)));
            }
            for (key, value) in batch {
                rows.insert(key, value);
            }
        }
        let mut bytes = 0usize;
        let mut result = Vec::with_capacity(limit);
        for (key, value) in rows {
            if last_complete.is_some_and(|last| key > last) {
                break;
            }
            if let PackLookup::Record(record) = &value {
                let next = bytes
                    .checked_add(record.bytes().len())
                    .ok_or(PackError::Limit("generation scan bytes"))?;
                if next > 64 * 1024 * 1024 {
                    if result.is_empty() {
                        return Err(PackError::Limit("generation scan bytes"));
                    }
                    break;
                }
                bytes = next;
            }
            result.push((key, value));
            if result.len() == limit {
                break;
            }
        }
        Ok(result)
    }
    pub fn lookup(&self, key: PackKey) -> Result<PackLookup, PackError> {
        for segment in self.deltas.iter().rev().chain(std::iter::once(&self.base)) {
            match segment.lookup(key)? {
                PackLookup::Missing => {}
                found => return Ok(found),
            }
        }
        Ok(PackLookup::Missing)
    }
}

impl PackGeneration {
    /// Streaming worker-only compaction. Keeps at most one record handle per
    /// active segment; newest replacements win and tombstones are discarded.
    /// Writes a new immutable file. The caller owns manifest CAS acceptance and
    /// reclamation after all older readers release their mappings. Cancellation
    /// never changes the active generation or truncates a mapped file.
    pub fn compact(
        &self,
        directory: &std::path::Path,
        limits: PackLimits,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<crate::PackDescriptor, PackError> {
        let segments: Vec<_> = std::iter::once(&self.base).chain(&self.deltas).collect();
        let mut cursors = vec![None; segments.len()];
        let mut heads: Vec<Option<(PackKey, PackLookup)>> = vec![None; segments.len()];
        let mut exhausted = vec![false; segments.len()];
        let mut failed = false;
        let records = std::iter::from_fn(|| {
            loop {
                if failed {
                    return None;
                }
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    failed = true;
                    return Some(Err(PackError::Format("compaction cancelled")));
                }
                for index in 0..segments.len() {
                    if heads[index].is_none() && !exhausted[index] {
                        match segments[index].scan(cursors[index], 1) {
                            Ok(mut rows) => {
                                heads[index] = rows.pop();
                                if heads[index].is_none() {
                                    exhausted[index] = true;
                                }
                            }
                            Err(error) => {
                                failed = true;
                                return Some(Err(error));
                            }
                        }
                    }
                }
                let key = heads.iter().filter_map(|h| h.as_ref().map(|h| h.0)).min()?;
                let mut winner = None;
                for index in 0..heads.len() {
                    if heads[index].as_ref().is_some_and(|h| h.0 == key) {
                        winner = heads[index].take().map(|h| h.1);
                        cursors[index] = Some(key);
                    }
                }
                if let Some(PackLookup::Record(record)) = winner {
                    return Some(Ok(crate::PackRecord {
                        key,
                        schema: record.schema(),
                        value: Some(record.bytes().to_vec()),
                    }));
                }
            }
        });
        crate::compile_pack(directory, records, limits)
    }
}
