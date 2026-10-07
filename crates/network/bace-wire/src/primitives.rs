use crate::WireError;

pub struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn take(&mut self, len: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(WireError::InvalidLength)?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or(WireError::Truncated)?;
        self.position = end;
        Ok(slice)
    }
    pub fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| WireError::Truncated)?,
        ))
    }
    pub fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| WireError::Truncated)?,
        ))
    }
    pub fn u64(&mut self) -> Result<u64, WireError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| WireError::Truncated)?,
        ))
    }
    pub fn f32(&mut self) -> Result<f32, WireError> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn f64(&mut self) -> Result<f64, WireError> {
        Ok(f64::from_bits(self.u64()?))
    }
    pub fn packed_u32(&mut self) -> Result<u32, WireError> {
        let high = self.u16()?;
        if high & 0x8000 == 0 {
            Ok(high.into())
        } else {
            Ok(((u32::from(high) & 0x7fff) << 16) | u32::from(self.u16()?))
        }
    }
    /// ACE BinaryReaderExtensions.ReadString16L uses UTF-8 bytes with a UTF-16
    /// code-unit length. This is distinct from server-side CP1252 String16L.
    pub fn client_string16(&mut self, max_units: usize) -> Result<String, WireError> {
        let units = usize::from(self.u16()?);
        let value = read_utf8_units(self, units, max_units)?;
        self.take((4 - (2 + units) % 4) % 4)?;
        Ok(value)
    }
    /// Decoder for the server's Windows-1252 string encoding, not the login UTF-8 reader.
    pub fn string16(&mut self, max_bytes: usize) -> Result<String, WireError> {
        let len = usize::from(self.u16()?);
        if len > max_bytes {
            return Err(WireError::LimitExceeded);
        }
        let bytes = self.take(len)?;
        let (value, _, errors) = encoding_rs::WINDOWS_1252.decode(bytes);
        if errors {
            return Err(WireError::InvalidEncoding);
        }
        let value = value.into_owned();
        self.take((4 - (len + 2) % 4) % 4)?;
        Ok(value)
    }
}

#[derive(Default)]
pub struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn position(&self) -> usize {
        self.bytes.len()
    }
    pub fn align4(&mut self) {
        self.bytes
            .resize(self.bytes.len() + (4 - self.bytes.len() % 4) % 4, 0);
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    pub fn u16(&mut self, value: u16) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }
    pub fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    pub fn packed_u32(&mut self, value: u32) -> Result<(), WireError> {
        // This format has only 31 data bits; reject lossy representations.
        if value > 0x7fff_ffff {
            return Err(WireError::InvalidLength);
        }
        if value <= 32767 {
            self.u16(value as u16);
        } else {
            self.u32(value.rotate_right(16) | 0x8000);
        }
        Ok(())
    }
    pub fn string16(&mut self, value: &str) -> Result<(), WireError> {
        let (bytes, _, errors) = encoding_rs::WINDOWS_1252.encode(value);
        if errors {
            return Err(WireError::InvalidEncoding);
        }
        let len = u16::try_from(bytes.len()).map_err(|_| WireError::InvalidLength)?;
        self.u16(len);
        self.bytes(&bytes);
        self.bytes
            .resize(self.bytes.len() + (4 - (usize::from(len) + 2) % 4) % 4, 0);
        Ok(())
    }
}

pub(crate) fn read_utf8_units(
    reader: &mut Reader<'_>,
    required: usize,
    max_units: usize,
) -> Result<String, WireError> {
    if required > max_units {
        return Err(WireError::LimitExceeded);
    }
    let mut result = String::new();
    let mut units = 0;
    while units < required {
        let first = reader.take(1)?[0];
        let width = match first {
            0..=0x7f => 1,
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Err(WireError::InvalidEncoding),
        };
        let mut bytes = [0; 4];
        bytes[0] = first;
        bytes[1..width].copy_from_slice(reader.take(width - 1)?);
        let value = std::str::from_utf8(&bytes[..width]).map_err(|_| WireError::InvalidEncoding)?;
        units += value.encode_utf16().count();
        if units > required {
            return Err(WireError::InvalidEncoding);
        }
        result.push_str(value);
    }
    Ok(result)
}
