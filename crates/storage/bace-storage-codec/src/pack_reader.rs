use crate::pack_format::*;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct MappedPack {
    map: memmap2::Mmap,
    descriptor: PackDescriptor,
    directory_offset: usize,
    pages: usize,
    index_start: usize,
    limits: PackLimits,
}

#[derive(Debug, Clone)]
pub struct RecordHandle {
    owner: Arc<MappedPack>,
    key: PackKey,
    schema: u16,
    offset: usize,
    length: usize,
}

impl RecordHandle {
    pub fn key(&self) -> PackKey {
        self.key
    }
    pub fn schema(&self) -> u16 {
        self.schema
    }
    pub fn generation(&self) -> [u8; 32] {
        self.owner.descriptor.generation
    }
    /// Borrow remains tied to this owning handle; no self-referential objects.
    pub fn bytes(&self) -> &[u8] {
        &self.owner.map[self.offset..self.offset + self.length]
    }
}

#[derive(Debug, Clone)]
pub enum PackLookup {
    Missing,
    Tombstone,
    Record(RecordHandle),
}

impl MappedPack {
    /// Open an application-owned immutable generation. The directory MUST NOT
    /// be modified outside the append-only publication protocol: in particular,
    /// no process may modify/truncate existing generation files while mapped.
    /// Only the bounded root directory is read/verified here, never payloads or
    /// record-index pages. mmap faults are I/O; call this and lookup on workers.
    pub fn open(
        directory: &Path,
        descriptor: &PackDescriptor,
        limits: PackLimits,
    ) -> Result<Arc<Self>, PackError> {
        if descriptor.file_name
            != PackDescriptor::new(descriptor.generation, descriptor.record_count).file_name
        {
            return Err(PackError::Format("noncanonical filename"));
        }
        let file = File::open(directory.join(&descriptor.file_name))?;
        let size = file.metadata()?.len();
        if size < HEADER as u64 || size > limits.max_file_bytes {
            return Err(PackError::Limit("file size"));
        }
        usize_at(size)?;
        let map = crate::mapping::open_immutable(&file)?;
        let header = slice(&map, 0, HEADER)?;
        if &header[..8] != MAGIC
            || u16_at(header, 8)? != 1
            || u16_at(header, 10)? as usize != HEADER
            || header[12..16]
                .iter()
                .chain(header[56..96].iter())
                .any(|b| *b != 0)
        {
            return Err(PackError::Format("header magic/version/reserved"));
        }
        if u64_at(header, 16)? != size || u64_at(header, 24)? != descriptor.record_count {
            return Err(PackError::Format("declared file/count mismatch"));
        }
        if descriptor.record_count > limits.max_records {
            return Err(PackError::Limit("record count"));
        }
        let directory_offset = usize_at(u64_at(header, 32)?)?;
        let directory_length = usize_at(u64_at(header, 40)?)?;
        let pages = usize_at(u64_at(header, 48)?)?;
        if directory_length > limits.max_directory_bytes {
            return Err(PackError::Limit("root directory"));
        }
        if pages.checked_mul(DIRECTORY_ENTRY) != Some(directory_length)
            || directory_offset.checked_add(directory_length) != Some(map.len())
            || directory_offset < HEADER
        {
            return Err(PackError::Format("root range"));
        }
        let directory_bytes = slice(&map, directory_offset, directory_length)?;
        let mut hasher = Sha256::new();
        hasher.update(&header[..96]);
        hasher.update(directory_bytes);
        let actual: [u8; 32] = hasher.finalize().into();
        if actual != descriptor.generation || header[96..] != actual {
            return Err(PackError::Integrity);
        }
        let index_start = if pages == 0 {
            HEADER
        } else {
            usize_at(u64_at(directory_bytes, 32)?)?
        };
        if index_start < HEADER {
            return Err(PackError::Format("index starts in header"));
        }
        let mut expected = index_start;
        let mut count = 0_u64;
        let mut previous = None;
        for root in directory_bytes.chunks_exact(DIRECTORY_ENTRY) {
            let first = key(root)?;
            let last = key(&root[16..])?;
            let entries = u32_at(root, 40)? as usize;
            if first > last || previous.is_some_and(|p| p >= first) {
                return Err(PackError::Order);
            }
            if !(1..=PAGE_RECORDS).contains(&entries)
                || u32_at(root, 44)? != 0
                || usize_at(u64_at(root, 32)?)? != expected
            {
                return Err(PackError::Format("index page directory"));
            }
            expected = expected
                .checked_add(entries * ENTRY)
                .ok_or(PackError::Format("index overflow"))?;
            count += entries as u64;
            previous = Some(last);
        }
        if expected != directory_offset || count != descriptor.record_count {
            return Err(PackError::Format("index count/end mismatch"));
        }
        Ok(Arc::new(Self {
            map,
            descriptor: descriptor.clone(),
            directory_offset,
            pages,
            index_start,
            limits,
        }))
    }

