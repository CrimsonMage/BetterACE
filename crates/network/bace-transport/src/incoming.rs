use crate::TransportError;
use bace_wire::{
    ClientKeys, Datagram, Fragment, OptionalHeaders, PacketHeader, Reader, flags, hash32,
};
#[derive(Debug)]
pub struct IncomingPacket {
    pub header: PacketHeader,
    pub optional: OptionalHeaders,
    pub fragments: Vec<Fragment>,
}
/// Transport decode; handshake/referral currently return UnsupportedFlags.
pub fn decode_transport_packet(
    bytes: &[u8],
    keys: &mut ClientKeys,
) -> Result<IncomingPacket, TransportError> {
    let (packet, payload_hash) = parse_transport_packet(bytes)?;
    let valid = if packet.header.flags & flags::ENCRYPTED_CHECKSUM != 0 {
        keys.verify_and_consume(
            packet.header.checksum.wrapping_sub(packet.header.hash()) ^ payload_hash,
        )
    } else {
        packet.header.checksum == packet.header.hash().wrapping_add(payload_hash)
    };
    if !valid {
        return Err(TransportError::Checksum);
    }
    Ok(packet)
}

pub(crate) fn parse_transport_packet(
    bytes: &[u8],
) -> Result<(IncomingPacket, u32), TransportError> {
    let packet = Datagram::decode(bytes)?;
    let mut reader = Reader::new(packet.body);
    let optional = OptionalHeaders::decode(packet.header.flags, &mut reader)?;
    let mut payload_hash = hash32(&packet.body[..reader.position()]);
    let mut fragments = Vec::new();
    if packet.header.flags & flags::BLOB_FRAGMENTS != 0 {
        while reader.remaining() > 0 {
            let fragment = Fragment::decode(&mut reader)?;
            payload_hash = payload_hash.wrapping_add(fragment.hash());
            fragments.push(fragment);
        }
    } else if reader.remaining() != 0 {
        return Err(TransportError::InvalidFragment);
    }
    Ok((
        IncomingPacket {
            header: packet.header,
            optional,
            fragments,
        },
        payload_hash,
    ))
}
