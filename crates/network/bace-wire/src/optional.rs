use crate::{Reader, WireError, Writer, flags};
/// Transport-only optional headers. Handshake/referral bodies use their own
/// codecs and are deliberately rejected here instead of being misinterpreted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OptionalHeaders {
    pub server_switch: Option<[u8; 8]>,
    pub retransmit: Option<Vec<u32>>,
    pub reject_retransmit: Option<Vec<u32>>,
    pub ack: Option<u32>,
    pub command: Option<[u8; 8]>,
    pub time_sync: Option<f64>,
    pub echo_request: Option<f32>,
    pub echo_response: Option<(f32, f32)>,
    pub flow: Option<(u32, u16)>,
}
impl OptionalHeaders {
    pub fn decode(flags_value: u32, reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let allowed = flags::RETRANSMISSION
            | flags::ENCRYPTED_CHECKSUM
            | flags::BLOB_FRAGMENTS
            | flags::SERVER_SWITCH
            | flags::REQUEST_RETRANSMIT
            | flags::REJECT_RETRANSMIT
            | flags::ACK_SEQUENCE
            | flags::DISCONNECT
            | flags::NET_ERROR_DISCONNECT
            | flags::CICMD_COMMAND
            | flags::TIME_SYNC
            | flags::ECHO_REQUEST
            | flags::ECHO_RESPONSE
            | flags::FLOW;
        if flags_value & !allowed != 0 {
            return Err(WireError::UnsupportedFlags(flags_value & !allowed));
        }
        let mut result = Self::default();
        if flags_value & flags::SERVER_SWITCH != 0 {
            result.server_switch = Some(
                reader
                    .take(8)?
                    .try_into()
                    .map_err(|_| WireError::Truncated)?,
            );
        }
        if flags_value & flags::REQUEST_RETRANSMIT != 0 {
            result.retransmit = Some(read_sequences(reader)?);
        }
        if flags_value & flags::REJECT_RETRANSMIT != 0 {
            result.reject_retransmit = Some(read_sequences(reader)?);
        }
        if flags_value & flags::ACK_SEQUENCE != 0 {
            result.ack = Some(reader.u32()?);
        }
        if flags_value & flags::CICMD_COMMAND != 0 {
            result.command = Some(
                reader
                    .take(8)?
                    .try_into()
                    .map_err(|_| WireError::Truncated)?,
            );
        }
        if flags_value & flags::TIME_SYNC != 0 {
            result.time_sync = Some(reader.f64()?);
        }
        if flags_value & flags::ECHO_REQUEST != 0 {
            result.echo_request = Some(reader.f32()?);
        }
        if flags_value & flags::ECHO_RESPONSE != 0 {
            result.echo_response = Some((reader.f32()?, reader.f32()?));
        }
        if flags_value & flags::FLOW != 0 {
            result.flow = Some((reader.u32()?, reader.u16()?));
        }
        Ok(result)
    }
    pub fn encode(&self) -> Result<(u32, Vec<u8>), WireError> {
        let mut bits = 0;
        let mut w = Writer::new();
        if let Some(value) = self.server_switch {
            bits |= flags::SERVER_SWITCH;
            w.bytes(&value);
        }
        for (value, flag) in [
            (&self.retransmit, flags::REQUEST_RETRANSMIT),
            (&self.reject_retransmit, flags::REJECT_RETRANSMIT),
        ] {
            if let Some(value) = value {
                if value.len() > 115 {
                    return Err(WireError::LimitExceeded);
                }
                bits |= flag;
                w.u32(value.len() as u32);
                for sequence in value {
                    w.u32(*sequence);
                }
            }
        }
        if let Some(value) = self.ack {
            bits |= flags::ACK_SEQUENCE;
            w.u32(value);
        }
        if let Some(value) = self.command {
            bits |= flags::CICMD_COMMAND;
            w.bytes(&value);
        }
        if let Some(value) = self.time_sync {
            bits |= flags::TIME_SYNC;
            w.f64(value);
        }
        if let Some(value) = self.echo_request {
            bits |= flags::ECHO_REQUEST;
            w.f32(value);
        }
        if let Some((client_time, delta)) = self.echo_response {
            bits |= flags::ECHO_RESPONSE;
            w.f32(client_time);
            w.f32(delta);
        }
        if let Some((bytes, interval)) = self.flow {
            bits |= flags::FLOW;
            w.u32(bytes);
            w.u16(interval);
        }
        Ok((bits, w.into_bytes()))
    }
}
fn read_sequences(reader: &mut Reader<'_>) -> Result<Vec<u32>, WireError> {
    let count = usize::try_from(reader.u32()?).map_err(|_| WireError::LimitExceeded)?;
    // Bound by actual datagram bytes before allocating, rather than trusting count.
    if count > reader.remaining() / 4 {
        return Err(WireError::Truncated);
    }
    if count > 256 {
        return Err(WireError::LimitExceeded);
    }
    (0..count).map(|_| reader.u32()).collect()
}
