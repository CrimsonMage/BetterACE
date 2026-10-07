use bace_wire::{FragmentHeader, Isaac, PacketHeader, Reader, hash32};
fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn official_csharp_header_hash_and_isaac_vectors() {
    let fixture: serde_json::Value =
        serde_json::from_str(bace_compat::primitive_fixture()).unwrap();
    assert_eq!(fixture["commit"], bace_compat::ACE_COMMIT);
    assert_eq!(fixture["repository"], bace_compat::ACE_REPOSITORY);
    let vectors = &fixture["vectors"];
    let header = hex(vectors["header"].as_str().unwrap());
    let decoded = PacketHeader::decode(&mut Reader::new(&header)).unwrap();
    assert_eq!(decoded.encode().as_slice(), header);
    assert_eq!(decoded.sequence, 0x12345678);
    assert_eq!(decoded.flags, 6);
    assert_eq!(decoded.id, 0x1122);
    assert_eq!(decoded.size, 464);
    assert_eq!(
        decoded.hash(),
        vectors["header_hash"].as_u64().unwrap() as u32
    );
    let fragment = hex(vectors["fragment"].as_str().unwrap());
    let decoded = FragmentHeader::decode(&mut Reader::new(&fragment)).unwrap();
    assert_eq!(decoded.encode().as_slice(), fragment);
    assert_eq!(
        (
            decoded.sequence,
            decoded.id,
            decoded.count,
            decoded.size,
            decoded.index,
            decoded.queue
        ),
        (42, 0x80000000, 3, 464, 1, 10)
    );
    for vector in vectors["hashes"].as_array().unwrap() {
        assert_eq!(
            hash32(&hex(vector["input"].as_str().unwrap())),
            vector["hash"].as_u64().unwrap() as u32
        );
    }
    for vector in vectors["isaac"].as_array().unwrap() {
        let mut isaac = Isaac::new(vector["seed"].as_u64().unwrap() as u32);
        for expected in vector["keys"].as_array().unwrap() {
            assert_eq!(isaac.next_key(), expected.as_u64().unwrap() as u32);
        }
    }
}
#[test]
fn official_csharp_strings_packing_connect_and_packet_checksums() {
    use bace_wire::{
        ConnectRequest, Fragment, FragmentHeader, Writer, encode_server_packet, flags,
    };
    let fixture: serde_json::Value =
        serde_json::from_str(bace_compat::primitive_fixture()).unwrap();
    let vectors = &fixture["vectors"];
    for vector in vectors["strings"].as_array().unwrap() {
        let value = vector["value"].as_str().unwrap();
        let mut writer = Writer::new();
        writer.string16(value).unwrap();
        assert_eq!(writer.into_bytes(), hex(vector["bytes"].as_str().unwrap()));
    }
    for vector in vectors["packed"].as_array().unwrap() {
        let value = vector["value"].as_u64().unwrap() as u32;
        let mut writer = Writer::new();
        writer.packed_u32(value).unwrap();
        let bytes = hex(vector["bytes"].as_str().unwrap());
        assert_eq!(writer.into_bytes(), bytes);
        assert_eq!(Reader::new(&bytes).packed_u32().unwrap(), value);
    }
    let connect = ConnectRequest {
        server_time: 123.25,
        cookie: 0x123456789abcdef0,
        client_id: 42,
        server_seed: 0x87654321,
        client_seed: 0x11223344,
    };
    assert_eq!(connect.encode(), hex(vectors["connect"].as_str().unwrap()));
    for vector in vectors["packets"].as_array().unwrap() {
        let key = vector["key"].as_u64().unwrap() as u32;
        let header = PacketHeader {
            sequence: 2,
            flags: flags::BLOB_FRAGMENTS
                | flags::ACK_SEQUENCE
                | if key == 0 {
                    0
                } else {
                    flags::ENCRYPTED_CHECKSUM
                },
            checksum: 0,
            id: 7,
            time: 19,
            size: 0,
            iteration: 1,
        };
        let fragment = Fragment {
            header: FragmentHeader {
                sequence: 1,
                id: 0x80000000,
                count: 1,
                size: 21,
                index: 0,
                queue: 9,
            },
            data: vec![1, 2, 3, 4, 5],
        };
        let encoded = encode_server_packet(header, &12u32.to_le_bytes(), &[fragment], key).unwrap();
        assert_eq!(encoded, hex(vector["bytes"].as_str().unwrap()));
        let mut cache = bace_transport::RetransmitCache::new(4, 4096, 120000);
        cache.insert(encoded, 0).unwrap();
        assert_eq!(
            cache.retransmit(2).unwrap().unwrap(),
            hex(vector["retransmit"].as_str().unwrap())
        );
    }
}
#[test]
fn official_csharp_optional_header_decode_and_hash() {
    use bace_wire::OptionalHeaders;
    let fixture: serde_json::Value =
        serde_json::from_str(bace_compat::primitive_fixture()).unwrap();
    for vector in fixture["vectors"]["optional"].as_array().unwrap() {
        let flags = vector["flags"].as_u64().unwrap() as u32;
        let bytes = hex(vector["bytes"].as_str().unwrap());
        let mut reader = Reader::new(&bytes);
        let decoded = OptionalHeaders::decode(flags, &mut reader).unwrap();
        assert_eq!(reader.position(), vector["size"].as_u64().unwrap() as usize);
        assert_eq!(hash32(&bytes), vector["hash"].as_u64().unwrap() as u32);
        assert_eq!(
            decoded.ack.unwrap_or(0),
            vector["ack"].as_u64().unwrap() as u32
        );
        assert_eq!(
            decoded.time_sync.unwrap_or(0.0),
            vector["time"].as_f64().unwrap()
        );
        assert_eq!(
            decoded.echo_request.unwrap_or(0.0),
            vector["echo"].as_f64().unwrap() as f32
        );
        assert_eq!(
            decoded.flow.unwrap_or((0, 0)),
            (
                vector["flow_bytes"].as_u64().unwrap() as u32,
                vector["flow_interval"].as_u64().unwrap() as u16
            )
        );
        assert_eq!(decoded.encode().unwrap(), (flags, bytes));
    }
}
#[test]
fn official_login_payloads_preserve_utf8_units_and_secret_prefix_boundaries() {
    use bace_wire::{LoginCredential, LoginRequest, NetAuthType};
    let fixture: serde_json::Value =
        serde_json::from_str(bace_compat::primitive_fixture()).unwrap();
    for vector in fixture["vectors"]["logins"].as_array().unwrap() {
        let payload = hex(vector["bytes"].as_str().unwrap());
        let decoded = LoginRequest::decode_payload(&payload).unwrap();
        assert_eq!(decoded.client_version, vector["version"].as_str().unwrap());
        assert_eq!(decoded.account, vector["account"].as_str().unwrap());
        assert_eq!(
            decoded.net_auth_type,
            NetAuthType::from(vector["type"].as_u64().unwrap() as u32)
        );
        assert_eq!(
            decoded.timestamp,
            vector["timestamp"].as_u64().unwrap() as u32
        );
        assert_eq!(decoded.declared_length, 999);
        let expected = match decoded.net_auth_type {
            NetAuthType::AccountPassword => {
                LoginCredential::Password(vector["password"].as_str().unwrap().to_owned())
            }
            NetAuthType::GlsTicket => {
                LoginCredential::GlsTicket(vector["ticket"].as_str().unwrap().to_owned())
            }
            _ => LoginCredential::None,
        };
        assert_eq!(decoded.credential, expected);
    }
}
