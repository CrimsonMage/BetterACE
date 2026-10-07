use super::*;
#[test]
fn concurrent_work_is_bounded_and_permits_release() {
    let service = PasswordService::new(1).unwrap();
    let permit = service.acquire().unwrap();
    assert!(matches!(service.acquire(), Err(AuthError::Busy)));
    assert_eq!(service.hash(b"synthetic-password"), Err(AuthError::Busy));
    drop(permit);
    assert!(service.acquire().is_ok());
}
