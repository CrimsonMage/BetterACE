use bace_runtime::network::*;
use bace_transport::fragment_message;
use bace_wire::*;
use std::{
    net::UdpSocket,
    time::{Duration, Instant},
};

fn start() -> NetworkThread {
    for _ in 0..20 {
        let config = NetworkThreadConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            max_sessions: 2,
            ..NetworkThreadConfig::default()
        };
        if let Ok(worker) = NetworkThread::spawn(config) {
            return worker;
        }
    }
    panic!("no available UDP port pair");
}
fn socket() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
}
fn login_packet() -> Vec<u8> {
    // Payload from the independently compiled official inbound-login oracle.
    let payload_hex = "0400313830320000E703000002000000010000003930000007004163636F756E7400000000000000130000001273796E7468657469632D70617373776F72640000";
    let payload: Vec<u8> = payload_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    encode_server_packet(
        PacketHeader {
            flags: flags::LOGIN_REQUEST,
            ..PacketHeader::default()
        },
        &payload,
        &[],
        0,
    )
    .unwrap()
}
fn event(worker: &NetworkThread) -> NetworkEvent {
    worker
        .events()
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
}
#[test]
fn real_udp_handshake_reliable_delivery_and_explicit_drain() {
    let worker = start();
    let client = socket();
    let companion = socket();
    client
        .send_to(&login_packet(), worker.client_address)
        .unwrap();
    let key = match event(&worker) {
        NetworkEvent::Login { key, request } => {
            assert_eq!(request.account, "Account");
            key
        }
        other => panic!("{other:?}"),
    };
    worker
        .try_send(NetworkCommand::Authenticated {
            key,
            account_id: bace_types::AccountId(1),
        })
        .unwrap();
    let mut buffer = [0; 1025];
    let (size, source) = client.recv_from(&mut buffer).unwrap();
    assert_eq!(source, worker.client_address);
    let packet = Datagram::decode(&buffer[..size]).unwrap();
    assert_eq!(packet.header.flags, flags::CONNECT_REQUEST);
    assert_eq!(packet.header.sequence, 0);
    let challenge = ConnectRequest::decode(packet.body).unwrap();
    let connect = encode_server_packet(
        PacketHeader {
            flags: flags::CONNECT_RESPONSE,
            ..PacketHeader::default()
        },
        &challenge.cookie.to_le_bytes(),
        &[],
        0,
    )
    .unwrap();
    companion.send_to(&connect, worker.server_address).unwrap();
    assert!(matches!(event(&worker), NetworkEvent::Connected{ key:k } if k==key));
    let bytes = [0xB1, 0xF7, 0, 0, 1, 0, 0, 0, 0xA1, 0, 0, 0];
    let fragments = fragment_message(1, 9, &bytes, 1024).unwrap();
    let mut keys = Isaac::new(challenge.client_seed);
    let message = encode_server_packet(
        PacketHeader {
            sequence: 2,
            flags: flags::ENCRYPTED_CHECKSUM | flags::BLOB_FRAGMENTS,
            id: key.id,
            iteration: 1,
            ..PacketHeader::default()
        },
        &[],
        &fragments,
        keys.next_key(),
    )
    .unwrap();
    client.send_to(&message, worker.client_address).unwrap();
    assert!(
        matches!(event(&worker),NetworkEvent::Message { key:k,message:m } if k==key && m.bytes==bytes)
    );
    worker
        .try_send(NetworkCommand::Send {
            key,
            queue: 9,
            bytes: vec![0xE1, 0xF7, 0, 0],
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let (size, source) = companion.recv_from(&mut buffer).unwrap();
        assert_eq!(source, worker.server_address);
        let packet = Datagram::decode(&buffer[..size]).unwrap();
        assert!(size <= 484);
        if packet.header.flags & flags::BLOB_FRAGMENTS != 0 {
            break;
        }
        assert!(Instant::now() < deadline);
    }
    worker.terminate(key).unwrap();
    assert!(
        matches!(event(&worker),NetworkEvent::Terminated { key:k,reason:NetworkStopReason::Requested } if k==key)
    );
    worker
        .try_send(NetworkCommand::DrainCompleted { key })
        .unwrap();
    worker.shutdown().unwrap();
}
#[test]
fn stalled_event_consumer_does_not_prevent_shutdown() {
    let mut config = NetworkThreadConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        event_capacity: 1,
        max_sessions: 2,
        ..NetworkThreadConfig::default()
    };
    config.network.max_sessions_per_ip = 2;
    let worker = NetworkThread::spawn(config).unwrap();
    let a = socket();
    let b = socket();
    a.send_to(&login_packet(), worker.client_address).unwrap();
    b.send_to(&login_packet(), worker.client_address).unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    worker.shutdown().unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn lost_challenge_is_recoverable_and_pre_cookie_output_is_rejected() {
    let worker = start();
    let client = socket();
    client
        .send_to(&login_packet(), worker.client_address)
        .unwrap();
    let key = match event(&worker) {
        NetworkEvent::Login { key, .. } => key,
        other => panic!("{other:?}"),
    };
    worker
        .try_send(NetworkCommand::Authenticated {
            key,
            account_id: bace_types::AccountId(1),
        })
        .unwrap();
    let mut first = [0; 1025];
    let (first_size, _) = client.recv_from(&mut first).unwrap();
    worker
        .try_send(NetworkCommand::Send {
            key,
            queue: 9,
            bytes: vec![1, 2, 3, 4],
        })
        .unwrap();
    assert!(matches!(event(&worker),NetworkEvent::CommandRejected{key:k} if k==key));
    // Simulate challenge loss by discarding it and retransmitting the login.
    client
        .send_to(&login_packet(), worker.client_address)
        .unwrap();
    let mut retry = [0; 1025];
    let (retry_size, _) = client.recv_from(&mut retry).unwrap();
    assert_eq!(&first[..first_size], &retry[..retry_size]);
    client
        .set_read_timeout(Some(Duration::from_millis(2200)))
        .unwrap();
    assert!(
        client.recv_from(&mut retry).is_err(),
        "no periodic ACK before cookie proof"
    );
    worker.shutdown().unwrap();
}

#[test]
fn replacement_waits_for_old_owner_drain_and_stale_completion_is_rejected() {
    let worker = start();
    let old = socket();
    let new = socket();
    old.send_to(&login_packet(), worker.client_address).unwrap();
    let first = match event(&worker) {
        NetworkEvent::Login { key, .. } => key,
        other => panic!("{other:?}"),
    };
    worker
        .try_send(NetworkCommand::Authenticated {
            key: first,
            account_id: bace_types::AccountId(1),
        })
        .unwrap();
    let mut buffer = [0; 1025];
    old.recv_from(&mut buffer).unwrap();
    new.send_to(&login_packet(), worker.client_address).unwrap();
    let second = match event(&worker) {
        NetworkEvent::Login { key, .. } => key,
        other => panic!("{other:?}"),
    };
    worker
        .try_send(NetworkCommand::Authenticated {
            key: second,
            account_id: bace_types::AccountId(1),
        })
        .unwrap();
    assert!(
        matches!(event(&worker),NetworkEvent::Terminated{key,reason:NetworkStopReason::Replaced} if key==first)
    );
    new.set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    assert!(new.recv_from(&mut buffer).is_err());
    worker
        .try_send(NetworkCommand::DrainCompleted { key: first })
        .unwrap();
    new.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let (size, _) = new.recv_from(&mut buffer).unwrap();
    let packet = Datagram::decode(&buffer[..size]).unwrap();
    assert_eq!(
        ConnectRequest::decode(packet.body).unwrap().client_id,
        u32::from(second.id)
    );
    worker
        .try_send(NetworkCommand::DrainCompleted { key: first })
        .unwrap();
    assert!(matches!(event(&worker),NetworkEvent::CommandRejected{key} if key==first));
    worker.shutdown().unwrap();
}

#[test]
fn portal_origin_is_explicit_and_header_time_wraps_without_admitting_client_clocks() {
    let clock = PortalClock::new(65_536.25, 1000).unwrap();
    assert_eq!(clock.at(1250).unwrap(), 65_536.5);
    assert_eq!(PortalClock::header_time(clock.at(1250).unwrap()), 0);
    assert!(clock.at(999).is_err());
    assert!(PortalClock::new(f64::NAN, 0).is_err());
    assert!(PortalClock::new(PortalClock::MAX_SECONDS + 1.0, 0).is_err());
    assert!(
        PortalClock::new(PortalClock::MAX_SECONDS, 0)
            .unwrap()
            .at(1)
            .is_err()
    );
    let worker = NetworkThread::spawn(NetworkThreadConfig {
        bind_address: "127.0.0.1:0".parse().unwrap(),
        portal_time_origin: 65_536.25,
        ..Default::default()
    })
    .unwrap();
    let client = socket();
    client
        .send_to(&login_packet(), worker.client_address)
        .unwrap();
    let key = match event(&worker) {
        NetworkEvent::Login { key, .. } => key,
        other => panic!("{other:?}"),
    };
    worker
        .try_send(NetworkCommand::Authenticated {
            key,
            account_id: bace_types::AccountId(1),
        })
        .unwrap();
    let mut bytes = [0; 1025];
    let (size, _) = client.recv_from(&mut bytes).unwrap();
    let datagram = Datagram::decode(&bytes[..size]).unwrap();
    let challenge = ConnectRequest::decode(datagram.body).unwrap();
    assert!((65_536.25..65_539.25).contains(&challenge.server_time));
    assert_eq!(
        datagram.header.time,
        PortalClock::header_time(challenge.server_time)
    );
    worker.shutdown().unwrap();
}
