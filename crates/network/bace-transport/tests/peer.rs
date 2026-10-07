use bace_transport::*;
use bace_wire::*;

fn peer() -> Peer {
    Peer::new(
        PeerConfig::default(),
        PeerSeeds {
            client: 12,
            server: 34,
        },
        0,
    )
    .unwrap()
}
fn packet(sequence: u32, optional: OptionalHeaders, fragments: &[Fragment]) -> Vec<u8> {
    let (mut bits, bytes) = optional.encode().unwrap();
    if !fragments.is_empty() {
        bits |= flags::BLOB_FRAGMENTS;
    }
    encode_server_packet(
        PacketHeader {
            sequence,
            flags: bits,
            id: 1,
            ..PacketHeader::default()
        },
        &bytes,
        fragments,
        0,
    )
    .unwrap()
}
fn headers(bytes: &[u8]) -> (PacketHeader, OptionalHeaders) {
    let datagram = Datagram::decode(bytes).unwrap();
    (
        datagram.header,
        OptionalHeaders::decode(datagram.header.flags, &mut Reader::new(datagram.body)).unwrap(),
    )
}

#[test]
fn message_order_is_global_and_preserves_routing_metadata() {
    let mut peer = peer();
    let later = fragment_message(2, 9, &[2; 4], 1024).unwrap();
    assert!(
        peer.receive(&packet(3, OptionalHeaders::default(), &later), 1)
            .unwrap()
            .messages
            .is_empty()
    );
    let first = fragment_message(1, 3, &[1; 449], 1024).unwrap();
    assert!(
        peer.receive(&packet(2, OptionalHeaders::default(), &first[1..]), 2)
            .unwrap()
            .messages
            .is_empty()
    );
    let ready = peer
        .receive(&packet(4, OptionalHeaders::default(), &first[..1]), 3)
        .unwrap();
    assert_eq!(
        ready
            .messages
            .iter()
            .map(|m| (m.sequence, m.queue))
            .collect::<Vec<_>>(),
        [(1, 3), (2, 9)]
    );
    assert_eq!(ready.messages[0].bytes, vec![1; 449]);
    assert_eq!(ready.messages[0].id, 0x80000000);
    assert!(
        peer.receive(&packet(4, OptionalHeaders::default(), &[]), 4)
            .unwrap()
            .duplicate
    );
}

#[test]
fn incoming_gaps_request_bounded_retransmit_and_recover() {
    let mut peer = peer();
    peer.receive(&packet(5, OptionalHeaders::default(), &[]), 1)
        .unwrap();
    let emitted = peer.poll(1, 1.0).unwrap();
    assert_eq!(emitted.len(), 1);
    let (header, optional) = headers(&emitted[0]);
    assert_eq!(header.flags, flags::REQUEST_RETRANSMIT);
    assert_eq!(header.sequence, 0);
    assert_eq!(optional.retransmit, Some(vec![2, 3, 4]));
    assert!(peer.poll(1001, 2.0).unwrap().is_empty());
    assert_eq!(peer.poll(1002, 2.0).unwrap().len(), 1);
    for sequence in 2..5 {
        peer.receive(&packet(sequence, OptionalHeaders::default(), &[]), 1003)
            .unwrap();
    }
    assert_eq!(peer.last_received_sequence(), 5);
    assert!(peer.poll(1004, 2.0).unwrap().is_empty());
}

#[test]
fn ack_sync_and_echo_timers_follow_explicit_clock() {
    let mut peer = peer();
    assert!(peer.poll(2000, 10.0).unwrap().is_empty());
    let ack = peer.poll(2001, 10.0).unwrap();
    let (header, optional) = headers(&ack[0]);
    assert_eq!(header.flags, flags::ACK_SEQUENCE);
    assert_eq!(header.sequence, 0);
    assert_eq!(optional.ack, Some(1));
    peer.enable_time_sync();
    let sync = peer.poll(2006, 12.5).unwrap();
    let (header, optional) = headers(&sync[0]);
    assert_eq!(header.sequence, 2);
    assert_eq!(header.flags, flags::TIME_SYNC | flags::ENCRYPTED_CHECKSUM);
    assert_eq!(optional.time_sync, Some(12.5));
    // Drifting client time remains only a request to echo. It is not authority.
    peer.receive(
        &packet(
            2,
            OptionalHeaders {
                echo_request: Some(99999.0),
                ..Default::default()
            },
            &[],
        ),
        2007,
    )
    .unwrap();
    let response = peer.poll(2011, 13.0).unwrap();
    let (_, optional) = headers(&response[0]);
    assert_eq!(optional.echo_response, Some((99999.0, -99986.0)));
    assert!(!peer.is_closed());
    assert_eq!(peer.poll(2010, 13.0), Err(TransportError::InvalidClock));
}

