use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    Truncated,
    InvalidLength,
    InvalidFragment,
    LimitExceeded,
    InvalidEncoding,
    ChecksumMismatch,
    UnsupportedFlags(u32),
    UnexpectedOpcode(u32),
}
impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wire error: {self:?}")
    }
}
impl std::error::Error for WireError {}
