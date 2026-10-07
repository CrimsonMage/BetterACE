use crate::{Reader, WireError, Writer};
/// PacketOutboundConnectRequest payload; transport header is separate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConnectRequest {
    pub server_time: f64,
    pub cookie: u64,
    pub client_id: u32,
    pub server_seed: u32,
    pub client_seed: u32,
}
impl ConnectRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.f64(self.server_time);
        w.u64(self.cookie);
        w.u32(self.client_id);
        w.u32(self.server_seed);
        w.u32(self.client_seed);
        w.u32(0);
        w.into_bytes()
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, WireError> {
        if bytes.len() != 32 {
            return Err(WireError::InvalidLength);
        }
        let mut r = Reader::new(bytes);
        let value = Self {
            server_time: r.f64()?,
            cookie: r.u64()?,
            client_id: r.u32()?,
            server_seed: r.u32()?,
            client_seed: r.u32()?,
        };
        r.u32()?;
        Ok(value)
    }
}
pub fn decode_connect_response(bytes: &[u8]) -> Result<u64, WireError> {
    if bytes.len() != 8 {
        return Err(WireError::InvalidLength);
    }
    Reader::new(bytes).u64()
}
