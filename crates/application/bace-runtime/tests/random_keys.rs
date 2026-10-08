#![cfg(unix)]
use bace_random::Domain;
use bace_runtime::random_keys::{initialize_random_key, load_random_key};
#[test]
fn key_provisioning_is_private_idempotent_and_never_reseeds_on_restart() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("random.key");
    let first = initialize_random_key(&path, 1).unwrap();
    let identity = first.root.character_identity(0x50000001).unwrap();
    let draw = first
        .root
        .rare_stream(identity, 77, Domain::RareOccurrence)
        .unwrap()
        .next_u64()
        .unwrap();
    let second = initialize_random_key(&path, 1).unwrap();
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(
        draw,
        second
            .root
            .rare_stream(identity, 77, Domain::RareOccurrence)
            .unwrap()
            .next_u64()
            .unwrap()
    );
    assert!(initialize_random_key(&path, 2).is_err());
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[14] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    assert!(load_random_key(&path).is_err());
}
