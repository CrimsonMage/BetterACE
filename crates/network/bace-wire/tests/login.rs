use bace_wire::*;
fn payload(password: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[4, 0, b'1', b'8', b'0', b'2', 0, 0]);
    for value in [0u32, 2, 1, 123] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&[4, 0, b't', b'e', b's', b't', 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&((password.len() + 1) as u32).to_le_bytes());
    bytes.push(password.len() as u8);
    bytes.extend_from_slice(password);
    bytes
}
#[test]
fn clear_login_datagram_validates_hash_and_does_not_log_secret() {
    let payload = payload(b"private-fixture");
    let packet = encode_server_packet(
        PacketHeader {
            flags: flags::LOGIN_REQUEST,
            ..PacketHeader::default()
        },
        &payload,
        &[],
        0,
    )
    .unwrap();
    let decoded = LoginRequest::decode_datagram(&packet).unwrap();
    assert_eq!(decoded.account, "test");
    assert_eq!(
        decoded.credential,
        LoginCredential::Password("private-fixture".to_owned())
    );
    assert!(!format!("{decoded:?}").contains("private-fixture"));
    let mut corrupt = packet;
    corrupt[8] ^= 1;
    assert_eq!(
        LoginRequest::decode_datagram(&corrupt),
        Err(WireError::ChecksumMismatch)
    );
}
#[test]
fn login_truncation_and_invalid_utf8_are_rejected() {
    let bytes = payload(b"password");
    for length in 0..bytes.len() {
        assert!(
            LoginRequest::decode_payload(&bytes[..length]).is_err(),
            "truncation {length}"
        );
    }
    assert!(LoginRequest::decode_payload(&payload(&[0xff])).is_err());
    assert!(LoginRequest::decode_payload(&payload(&[0xf0, 0x9f, 0x99])).is_err());
    let mut enormous = payload(b"password");
    enormous[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(LoginRequest::decode_payload(&enormous).is_err());
    assert!(LoginRequest::decode_payload(&vec![0; 1005]).is_err());
}
#[test]
fn unknown_auth_type_is_preserved_without_assuming_password() {
    let mut bytes = payload(b"unused");
    bytes[12..16].copy_from_slice(&99u32.to_le_bytes());
    let parsed = LoginRequest::decode_payload(&bytes).unwrap();
    assert_eq!(parsed.net_auth_type, NetAuthType::Unknown(99));
    assert_eq!(parsed.credential, LoginCredential::None);
    assert!(parsed.trailing_bytes > 0);
}
