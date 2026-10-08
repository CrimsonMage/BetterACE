use crate::{DatArchive, DatError};

/// Resource ceilings for the pinned DAT table layouts; independent of gameplay
/// rank limits. Limits apply before allocation or iteration over untrusted counts.
#[derive(Clone, Copy, Debug)]
pub struct DatTableLimits {
    pub max_record_bytes: usize,
    pub max_entries: usize,
    pub max_string_bytes: usize,
}
impl Default for DatTableLimits {
    fn default() -> Self {
        Self {
            max_record_bytes: 16 * 1024 * 1024,
            max_entries: 65_536,
            max_string_bytes: 1024 * 1024,
        }
    }
}
/// Expected values from a fingerprint-approved asset manifest. Table records
/// contain no independent layout-version field; never invent one from their ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatTableVersion {
    pub engine_version: u32,
    pub game_version: u32,
    pub record_iteration: u32,
}
pub(crate) fn load_record(
    archive: &mut DatArchive,
    id: u32,
    version: DatTableVersion,
    limits: DatTableLimits,
) -> Result<Vec<u8>, DatError> {
    let header = archive.header();
    if header.dataset != 1
        || header.engine_version != version.engine_version
        || header.game_version != version.game_version
    {
        return Err(DatError::Format(
            "unsupported portal DAT version or dataset",
        ));
    }
    let record = archive.records().get(&id).ok_or(DatError::Missing(id))?;
    if record.iteration != version.record_iteration {
        return Err(DatError::Format("unsupported table iteration"));
    }
    if record.size as usize > limits.max_record_bytes {
        return Err(DatError::Format("DAT table size limit"));
    }
    archive.read(id)
}

pub(crate) struct TableReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    pub limits: DatTableLimits,
    entries_left: usize,
}
impl<'a> TableReader<'a> {
    pub fn new(bytes: &'a [u8], id: u32, limits: DatTableLimits) -> Result<Self, DatError> {
        if bytes.len() > limits.max_record_bytes {
            return Err(DatError::Format("DAT table size limit"));
        }
        let mut reader = Self {
            bytes,
            offset: 0,
            limits,
            entries_left: limits.max_entries,
        };
        if reader.u32()? != id {
            return Err(DatError::Format("wrong DAT table record ID"));
        }
        Ok(reader)
    }
    pub(crate) fn raw(bytes: &'a [u8], limits: DatTableLimits) -> Result<Self, DatError> {
        if bytes.len() > limits.max_record_bytes {
            return Err(DatError::Format("DAT record size limit"));
        }
        Ok(Self {
            bytes,
            offset: 0,
            limits,
            entries_left: limits.max_entries,
        })
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], DatError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(DatError::Format("DAT table offset overflow"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(DatError::Format("truncated DAT table"))?;
        self.offset = end;
        Ok(value)
    }
    pub fn u8(&mut self) -> Result<u8, DatError> {
        Ok(self.take(1)?[0])
    }
    pub fn align(&mut self) -> Result<(), DatError> {
        self.take((4 - self.offset % 4) % 4)?;
        Ok(())
    }
    pub fn f32(&mut self) -> Result<f32, DatError> {
        let value = f32::from_bits(self.u32()?);
        if !value.is_finite() {
            return Err(DatError::Format("nonfinite DAT table value"));
        }
        Ok(value)
    }
    pub fn compressed_u32(&mut self) -> Result<u32, DatError> {
        let first = self.u8()?;
        if first & 0x80 == 0 {
            return Ok(u32::from(first));
        }
        let second = u32::from(self.u8()?);
        if first & 0x40 == 0 {
            return Ok((u32::from(first & 0x7f) << 8) | second);
        }
        Ok((u32::from(first & 0x3f) << 24) | (second << 16) | u32::from(self.u16()?))
    }
    pub fn known_id(&mut self, kind: u32) -> Result<u32, DatError> {
        let value = self.u16()?;
        let index = if value & 0x8000 == 0 {
            u32::from(value)
        } else {
            (u32::from(value & 0x3fff) << 16) | u32::from(self.u16()?)
        };
        kind.checked_add(index)
            .ok_or(DatError::Format("DAT resource ID overflow"))
    }
    pub fn reserve_entries(&mut self, count: usize) -> Result<(), DatError> {
        self.entries_left = self
            .entries_left
            .checked_sub(count)
            .ok_or(DatError::Format("DAT nested entry limit"))?;
        Ok(())
    }
    pub fn smart_count(&mut self, minimum_size: usize) -> Result<usize, DatError> {
        let count = self.compressed_u32()? as usize;
        self.count(count, minimum_size)?;
        self.reserve_entries(count)?;
        Ok(count)
    }
    pub fn array<T>(
        &mut self,
        minimum_size: usize,
        mut read: impl FnMut(&mut Self) -> Result<T, DatError>,
    ) -> Result<Vec<T>, DatError> {
        let count = self.smart_count(minimum_size)?;
        (0..count).map(|_| read(self)).collect()
    }
    pub fn dotnet_string(&mut self) -> Result<String, DatError> {
        let mut length = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.u8()?;
            if shift == 28 && byte > 7 {
                return Err(DatError::Format("invalid DAT string length"));
            }
            length |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                if length as usize > self.limits.max_string_bytes {
                    return Err(DatError::Format("DAT string limit"));
                }
                return Ok(String::from_utf8_lossy(self.take(length as usize)?).into_owned());
            }
        }
        Err(DatError::Format("invalid DAT string length"))
    }
    pub fn u16(&mut self) -> Result<u16, DatError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| DatError::Format("truncated u16"))?,
        ))
    }
    pub fn u32(&mut self) -> Result<u32, DatError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| DatError::Format("truncated u32"))?,
        ))
    }
    pub fn u64(&mut self) -> Result<u64, DatError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| DatError::Format("truncated u64"))?,
        ))
    }
    pub fn f64(&mut self) -> Result<f64, DatError> {
        let value = f64::from_bits(self.u64()?);
        if !value.is_finite() {
            return Err(DatError::Format("nonfinite DAT table value"));
        }
        Ok(value)
    }
    pub fn count(&self, count: usize, minimum_size: usize) -> Result<usize, DatError> {
        if count > self.limits.max_entries || count > self.remaining() / minimum_size {
            return Err(DatError::Format("DAT table count limit or truncated data"));
        }
        Ok(count)
    }
    pub fn pstring(&mut self) -> Result<String, DatError> {
        let length = usize::from(self.u16()?);
        if length > self.limits.max_string_bytes {
            return Err(DatError::Format("DAT string limit"));
        }
        // Pinned .NET Encoding.Default is UTF-8 with replacement fallback.
        let value = String::from_utf8_lossy(self.take(length)?).into_owned();
        self.take((4 - self.offset % 4) % 4)?;
        Ok(value)
    }
    pub fn finish(self) -> Result<(), DatError> {
        if self.remaining() != 0 {
            return Err(DatError::Format("trailing DAT table bytes"));
        }
        Ok(())
    }
}
