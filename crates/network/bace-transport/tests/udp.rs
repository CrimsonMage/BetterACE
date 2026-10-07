use bace_transport::{EndpointKind, UdpEndpoints};
use std::{
    io,
    net::UdpSocket,
    time::{Duration, Instant},
};

fn endpoints() -> UdpEndpoints {
    // Ephemeral adjacency is not guaranteed by the OS. Retry collisions only.
    for _ in 0..32 {
        match UdpEndpoints::bind("127.0.0.1:0".parse().unwrap()) {
            Ok(endpoints) => return endpoints,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => panic!("bind failed: {error}"),
        }
    }
    panic!("no adjacent ephemeral UDP ports available")
}
fn receive(
    endpoints: &UdpEndpoints,
    kind: EndpointKind,
    buffer: &mut [u8; 1025],
) -> io::Result<(std::net::SocketAddr, usize)> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match endpoints.receive(kind, buffer)? {
            Some(packet) => return Ok(packet),
            None if Instant::now() < deadline => std::thread::yield_now(),
            None => panic!("loopback datagram not received"),
        }
    }
}
#[test]
fn paired_udp_ports_receive_and_send_on_correct_source_port() {
    let endpoints = endpoints();
    assert_eq!(
        endpoints.local_address(EndpointKind::Server).port(),
        endpoints.local_address(EndpointKind::Client).port() + 1
    );
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut buffer = [0; 1025];
    assert!(
        endpoints
            .receive(EndpointKind::Client, &mut buffer)
            .unwrap()
            .is_none()
    );
    for kind in [EndpointKind::Client, EndpointKind::Server] {
        client
            .send_to(&[1, 2, 3], endpoints.local_address(kind))
            .unwrap();
        let (source, length) = receive(&endpoints, kind, &mut buffer).unwrap();
        assert_eq!(source, client.local_addr().unwrap());
        assert_eq!(&buffer[..length], &[1, 2, 3]);
        endpoints.send_to(kind, source, &[4, 5]).unwrap();
        let (length, source) = client.recv_from(&mut buffer).unwrap();
        assert_eq!(source, endpoints.local_address(kind));
        assert_eq!(&buffer[..length], &[4, 5]);
    }
}
#[test]
fn oversized_udp_input_is_consumed_and_output_is_rejected() {
    let endpoints = endpoints();
    let client = UdpSocket::bind("127.0.0.1:0").unwrap();
    client
        .send_to(&[0; 2048], endpoints.local_address(EndpointKind::Client))
        .unwrap();
    let mut buffer = [0; 1025];
    assert_eq!(
        receive(&endpoints, EndpointKind::Client, &mut buffer)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    client
        .send_to(&[0; 1024], endpoints.local_address(EndpointKind::Client))
        .unwrap();
    assert_eq!(
        receive(&endpoints, EndpointKind::Client, &mut buffer)
            .unwrap()
            .1,
        1024
    );
    assert_eq!(
        endpoints
            .send_to(
                EndpointKind::Server,
                client.local_addr().unwrap(),
                &[0; 485]
            )
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
}
#[test]
fn failed_companion_bind_releases_first_socket() {
    let endpoints = endpoints();
    let base = endpoints.local_address(EndpointKind::Client);
    let companion = endpoints.local_address(EndpointKind::Server);
    drop(endpoints);
    let held = UdpSocket::bind(companion).unwrap();
    assert!(UdpEndpoints::bind(base).is_err());
    let first = UdpSocket::bind(base).unwrap();
    drop(first);
    drop(held);
}
