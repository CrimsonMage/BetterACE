use bace_transport::*;
use bace_wire::*;
fn packet(sequence: u32) -> Vec<u8> {
    encode_server_packet(
        PacketHeader {
            sequence,
            ..PacketHeader::default()
        },
        &[],
        &[],
        0,
    )
    .unwrap()
}
#[test]
fn fragmentation_boundary_and_tail_first_reassembly() {
    for length in [4usize, 447, 448, 449, 896, 897] {
        let bytes: Vec<u8> = (0..length).map(|index| index as u8).collect();
        let mut parts = fragment_message(1, 9, &bytes, 1024).unwrap();
        assert_eq!(parts.len(), length.div_ceil(448));
        let mut assembly = Reassembly::new(ReassemblyLimits::default());
        let mut result = None;
        while let Some(part) = parts.pop() {
            result = assembly.insert(part, 0).unwrap();
        }
        assert_eq!(result.unwrap(), bytes);
        assert_eq!(assembly.buffered_bytes(), 0);
    }
}
#[test]
fn duplicates_conflicts_expiry_and_memory_limits() {
    let parts = fragment_message(1, 9, &vec![1; 900], 1000).unwrap();
    let mut assembly = Reassembly::new(ReassemblyLimits {
        max_messages: 1,
        max_message_bytes: 1024,
        max_total_bytes: 450,
        lifetime_ms: 10,
    });
    assert_eq!(assembly.insert(parts[0].clone(), 0).unwrap(), None);
    assert_eq!(assembly.insert(parts[0].clone(), 1).unwrap(), None);
    let mut conflict = parts[0].clone();
    conflict.data[0] = 2;
    assert_eq!(
        assembly.insert(conflict, 2),
        Err(TransportError::ConflictingFragment)
    );
    assert_eq!(
        assembly.insert(parts[1].clone(), 3),
        Err(TransportError::Capacity)
    );
    assert_eq!(assembly.buffered_bytes(), 448);
    assert_eq!(assembly.expire(10).unwrap(), 1);
    assert_eq!(assembly.buffered_bytes(), 0);
    assert_eq!(assembly.expire(9), Err(TransportError::InvalidClock));
}
#[test]
fn ordering_handles_loss_duplicates_and_bounded_gaps() {
    let mut order = OrderedPackets::new(1, 2, 1024, 256);
    assert!(order.receive(packet(3)).unwrap().ready.is_empty());
    assert!(order.receive(packet(3)).unwrap().duplicate);
    assert_eq!(order.missing(4), vec![2]);
    assert_eq!(
        order.receive(packet(2)).unwrap().ready,
        vec![packet(2), packet(3)]
    );
    assert_eq!(order.last_received(), 3);
    assert!(order.receive(packet(2)).unwrap().duplicate);
    assert_eq!(
        order.receive(packet(999)),
        Err(TransportError::SequenceWindow)
    );
    assert_eq!(order.receive(packet(0)).unwrap().ready, vec![packet(0)]);
}
#[test]
fn cache_ack_is_exclusive_and_expiry_bounded() {
    let mut cache = RetransmitCache::new(2, 1024, 10);
    cache.insert(packet(2), 0).unwrap();
    cache.insert(packet(3), 0).unwrap();
    assert_eq!(cache.insert(packet(4), 0), Err(TransportError::Capacity));
    cache.acknowledge(3);
    assert!(cache.retransmit(2).unwrap().is_none());
    assert!(cache.retransmit(3).unwrap().is_some());
    cache.expire(10).unwrap();
    assert!(cache.retransmit(3).unwrap().is_some());
    cache.expire(11).unwrap();
    assert!(cache.retransmit(3).unwrap().is_none());
}
#[test]
fn checksum_validation_and_every_truncation() {
    let mut keys = ClientKeys::new(12);
    let mut outgoing = Isaac::new(12);
    let fragments = fragment_message(1, 9, &[1, 2, 3, 4], 1024).unwrap();
    let bytes = encode_server_packet(
        PacketHeader {
            sequence: 2,
            flags: flags::BLOB_FRAGMENTS | flags::ENCRYPTED_CHECKSUM,
            ..PacketHeader::default()
        },
        &[],
        &fragments,
        outgoing.next_key(),
    )
    .unwrap();
    for length in 0..bytes.len() {
        assert!(decode_transport_packet(&bytes[..length], &mut keys).is_err());
    }
    assert_eq!(
        decode_transport_packet(&bytes, &mut keys)
            .unwrap()
            .fragments,
        fragments
    );
    assert!(decode_transport_packet(&bytes, &mut keys).is_err());
}
#[test]
fn sequence_exhaustion_and_conflicting_single_fragment_fail_explicitly() {
    let mut order = OrderedPackets::new(u32::MAX - 1, 2, 1024, 256);
    assert_eq!(order.receive(packet(u32::MAX)).unwrap().ready.len(), 1);
    assert_eq!(
        order.receive(packet(0)),
        Err(TransportError::SequenceExhausted)
    );
    let mut assembly = Reassembly::new(ReassemblyLimits::default());
    let partial = fragment_message(1, 9, &vec![1; 449], 1024).unwrap();
    assembly.insert(partial[0].clone(), 0).unwrap();
    let single = fragment_message(1, 9, &[1, 2, 3, 4], 1024).unwrap();
    assert_eq!(
        assembly.insert(single[0].clone(), 1),
        Err(TransportError::ConflictingFragment)
    );
    assert_eq!(assembly.buffered_bytes(), 448);
}
