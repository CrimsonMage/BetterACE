use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAGIC: &[u8; 8] = b"ACERBIN\0";
const ENVELOPE_VERSION: u16 = 1;
const HEADER_LEN: usize = 52;

#[derive(Clone, Copy, Debug)]
pub struct CodecLimits {
    pub max_payload_bytes: usize,
}

impl Default for CodecLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeInfo {
    pub kind: u16,
    pub schema_version: u16,
    pub payload_bytes: usize,
    pub sha256: [u8; 32],
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("binary storage header is truncated or has invalid magic")]
    Header,
    #[error("unsupported storage envelope version {0}")]
    EnvelopeVersion(u16),
    #[error("unsupported envelope flags {0}")]
    Flags(u16),
    #[error("unsupported aggregate kind {actual}; expected {expected}")]
    Kind { expected: u16, actual: u16 },
    #[error("unsupported schema {actual}; expected {expected}; explicit migration required")]
    Schema { expected: u16, actual: u16 },
    #[error("payload exceeds configured byte limit")]
    Limit,
    #[error("declared payload length does not match file length")]
    Length,
    #[error("storage integrity checksum mismatch")]
    Integrity,
    #[error("invalid binary DTO: {0}")]
    Payload(#[from] postcard::Error),
    #[error("binary DTO has trailing bytes")]
    Trailing,
}

/// Header: magic[8], envelope/kind/schema/flags u16 LE, payload length u32 LE,
/// SHA256[32], then postcard bytes. Digest covers the first 20 header bytes and
/// payload. This is corruption detection, not authentication of hostile writers.
pub fn encode<T: Serialize>(
    kind: u16,
    schema: u16,
    value: &T,
    limits: CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    let payload = postcard::serialize_with_flavor(
        value,
        crate::bounded_output::BoundedOutput::new(limits.max_payload_bytes.min(u32::MAX as usize)),
    )
    .map_err(|e| {
        if e == postcard::Error::SerializeBufferFull {
            CodecError::Limit
        } else {
            CodecError::Payload(e)
        }
    })?;
    let length = payload.len();
    let mut out = Vec::with_capacity(HEADER_LEN + length);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ENVELOPE_VERSION.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&schema.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&(length as u32).to_le_bytes());
    let mut digest = Sha256::new();
    digest.update(&out);
    digest.update(&payload);
    out.extend_from_slice(&digest.finalize());
    out.extend_from_slice(&payload);
    Ok(out)
}

pub fn inspect(bytes: &[u8], limits: CodecLimits) -> Result<EnvelopeInfo, CodecError> {
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err(CodecError::Header);
    }
    let word = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    if word(8) != ENVELOPE_VERSION {
        return Err(CodecError::EnvelopeVersion(word(8)));
    }
    if word(14) != 0 {
        return Err(CodecError::Flags(word(14)));
    }
    let length = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as usize;
    if length > limits.max_payload_bytes {
        return Err(CodecError::Limit);
    }
    if bytes.len() - HEADER_LEN != length {
        return Err(CodecError::Length);
    }
    let mut digest = Sha256::new();
    digest.update(&bytes[..20]);
    digest.update(&bytes[HEADER_LEN..]);
    let actual: [u8; 32] = digest.finalize().into();
    if bytes[20..52] != actual {
        return Err(CodecError::Integrity);
    }
    Ok(EnvelopeInfo {
        kind: word(10),
        schema_version: word(12),
        payload_bytes: length,
        sha256: actual,
    })
}

/// Call only with the frozen DTO for this kind/schema. Byte limits apply before
/// deserialization; domain collection/string limits must also be validated before
/// publication. No implicit fallback or migration is attempted.
pub fn decode<T: DeserializeOwned>(
    bytes: &[u8],
    expected_kind: u16,
    expected_schema: u16,
    limits: CodecLimits,
) -> Result<T, CodecError> {
    let info = inspect(bytes, limits)?;
    if info.kind != expected_kind {
        return Err(CodecError::Kind {
            expected: expected_kind,
            actual: info.kind,
        });
    }
    if info.schema_version != expected_schema {
        return Err(CodecError::Schema {
            expected: expected_schema,
            actual: info.schema_version,
        });
    }
    let (value, remaining) = postcard::take_from_bytes(&bytes[HEADER_LEN..])?;
    if !remaining.is_empty() {
        return Err(CodecError::Trailing);
    }
    Ok(value)
}
