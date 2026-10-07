//! BTree/sector layout ported from the pinned ACE.DatLoader DatDatabaseHeader,
//! DatDirectoryHeader, DatFile and DatReader. Adds bounds and cycle checks.
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const HEADER_OFFSET: u64 = 0x140;
const DIRECTORY_BYTES: usize = 62 * 4 + 4 + 61 * 24;
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
const MAX_DIRECTORIES: usize = 100_000;
const MAX_RECORDS: usize = 5_000_000;

#[derive(Debug, Clone, Copy)]
pub struct DatHeader {
    pub block_size: u32,
    pub file_size: u32,
    pub dataset: u32,
    pub subset: u32,
    pub root: u32,
    pub engine_version: u32,
    pub game_version: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct DatRecord {
    pub id: u32,
    pub offset: u32,
    pub size: u32,
    pub iteration: u32,
}

pub struct DatArchive {
    file: File,
    header: DatHeader,
    records: BTreeMap<u32, DatRecord>,
}

impl DatArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DatError> {
        let mut file = File::open(path)?;
        let physical_size = file.metadata()?.len();
        file.seek(SeekFrom::Start(HEADER_OFFSET))?;
        let mut raw = [0_u8; 80];
        file.read_exact(&mut raw)?;
        if word(&raw, 0)? != 0x5442 {
            return Err(DatError::Format("unsupported DAT magic"));
        }
        let header = DatHeader {
            block_size: word(&raw, 4)?,
            file_size: word(&raw, 8)?,
            dataset: word(&raw, 12)?,
            subset: word(&raw, 16)?,
            root: word(&raw, 32)?,
            engine_version: word(&raw, 52)?,
            game_version: word(&raw, 56)?,
        };
        if header.file_size as u64 != physical_size
            || !(256..=65536).contains(&header.block_size)
            || !header.block_size.is_power_of_two()
            || !(1..=4).contains(&header.dataset)
        {
            return Err(DatError::Format("invalid DAT header dimensions or dataset"));
        }
        let mut archive = Self {
            file,
            header,
            records: BTreeMap::new(),
        };
        let mut stack = vec![header.root];
        let mut visited = BTreeSet::new();
        while let Some(offset) = stack.pop() {
            if !visited.insert(offset) || visited.len() > MAX_DIRECTORIES {
                return Err(DatError::Format("directory cycle or limit"));
            }
            let directory = archive.read_chain(offset, DIRECTORY_BYTES)?;
            let count = word(&directory, 62 * 4)? as usize;
            if count > 61 {
                return Err(DatError::Format("directory entry limit"));
            }
            if word(&directory, 0)? != 0 {
                for branch in 0..=count {
                    stack.push(word(&directory, branch * 4)?);
                }
            }
            for index in 0..count {
                let start = 62 * 4 + 4 + index * 24;
                let record = DatRecord {
                    id: word(&directory, start + 4)?,
                    offset: word(&directory, start + 8)?,
                    size: word(&directory, start + 12)?,
                    iteration: word(&directory, start + 20)?,
                };
                if record.size as usize > MAX_RECORD_BYTES || archive.records.len() >= MAX_RECORDS {
                    return Err(DatError::Format("record allocation limit"));
                }
                if archive.records.insert(record.id, record).is_some() {
                    return Err(DatError::Format("duplicate record ID"));
                }
            }
        }
        Ok(archive)
    }
    pub fn header(&self) -> DatHeader {
        self.header
    }
    pub fn records(&self) -> &BTreeMap<u32, DatRecord> {
        &self.records
    }
    pub fn read(&mut self, id: u32) -> Result<Vec<u8>, DatError> {
        let record = *self.records.get(&id).ok_or(DatError::Missing(id))?;
        self.read_chain(record.offset, record.size as usize)
    }
    fn read_chain(&mut self, mut offset: u32, size: usize) -> Result<Vec<u8>, DatError> {
        if size > MAX_RECORD_BYTES {
            return Err(DatError::Format("record allocation limit"));
        }
        let mut output = Vec::with_capacity(size);
        let mut visited = BTreeSet::new();
        while output.len() < size {
            if offset == 0
                || !offset.is_multiple_of(self.header.block_size)
                || u64::from(offset) + u64::from(self.header.block_size)
                    > u64::from(self.header.file_size)
                || !visited.insert(offset)
            {
                return Err(DatError::Format("invalid or cyclic sector chain"));
            }
            self.file.seek(SeekFrom::Start(u64::from(offset)))?;
            let mut next = [0_u8; 4];
            self.file.read_exact(&mut next)?;
            let count = (size - output.len()).min(self.header.block_size as usize - 4);
            let start = output.len();
            output.resize(start + count, 0);
            self.file.read_exact(&mut output[start..])?;
            offset = u32::from_le_bytes(next);
        }
        if size != 0 && offset != 0 {
            return Err(DatError::Format(
                "sector chain exceeds declared record length",
            ));
        }
        Ok(output)
    }
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, DatError> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(DatError::Format("truncated record"))?
        .try_into()
        .map_err(|_| DatError::Format("truncated record"))?;
    Ok(u32::from_le_bytes(raw))
}

pub fn fingerprint(path: impl AsRef<Path>) -> Result<String, DatError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[derive(Debug, thiserror::Error)]
pub enum DatError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid DAT: {0}")]
    Format(&'static str),
    #[error("DAT record {0:08X} is missing")]
    Missing(u32),
}
