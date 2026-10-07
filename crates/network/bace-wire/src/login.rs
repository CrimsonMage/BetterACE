//! Official PacketInboundLoginRequest/BinaryReaderExtensions decoding. The
//! default .NET BinaryReader uses UTF-8 and counts UTF-16 code units here.
use crate::{CLIENT_DATAGRAM_LIMIT, Datagram, Reader, WireError, flags, hash32};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetAuthType {
    Undefined,
    Account,
    AccountPassword,
    GlsTicket,
    Unknown(u32),
}
impl From<u32> for NetAuthType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Undefined,
            1 => Self::Account,
            2 => Self::AccountPassword,
            0x40000002 => Self::GlsTicket,
            other => Self::Unknown(other),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub enum LoginCredential {
    None,
    Password(String),
    GlsTicket(String),
}
impl std::fmt::Debug for LoginCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Password(_) => "Password([redacted])",
            Self::GlsTicket(_) => "GlsTicket([redacted])",
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRequest {
    pub client_version: String,
    /// Upstream reads but does not validate this field. Retained for diagnostics.
    pub declared_length: u32,
    pub net_auth_type: NetAuthType,
    pub auth_flags: u32,
    pub timestamp: u32,
    pub account: String,
    /// Upstream ignores account override. Parsing MUST NOT grant impersonation.
    pub account_to_login_as: String,
    pub credential: LoginCredential,
    pub trailing_bytes: usize,
}
impl LoginRequest {
    pub fn decode_payload(bytes: &[u8]) -> Result<Self, WireError> {
        if bytes.len() > CLIENT_DATAGRAM_LIMIT - 20 {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        let client_version = read_string16(&mut reader)?;
        let declared_length = reader.u32()?;
        let net_auth_type = NetAuthType::from(reader.u32()?);
        let auth_flags = reader.u32()?;
        let timestamp = reader.u32()?;
        let account = read_string16(&mut reader)?;
        let account_to_login_as = read_string16(&mut reader)?;
        let credential = match net_auth_type {
            NetAuthType::AccountPassword => LoginCredential::Password(read_string32(&mut reader)?),
            NetAuthType::GlsTicket => LoginCredential::GlsTicket(read_string32(&mut reader)?),
            _ => LoginCredential::None,
        };
        Ok(Self {
            client_version,
            declared_length,
            net_auth_type,
            auth_flags,
            timestamp,
            account,
            account_to_login_as,
            credential,
            trailing_bytes: reader.remaining(),
        })
    }
    /// Initial clear-checksum LoginRequest only; mixed/control flags are explicit
    /// unsupported combinations, not guessed optional layouts.
    pub fn decode_datagram(bytes: &[u8]) -> Result<Self, WireError> {
        let packet = Datagram::decode(bytes)?;
        if packet.header.flags != flags::LOGIN_REQUEST {
            return Err(WireError::UnsupportedFlags(packet.header.flags));
        }
        if packet.header.checksum != packet.header.hash().wrapping_add(hash32(packet.body)) {
            return Err(WireError::ChecksumMismatch);
        }
        Self::decode_payload(packet.body)
    }
}
fn read_string16(reader: &mut Reader<'_>) -> Result<String, WireError> {
    let units = usize::from(reader.u16()?);
    let value = crate::primitives::read_utf8_units(reader, units, CLIENT_DATAGRAM_LIMIT)?;
    reader.take((4 - (2 + units) % 4) % 4)?;
    Ok(value)
}
fn read_string32(reader: &mut Reader<'_>) -> Result<String, WireError> {
    let mut units = usize::try_from(reader.u32()?).map_err(|_| WireError::LimitExceeded)?;
    if units == 0 {
        return Ok(String::new());
    }
    if units > CLIENT_DATAGRAM_LIMIT {
        return Err(WireError::LimitExceeded);
    }
    reader.take(1)?;
    units -= 1;
    if units > 255 {
        reader.take(1)?;
        units -= 1;
    }
    let value = crate::primitives::read_utf8_units(reader, units, CLIENT_DATAGRAM_LIMIT)?;
    // ACE seeks across final padding even if it lies beyond end-of-stream.
    // Missing terminal padding is therefore valid legacy behavior. Intermediate
    // string16 padding, by contrast, precedes required fields and must exist.
    let padding = (4 - units % 4) % 4;
    reader.take(padding.min(reader.remaining()))?;
    Ok(value)
}
