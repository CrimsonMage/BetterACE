use crate::{Reader, WireError, Writer, hash32};

pub const PACKET_HEADER_SIZE: usize = 20;
pub const FRAGMENT_HEADER_SIZE: usize = 16;
pub const SERVER_BODY_LIMIT: usize = 464;
pub const SERVER_DATAGRAM_LIMIT: usize = 484;
pub const CLIENT_DATAGRAM_LIMIT: usize = 1024;
pub const FRAGMENT_DATA_LIMIT: usize = 448;

pub mod flags {
    pub const RETRANSMISSION: u32 = 0x1;
    pub const ENCRYPTED_CHECKSUM: u32 = 0x2;
    pub const BLOB_FRAGMENTS: u32 = 0x4;
    pub const SERVER_SWITCH: u32 = 0x100;
    pub const LOGON_SERVER_ADDR: u32 = 0x200;
    pub const EMPTY_HEADER_1: u32 = 0x400;
    pub const REFERRAL: u32 = 0x800;
    pub const REQUEST_RETRANSMIT: u32 = 0x1000;
    pub const REJECT_RETRANSMIT: u32 = 0x2000;
    pub const ACK_SEQUENCE: u32 = 0x4000;
    pub const DISCONNECT: u32 = 0x8000;
    pub const LOGIN_REQUEST: u32 = 0x10000;
    pub const WORLD_LOGIN_REQUEST: u32 = 0x20000;
    pub const CONNECT_REQUEST: u32 = 0x40000;
    pub const CONNECT_RESPONSE: u32 = 0x80000;
    pub const NET_ERROR: u32 = 0x100000;
    pub const NET_ERROR_DISCONNECT: u32 = 0x200000;
    pub const CICMD_COMMAND: u32 = 0x400000;
    pub const TIME_SYNC: u32 = 0x1000000;
    pub const ECHO_REQUEST: u32 = 0x2000000;
    pub const ECHO_RESPONSE: u32 = 0x4000000;
    pub const FLOW: u32 = 0x8000000;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PacketHeader {
    pub sequence: u32,
    pub flags: u32,
    pub checksum: u32,
    pub id: u16,
    pub time: u16,
    pub size: u16,
    pub iteration: u16,
}
impl PacketHeader {
    pub fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            sequence: reader.u32()?,
            flags: reader.u32()?,
            checksum: reader.u32()?,
            id: reader.u16()?,
            time: reader.u16()?,
            size: reader.u16()?,
            iteration: reader.u16()?,
        })
    }
    pub fn encode(&self) -> [u8; PACKET_HEADER_SIZE] {
        let mut out = [0; PACKET_HEADER_SIZE];
        out[0..4].copy_from_slice(&self.sequence.to_le_bytes());
        out[4..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..12].copy_from_slice(&self.checksum.to_le_bytes());
        for (index, value) in [self.id, self.time, self.size, self.iteration]
            .into_iter()
            .enumerate()
        {
            out[12 + index * 2..14 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        out
    }
    pub fn hash(&self) -> u32 {
        let mut copy = *self;
        copy.checksum = 0xbadd70dd;
        hash32(&copy.encode())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FragmentHeader {
    pub sequence: u32,
    pub id: u32,
    pub count: u16,
    pub size: u16,
    pub index: u16,
    pub queue: u16,
}
impl FragmentHeader {
    pub fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            sequence: reader.u32()?,
            id: reader.u32()?,
            count: reader.u16()?,
            size: reader.u16()?,
            index: reader.u16()?,
            queue: reader.u16()?,
        })
    }
    pub fn encode(&self) -> [u8; FRAGMENT_HEADER_SIZE] {
        let mut out = [0; FRAGMENT_HEADER_SIZE];
        out[0..4].copy_from_slice(&self.sequence.to_le_bytes());
        out[4..8].copy_from_slice(&self.id.to_le_bytes());
        for (index, value) in [self.count, self.size, self.index, self.queue]
            .into_iter()
            .enumerate()
        {
            out[8 + index * 2..10 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        out
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub header: FragmentHeader,
    pub data: Vec<u8>,
}
impl Fragment {
    pub fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        let header = FragmentHeader::decode(reader)?;
        if header.count == 0
            || header.index >= header.count
            || header.queue >= 12
            || !(16..=464).contains(&header.size)
        {
            return Err(WireError::InvalidFragment);
        }
        let data = reader
            .take(usize::from(header.size) - FRAGMENT_HEADER_SIZE)?
            .to_vec();
        Ok(Self { header, data })
    }
    pub fn validate(&self) -> Result<(), WireError> {
        if self.header.count == 0
            || self.header.index >= self.header.count
            || self.header.queue >= 12
            || self.data.len() > FRAGMENT_DATA_LIMIT
            || usize::from(self.header.size) != FRAGMENT_HEADER_SIZE + self.data.len()
        {
            return Err(WireError::InvalidFragment);
        }
        Ok(())
    }
    pub fn hash(&self) -> u32 {
        hash32(&self.header.encode()).wrapping_add(hash32(&self.data))
    }
}

/// Header and declared body. ACE ignores bytes beyond the declared body; expose
/// their count so a caller can apply an explicit hardening policy.
pub struct Datagram<'a> {
    pub header: PacketHeader,
    pub body: &'a [u8],
    pub trailing_bytes: usize,
}
impl<'a> Datagram<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<Self, WireError> {
        if bytes.len() > CLIENT_DATAGRAM_LIMIT {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        let header = PacketHeader::decode(&mut reader)?;
        let body = reader.take(usize::from(header.size))?;
        Ok(Self {
            header,
            body,
            trailing_bytes: reader.remaining(),
        })
    }
}
/// Serialize a server datagram from already encoded optional-header bytes and
/// individually hashed fragments. Caller owns correct flags/optional ordering.
pub fn encode_server_packet(
    mut header: PacketHeader,
    optional: &[u8],
    fragments: &[Fragment],
    isaac_key: u32,
) -> Result<Vec<u8>, WireError> {
    // Bound output before allocating or copying caller-provided payloads.
    let total = fragments
        .iter()
        .try_fold(optional.len(), |total, fragment| {
            fragment.validate()?;
            total
                .checked_add(FRAGMENT_HEADER_SIZE + fragment.data.len())
                .ok_or(WireError::LimitExceeded)
        })?;
    if total > SERVER_BODY_LIMIT {
        return Err(WireError::LimitExceeded);
    }
    let mut body = Writer::new();
    body.bytes(optional);
    let mut hash = hash32(optional);
    for fragment in fragments {
        fragment.validate()?;
        body.bytes(&fragment.header.encode());
        body.bytes(&fragment.data);
        hash = hash.wrapping_add(fragment.hash());
    }
    let body = body.into_bytes();
    if body.len() > SERVER_BODY_LIMIT {
        return Err(WireError::LimitExceeded);
    }
    // ACE SendBundle may flush only optional headers with BlobFragments still set.
    // A real fragment without the flag remains invalid.
    if header.flags & flags::BLOB_FRAGMENTS == 0 && !fragments.is_empty() {
        return Err(WireError::InvalidFragment);
    }
    if header.flags & flags::ENCRYPTED_CHECKSUM == 0 && isaac_key != 0 {
        return Err(WireError::InvalidLength);
    }
    header.size = body.len() as u16;
    header.checksum = header.hash().wrapping_add(hash ^ isaac_key);
    let mut out = Vec::with_capacity(PACKET_HEADER_SIZE + body.len());
    out.extend_from_slice(&header.encode());
    out.extend_from_slice(&body);
    Ok(out)
}