    pub fn descriptor(&self) -> &PackDescriptor {
        &self.descriptor
    }

    pub fn lookup(self: &Arc<Self>, wanted: PackKey) -> Result<PackLookup, PackError> {
        let page_index = self.lower_page(wanted)?;
        if page_index == self.pages {
            return Ok(PackLookup::Missing);
        }
        let page = self.checked_page(page_index)?;
        for entry in page.chunks_exact(ENTRY) {
            let found = key(entry)?;
            if found == wanted {
                return self.handle(entry);
            }
            if found > wanted {
                break;
            }
        }
        Ok(PackLookup::Missing)
    }

    /// Bounded physical-record iteration, including tombstones. The returned
    /// final key is the exclusive cursor for the next call. No eager iterator
    /// spanning an unbounded world is exposed.
    pub fn scan(
        self: &Arc<Self>,
        after: Option<PackKey>,
        limit: usize,
    ) -> Result<Vec<(PackKey, PackLookup)>, PackError> {
        if limit == 0 || limit > self.limits.max_scan_records {
            return Err(PackError::Limit("scan records"));
        }
        let mut records = Vec::with_capacity(limit);
        let mut payload_bytes = 0_usize;
        let start = match after {
            Some(key) => self.lower_page(key)?,
            None => 0,
        };
        for page_index in start..self.pages {
            let page = self.checked_page(page_index)?;
            for entry in page.chunks_exact(ENTRY) {
                let found = key(entry)?;
                if after.is_some_and(|key| found <= key) {
                    continue;
                }
                let length = usize_at(u64_at(entry, 32)?)?;
                if payload_bytes
                    .checked_add(length)
                    .is_none_or(|n| n > self.limits.max_scan_bytes)
                {
                    if records.is_empty() {
                        return Err(PackError::Limit("scan payload bytes"));
                    }
                    return Ok(records);
                }
                payload_bytes += length;
                records.push((found, self.handle(entry)?));
                if records.len() == limit {
                    return Ok(records);
                }
            }
        }
        Ok(records)
    }

    fn root(&self, index: usize) -> Result<&[u8], PackError> {
        if index >= self.pages {
            return Err(PackError::Format("page index"));
        }
        slice(
            &self.map,
            self.directory_offset + index * DIRECTORY_ENTRY,
            DIRECTORY_ENTRY,
        )
    }
    fn lower_page(&self, wanted: PackKey) -> Result<usize, PackError> {
        let (mut low, mut high) = (0, self.pages);
        while low < high {
            let mid = low + (high - low) / 2;
            if key(&self.root(mid)?[16..])? < wanted {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }
    fn checked_page(&self, index: usize) -> Result<&[u8], PackError> {
        let root = self.root(index)?;
        let count = u32_at(root, 40)? as usize;
        let page = slice(&self.map, usize_at(u64_at(root, 32)?)?, count * ENTRY)?;
        if digest(page) != root[48..80] {
            return Err(PackError::Integrity);
        }
        let mut previous = None;
        for entry in page.chunks_exact(ENTRY) {
            let found = key(entry)?;
            if previous.is_some_and(|p| p >= found) {
                return Err(PackError::Order);
            }
            let flags = u16_at(entry, 16)?;
            if flags > 1 || u32_at(entry, 20)? != 0 {
                return Err(PackError::Format("record flags"));
            }
            let offset = usize_at(u64_at(entry, 24)?)?;
            let length = usize_at(u64_at(entry, 32)?)?;
            if length > self.limits.max_record_bytes {
                return Err(PackError::Limit("record bytes"));
            }
            if offset < HEADER
                || offset
                    .checked_add(length)
                    .is_none_or(|end| end > self.index_start)
                || (flags == 1 && length != 0)
            {
                return Err(PackError::Format("payload range"));
            }
            previous = Some(found);
        }
        if key(page)? != key(root)? || previous != Some(key(&root[16..])?) {
            return Err(PackError::Order);
        }
        Ok(page)
    }
    fn handle(self: &Arc<Self>, entry: &[u8]) -> Result<PackLookup, PackError> {
        let offset = usize_at(u64_at(entry, 24)?)?;
        let length = usize_at(u64_at(entry, 32)?)?;
        if digest(slice(&self.map, offset, length)?) != entry[40..72] {
            return Err(PackError::Integrity);
        }
        if u16_at(entry, 16)? == 1 {
            return Ok(PackLookup::Tombstone);
        }
        Ok(PackLookup::Record(RecordHandle {
            owner: Arc::clone(self),
            key: key(entry)?,
            schema: u16_at(entry, 18)?,
            offset,
            length,
        }))
    }
}
