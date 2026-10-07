use bace_session::*;
use bace_wire::ConnectRequest;
#[test]
fn verified_login_cookie_and_address_bind_endpoint() {
    let address = "127.0.0.1".parse().unwrap();
    let mut session = SessionLifecycle::new(address, 0).unwrap();
    let request = ConnectRequest {
        server_time: 1.0,
        cookie: 123,
        client_id: 7,
        server_seed: 8,
        client_seed: 9,
    };
    session.begin_verified_login(request, 1, 60_000).unwrap();
    assert_eq!(
        session.accept_connect_response(122, "127.0.0.1:9001".parse().unwrap(), 2),
        Err(SessionError::BadCookie)
    );
    assert_eq!(
        session.accept_connect_response(123, "127.0.0.2:9001".parse().unwrap(), 2),
        Err(SessionError::WrongAddress)
    );
    assert_eq!(session.state(), SessionState::AuthConnectResponse);
    session
        .accept_connect_response(123, "127.0.0.1:9001".parse().unwrap(), 2)
        .unwrap();
    assert_eq!(session.state(), SessionState::AuthConnected);
    assert!(
        session
            .accept_connect_response(123, "127.0.0.1:9001".parse().unwrap(), 2)
            .is_err()
    );
    session.world_entry_committed().unwrap();
    session.terminate();
    assert!(!session.allows_flags(0));
}
#[test]
fn unverified_and_expired_sessions_cannot_connect() {
    let mut session = SessionLifecycle::new("127.0.0.1".parse().unwrap(), 0).unwrap();
    assert_eq!(
        session.accept_connect_response(1, "127.0.0.1:9001".parse().unwrap(), 1),
        Err(SessionError::WrongState)
    );
    assert_eq!(
        session.accept_connect_response(1, "127.0.0.1:9001".parse().unwrap(), 15_000),
        Err(SessionError::Expired)
    );
    assert!(session.world_entry_committed().is_err());
}
#[test]
fn parsed_credentials_still_require_account_verification() {
    use bace_wire::{LoginCredential, LoginRequest, NetAuthType};
    let mut request = LoginRequest {
        client_version: "1802".to_owned(),
        declared_length: 0,
        net_auth_type: NetAuthType::AccountPassword,
        auth_flags: 2,
        timestamp: 0,
        account: "player".to_owned(),
        account_to_login_as: "administrator".to_owned(),
        credential: LoginCredential::Password("synthetic-secret".to_owned()),
        trailing_bytes: 0,
    };
    let login = validate_password_login(&request).unwrap();
    assert_eq!(login.account, "player");
    assert!(!format!("{login:?}").contains("synthetic-secret"));
    request.client_version = "old".to_owned();
    assert!(matches!(
        validate_password_login(&request),
        Err(LoginRejection::WrongClientVersion)
    ));
    request.client_version = "1802".to_owned();
    request.net_auth_type = NetAuthType::GlsTicket;
    assert!(matches!(
        validate_password_login(&request),
        Err(LoginRejection::UnsupportedAuthentication)
    ));
}
