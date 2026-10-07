use bace_transport::{Peer, PeerConfig, PeerSeeds, RetransmitCache};
use bace_wire::{Datagram, Fragment, Isaac, OptionalHeaders, Reader, encode_server_packet, flags};
use serde_json::Value;

fn fixture() -> Value {
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/transport.json")).unwrap();
    assert_eq!(fixture["commit"], bace_compat::ACE_COMMIT);
    assert_eq!(fixture["repository"], bace_compat::ACE_REPOSITORY);
    for name in [
        "SendBundle",
        "WriteOptionalHeaders",
        "FlushPackets",
        "SendPacket",
        "Retransmit",
        "AcknowledgeSequence",
        "DoRequestForRetransmission",
        "PruneCachedPackets",
    ] {
        assert_eq!(
            fixture["extracted_method_sha256"][name]
                .as_str()
                .unwrap()
                .len(),
            64
        );
    }
    fixture
}
fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn packets(value: &Value) -> Vec<Vec<u8>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| hex(v.as_str().unwrap()))
        .collect()
}
fn outgoing(queue: u16, lengths: &[Value]) -> Peer {
    let mut peer = Peer::new(
        PeerConfig::default(),
        PeerSeeds {
            client: 12,
            server: 34,
        },
        0,
    )
    .unwrap();
    for length in lengths {
        let bytes = (0..length.as_u64().unwrap()).map(|n| n as u8).collect();
        peer.enqueue(queue, bytes).unwrap();
    }
    peer
}

#[test]
fn pinned_send_bundle_matches_ui_order_tail_packing_and_isaac_sequence() {
    for vector in fixture()["vectors"]["bundles"].as_array().unwrap() {
        if vector["optional"].as_bool().unwrap() {
            continue;
        }
        let expected = packets(&vector["packets"]);
        // Separate explicit regression below covers ACE's oversized-tail bug.
        if expected.iter().any(|p| p.len() > 484) {
            continue;
        }
        let mut peer = outgoing(
            vector["queue"].as_u64().unwrap() as u16,
            vector["lengths"].as_array().unwrap(),
        );
        assert_eq!(
            peer.poll(0, 123.25).unwrap(),
            expected,
            "queue={}, lengths={}",
            vector["queue"],
            vector["lengths"]
        );
    }
}

#[test]
fn pinned_optional_headers_preserve_budget_order_and_empty_fragment_flag() {
    for vector in fixture()["vectors"]["bundles"].as_array().unwrap() {
        if !vector["optional"].as_bool().unwrap() {
            continue;
        }
        let mut isaac = Isaac::new(34);
        for bytes in packets(&vector["packets"]) {
            let datagram = Datagram::decode(&bytes).unwrap();
            let mut reader = Reader::new(datagram.body);
            let optional = OptionalHeaders::decode(datagram.header.flags, &mut reader).unwrap();
            if optional.ack.is_some() {
                assert_eq!(optional.ack, Some(1));
                assert_eq!(optional.time_sync, Some(123.25));
                assert_eq!(optional.echo_response, Some((100.5, 22.75)));
                assert_eq!(reader.position(), 20);
            }
            let (_, optional_bytes) = optional.encode().unwrap();
            let mut fragments = Vec::new();
            while reader.remaining() != 0 {
                fragments.push(Fragment::decode(&mut reader).unwrap());
            }
            let encoded = encode_server_packet(
                datagram.header,
                &optional_bytes,
                &fragments,
                isaac.next_key(),
            )
            .unwrap();
            assert_eq!(encoded, bytes);
        }
    }
}