#[test]
fn cicmd_unsequenced_keepalive_has_no_response_or_disconnect() {
    let mut peer = peer();
    let command = [1, 0, 0, 0, 0, 0, 0, 0];
    for now in [0, 110_000, 220_000] {
        let input = peer
            .receive(
                &packet(
                    0,
                    OptionalHeaders {
                        command: Some(command),
                        ..Default::default()
                    },
                    &[],
                ),
                now,
            )
            .unwrap();
        assert_eq!(input.controls, [PeerControl::Command(command)]);
        assert!(input.messages.is_empty());
        assert_eq!(peer.last_received_sequence(), 1);
        assert!(!peer.is_closed());
    }
}

#[test]
fn reliable_overload_and_incomplete_message_expiry_close_peer() {
    let config = PeerConfig {
        max_outgoing_messages: 1,
        ..Default::default()
    };
    let mut limited = Peer::new(
        config,
        PeerSeeds {
            client: 0,
            server: 0,
        },
        0,
    )
    .unwrap();
    limited.enqueue(9, vec![0; 4]).unwrap();
    assert_eq!(
        limited.enqueue(9, vec![0; 4]),
        Err(TransportError::Capacity)
    );
    assert!(limited.is_closed());
    assert_eq!(limited.poll(1, 0.0), Err(TransportError::Closed));
    let mut peer = peer();
    let parts = fragment_message(1, 9, &vec![1; 449], 1024).unwrap();
    peer.receive(&packet(2, OptionalHeaders::default(), &parts[..1]), 0)
        .unwrap();
    assert_eq!(peer.poll(30000, 30.0), Err(TransportError::ExpiredMessage));
    assert!(peer.is_closed());
}

#[test]
fn cache_retransmission_preserves_checksum_key_and_rejects_misses() {
    let mut peer = peer();
    peer.enqueue(9, vec![1; 4]).unwrap();
    let first = peer.poll(0, 0.0).unwrap();
    let original = Datagram::decode(&first[0]).unwrap().header;
    peer.receive(
        &packet(
            0,
            OptionalHeaders {
                retransmit: Some(vec![2, 99]),
                ..Default::default()
            },
            &[],
        ),
        1,
    )
    .unwrap();
    let replies = peer.poll(1, 0.0).unwrap();
    assert_eq!(replies.len(), 2);
    let resend = Datagram::decode(&replies[0]).unwrap().header;
    assert_eq!(resend.sequence, 2);
    assert_eq!(resend.flags, original.flags | flags::RETRANSMISSION);
    assert_eq!(
        resend.checksum.wrapping_sub(resend.hash()),
        original.checksum.wrapping_sub(original.hash())
    );
    assert_eq!(headers(&replies[1]).1.reject_retransmit, Some(vec![99]));
    peer.receive(
        &packet(
            2,
            OptionalHeaders {
                ack: Some(3),
                ..Default::default()
            },
            &[],
        ),
        2,
    )
    .unwrap();
    peer.receive(
        &packet(
            0,
            OptionalHeaders {
                retransmit: Some(vec![2]),
                ..Default::default()
            },
            &[],
        ),
        3,
    )
    .unwrap();
    assert_eq!(
        headers(&peer.poll(3, 0.0).unwrap()[0]).1.reject_retransmit,
        Some(vec![2])
    );
}

#[test]
fn unauthenticated_checksums_do_not_poison_future_valid_keys() {
    let mut peer = peer();
    let fragments = fragment_message(1, 9, &[1; 4], 1024).unwrap();
    let mut isaac = Isaac::new(12);
    let valid = encode_server_packet(
        PacketHeader {
            sequence: 2,
            id: 1,
            flags: flags::ENCRYPTED_CHECKSUM | flags::BLOB_FRAGMENTS,
            ..Default::default()
        },
        &[],
        &fragments,
        isaac.next_key(),
    )
    .unwrap();
    let mut invalid = valid.clone();
    invalid[8] ^= 0xff;
    for _ in 0..3 {
        assert_eq!(peer.receive(&invalid, 0), Err(TransportError::Checksum));
    }
    assert!(!peer.is_closed());
    assert_eq!(peer.receive(&valid, 0).unwrap().messages.len(), 1);
}

#[test]
fn output_packet_and_work_budgets_hold_across_large_messages() {
    let config = PeerConfig {
        max_datagrams_per_poll: 2,
        ..Default::default()
    };
    let mut peer = Peer::new(
        config,
        PeerSeeds {
            client: 0,
            server: 0,
        },
        0,
    )
    .unwrap();
    peer.enqueue(3, vec![0; 2000]).unwrap();
    let mut emitted = Vec::new();
    while peer.queued_message_bytes() > 0 {
        let packets = peer.poll(0, 65537.0).unwrap();
        assert!(!packets.is_empty());
        assert!(packets.len() <= 2);
        emitted.extend(packets);
    }
    assert_eq!(emitted.len(), 5);
    for bytes in emitted {
        assert!(bytes.len() <= 484);
        assert_eq!(Datagram::decode(&bytes).unwrap().header.time, 1);
    }
}
