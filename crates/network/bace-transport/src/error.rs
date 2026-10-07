#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Wire(bace_wire::WireError),
    Capacity,
    InvalidFragment,
    ConflictingFragment,
    SequenceWindow,
    SequenceExhausted,
    Checksum,
    InvalidClock,
    InvalidConfiguration,
    WrongPeer,
    ExpiredMessage,
    RetransmitUnavailable,
    Closed,
}
impl From<bace_wire::WireError> for TransportError {
    fn from(value: bace_wire::WireError) -> Self {
        Self::Wire(value)
    }
}
impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "transport error: {self:?}")
    }
}
impl std::error::Error for TransportError {}
