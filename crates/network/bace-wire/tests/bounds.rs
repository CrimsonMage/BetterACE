use bace_wire::*;
#[test]
fn truncated_headers_and_fragments_are_errors() {
    for length in 0..20 {
        assert!(PacketHeader::decode(&mut Reader::new(&vec![0; length])).is_err());
    }
    let header = FragmentHeader {
        sequence: 1,
        id: 0x80000000,
        count: 1,
        size: 20,
        index: 0,
        queue: 9,
    };
    for payload in 0..4 {
        let mut bytes = header.encode().to_vec();
        bytes.resize(16 + payload, 0);
        assert!(Fragment::decode(&mut Reader::new(&bytes)).is_err());
    }
    for (count, index, size, queue) in [
        (0, 0, 16, 0),
        (1, 1, 16, 0),
        (1, 0, 15, 0),
        (1, 0, 465, 0),
        (1, 0, 16, 12),
    ] {
        let header = FragmentHeader {
            count,
            index,
            size,
            queue,
            ..header
        };
        assert!(Fragment::decode(&mut Reader::new(&header.encode())).is_err());
    }
}
#[test]
fn declared_body_and_count_cannot_trigger_unbounded_reads() {
    let header = PacketHeader {
        size: u16::MAX,
        ..PacketHeader::default()
    };
    assert!(Datagram::decode(&header.encode()).is_err());
    assert!(Datagram::decode(&[0; 1025]).is_err());
    assert!(
        OptionalHeaders::decode(
            flags::REQUEST_RETRANSMIT,
            &mut Reader::new(&u32::MAX.to_le_bytes())
        )
        .is_err()
    );
    assert!(OptionalHeaders::decode(flags::LOGIN_REQUEST, &mut Reader::new(&[])).is_err());
}
#[test]
fn string_and_packed_boundaries_are_checked_before_write() {
    let mut writer = Writer::new();
    assert!(writer.string16("😸").is_err());
    assert!(writer.string16(&"a".repeat(65536)).is_err());
    assert!(writer.packed_u32(u32::MAX).is_err());
    assert!(writer.into_bytes().is_empty());
    assert!(Reader::new(&[2, 0, b'a']).string16(100).is_err());
    assert!(Reader::new(&[2, 0, b'a', b'b']).string16(1).is_err());
}
#[test]
fn optional_headers_follow_ace_order_and_every_truncation_fails() {
    let value = OptionalHeaders {
        server_switch: Some([8; 8]),
        retransmit: Some(vec![7, 8]),
        reject_retransmit: Some(vec![9]),
        ack: Some(10),
        command: Some([11; 8]),
        time_sync: Some(12.25),
        echo_request: Some(13.5),
        echo_response: Some((13.5, 0.25)),
        flow: Some((14, 15)),
    };
    let (flags, bytes) = value.encode().unwrap();
    assert_eq!(&bytes[..8], &[8; 8]);
    assert_eq!(&bytes[8..12], &2u32.to_le_bytes());
    assert_eq!(
        OptionalHeaders::decode(flags, &mut Reader::new(&bytes)).unwrap(),
        value
    );
    for length in 0..bytes.len() {
        assert!(OptionalHeaders::decode(flags, &mut Reader::new(&bytes[..length])).is_err());
    }
}
#[test]
fn incoming_keys_allow_reordering_but_reject_reuse() {
    let mut server = Isaac::new(17);
    let keys: Vec<_> = (0..5).map(|_| server.next_key()).collect();
    let mut client = ClientKeys::new(17);
    for index in [4, 0, 2, 1, 3] {
        assert!(client.verify_and_consume(keys[index]));
    }
    assert!(!client.verify_and_consume(keys[0]));
}
#[test]
fn arbitrary_short_inputs_never_panic() {
    let mut seed = 7u32;
    for length in 0..1100 {
        let data: Vec<u8> = (0..length)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        let _ = Datagram::decode(&data);
        let _ = Fragment::decode(&mut Reader::new(&data));
        let _ = OptionalHeaders::decode(seed, &mut Reader::new(&data));
    }
}