#[test]
fn pinned_exact_multiple_tail_overflow_is_an_explicit_bounded_deviation() {
    use bace_transport::{Reassembly, ReassemblyLimits};
    for vector in fixture()["vectors"]["bundles"].as_array().unwrap() {
        let expected = packets(&vector["packets"]);
        if !expected.iter().any(|p| p.len() > 484) {
            continue;
        }
        assert_eq!(vector["lengths"], serde_json::json!([4, 896]));
        assert_eq!(expected[0].len(), 504);
        let mut peer = outgoing(
            vector["queue"].as_u64().unwrap() as u16,
            vector["lengths"].as_array().unwrap(),
        );
        let actual = peer.poll(0, 123.25).unwrap();
        assert_eq!(actual.len(), 3);
        let mut assembly = Reassembly::new(ReassemblyLimits::default());
        let mut messages = Vec::new();
        for bytes in actual {
            assert!(bytes.len() <= 484);
            let datagram = Datagram::decode(&bytes).unwrap();
            let mut reader = Reader::new(datagram.body);
            while reader.remaining() != 0 {
                let fragment = Fragment::decode(&mut reader).unwrap();
                if let Some(message) = assembly.insert(fragment, 0).unwrap() {
                    messages.push(message);
                }
            }
        }
        assert_eq!(
            messages,
            [vec![0, 1, 2, 3], (0..896).map(|n| n as u8).collect()]
        );
    }
}

#[test]
fn pinned_retransmit_ack_nak_and_retention_transcript() {
    let fixture = fixture();
    let vector = &fixture["vectors"]["reliability"];
    let original = packets(&vector["originals"]);
    let mut cache = RetransmitCache::new(8, 8192, 120_000);
    for bytes in &original {
        cache.insert(bytes.clone(), 0).unwrap();
    }
    assert_eq!(
        cache.retransmit(2).unwrap().unwrap(),
        packets(&vector["replay"])[0]
    );
    assert!(vector["found"].as_bool().unwrap());
    assert_eq!(
        cache.retransmit(99).unwrap().is_some(),
        vector["missing"].as_bool().unwrap()
    );
    cache.acknowledge(3);
    assert_eq!(
        (2..=3)
            .filter(|s| cache.retransmit(*s).unwrap().is_some())
            .collect::<Vec<_>>(),
        vec![3]
    );
    assert_eq!(vector["afterAck"], serde_json::json!([3]));
    cache.expire(120_000).unwrap();
    assert!(cache.retransmit(3).unwrap().is_some());
    assert_eq!(vector["retainedAt120"], serde_json::json!([3]));
    cache.expire(121_000).unwrap();
    assert!(cache.retransmit(3).unwrap().is_none());
    assert_eq!(vector["retainedAt121"], serde_json::json!([]));
    let mut peer = outgoing(9, &[serde_json::json!(4), serde_json::json!(448)]);
    assert_eq!(peer.poll(0, 123.25).unwrap(), original);
    for sequence in [4, 7] {
        let bytes = encode_server_packet(
            bace_wire::PacketHeader {
                sequence,
                id: 1,
                ..Default::default()
            },
            &[],
            &[],
            0,
        )
        .unwrap();
        peer.receive(&bytes, 1).unwrap();
    }
    let emitted = peer.poll(1, 123.25).unwrap();
    assert_eq!(emitted, packets(&vector["nak"]));
    assert_eq!(
        Datagram::decode(&emitted[0]).unwrap().header.flags,
        flags::REQUEST_RETRANSMIT
    );
}

#[test]
fn pinned_initial_handshake_and_first_encrypted_sequence_promotion() {
    let fixture = fixture();
    let expected = packets(&fixture["vectors"]["initialCounters"]);
    assert_eq!(Datagram::decode(&expected[0]).unwrap().header.sequence, 0);
    let mut peer = Peer::new(
        PeerConfig::default(),
        PeerSeeds {
            client: 12,
            server: 34,
        },
        0,
    )
    .unwrap();
    assert_eq!(peer.poll(2001, 123.25).unwrap(), vec![expected[1].clone()]);
    peer.enable_time_sync();
    assert_eq!(peer.poll(2006, 123.25).unwrap(), vec![expected[2].clone()]);
}
