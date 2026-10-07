use bace_auth::*;
#[test]
fn fresh_argon2id_hashes_use_unique_salts_and_verify() {
    let service = PasswordService::new(2).unwrap();
    let first = service.hash(b"synthetic-fixture-password").unwrap();
    let second = service.hash(b"synthetic-fixture-password").unwrap();
    assert_ne!(first, second);
    assert!(
        first
            .as_phc()
            .starts_with("$argon2id$v=19$m=19456,t=2,p=1$")
    );
    assert!(
        service
            .verify(b"synthetic-fixture-password", &first)
            .unwrap()
    );
    assert!(!service.verify(b"wrong-password", &first).unwrap());
    assert_eq!(PasswordHashRecord::parse(first.as_phc()).unwrap(), first);
    assert!(!format!("{first:?}").contains(first.as_phc()));
    let mut account = AccountRecord {
        id: bace_types::AccountId(1),
        name: AccountName::parse("fixture").unwrap(),
        password_hash: first,
        access_level: AccessLevel::Player,
        disabled: true,
    };
    assert!(
        !service
            .verify_account(b"synthetic-fixture-password", &account)
            .unwrap()
    );
    account.disabled = false;
    assert!(
        service
            .verify_account(b"synthetic-fixture-password", &account)
            .unwrap()
    );
}
#[test]
fn malformed_or_expensive_hashes_fail_before_memory_work() {
    let salt = "MDEyMzQ1Njc4OWFiY2RlZg";
    let hash = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    for bad in [
        format!("$argon2id$v=19$m=4294967295,t=2,p=1${salt}${hash}"),
        format!("$argon2id$v=19$m=19456,t=999,p=1${salt}${hash}"),
        format!("$argon2id$v=19$m=19456,t=2,p=999${salt}${hash}"),
        format!("$argon2id$v=19$m=8,t=1,p=1${salt}${hash}"),
        format!("$argon2i$v=19$m=19456,t=2,p=1${salt}${hash}"),
        format!("$argon2id$v=16$m=19456,t=2,p=1${salt}${hash}"),
        format!("$argon2id$v=19$m=19456,t=2${salt}${hash}"),
        "x".repeat(513),
    ] {
        assert!(PasswordHashRecord::parse(&bad).is_err());
    }
    let service = PasswordService::new(1).unwrap();
    assert_eq!(service.hash(&[]), Err(AuthError::InvalidPasswordLength));
    assert_eq!(
        service.hash(&vec![1; 1025]),
        Err(AuthError::InvalidPasswordLength)
    );
    assert!(PasswordService::new(0).is_err());
    assert!(PasswordService::new(17).is_err());
}
#[test]
fn canonical_identity_is_bounded_and_never_silently_trimmed() {
    assert_eq!(
        AccountName::parse("TestAccount").unwrap().as_str(),
        "testaccount"
    );
    assert_eq!(AccountName::parse("Élodie").unwrap().as_str(), "élodie");
    assert_eq!(AccountName::parse(" spaced ").unwrap().as_str(), " spaced ");
    for name in [
        "".to_owned(),
        "x".repeat(51),
        "a\0b".to_owned(),
        "🙂".repeat(26),
    ] {
        assert!(AccountName::parse(&name).is_err());
    }
    assert_eq!(AccessLevel::try_from(5).unwrap(), AccessLevel::Admin);
    assert!(AccessLevel::try_from(6).is_err());
}
