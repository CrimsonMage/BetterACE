use bace_session::*;
use bace_wire::ConnectRequest;

fn challenge(key: SessionKey) -> ConnectRequest {
    ConnectRequest {
        server_time: 0.0,
        cookie: 17,
        client_id: key.id.into(),
        server_seed: 1,
        client_seed: 2,
    }
}
#[test]
fn reused_slot_rejects_stale_authentication_and_drain_completion() {
    let mut sessions = SessionRegistry::new(1, 1).unwrap();
    let endpoint = "127.0.0.1:19000".parse().unwrap();
    let first = sessions.admit(endpoint, 0).unwrap();
    assert_eq!(
        sessions.drain_completed(first),
        Err(AdmissionError::DrainRequired)
    );
    sessions.terminate(first).unwrap();
    sessions.drain_completed(first).unwrap();
    let second = sessions.admit(endpoint, 1).unwrap();
    assert_eq!(first.id, second.id);
    assert_ne!(first.generation, second.generation);
    assert_eq!(
        sessions.verified_login(first, challenge(first), 2, 60_000),
        Err(AdmissionError::StaleSession)
    );
    assert_eq!(
        sessions.drain_completed(first),
        Err(AdmissionError::StaleSession)
    );
    assert_eq!(
        sessions.get(second).unwrap().lifecycle().state(),
        SessionState::AuthLoginRequest
    );
}
#[test]
fn endpoint_limits_expiration_and_validated_refresh() {
    let mut sessions = SessionRegistry::new(2, 1).unwrap();
    let endpoint = "127.0.0.1:19000".parse().unwrap();
    let key = sessions.admit(endpoint, 0).unwrap();
    assert_eq!(
        sessions.admit(endpoint, 0),
        Err(AdmissionError::EndpointInUse)
    );
    assert_eq!(
        sessions.admit("127.0.0.1:19001".parse().unwrap(), 0),
        Err(AdmissionError::AddressLimit)
    );
    assert!(
        sessions
            .route(key.id, "127.0.0.2:19000".parse().unwrap())
            .is_none()
    );
    sessions
        .verified_login(key, challenge(key), 1, 60_000)
        .unwrap();
    assert!(
        sessions
            .connect_response(17, "127.0.0.2:19001".parse().unwrap(), 2)
            .is_err()
    );
    sessions
        .connect_response(17, "127.0.0.1:19001".parse().unwrap(), 2)
        .unwrap();
    sessions.world_entry_committed(key).unwrap();
    sessions.validated_activity(key, 50_000).unwrap();
    assert!(sessions.expired(60_001).unwrap().is_empty());
    assert_eq!(sessions.expired(110_000).unwrap(), [key]);
    assert_eq!(
        sessions.len(),
        1,
        "expiry must retain world ownership until drain"
    );
    assert_eq!(
        sessions.validated_activity(key, 110_000),
        Err(AdmissionError::Lifecycle(SessionError::Expired))
    );
    sessions.terminate(key).unwrap();
    assert!(sessions.expired(110_001).unwrap().is_empty());
    sessions.drain_completed(key).unwrap();
    assert!(sessions.is_empty());
}
#[test]
fn clocks_and_failed_challenges_do_not_advance_lifecycle() {
    let mut session = SessionLifecycle::new("127.0.0.1".parse().unwrap(), 5).unwrap();
    assert_eq!(
        session.validated_activity(4),
        Err(SessionError::InvalidClock)
    );
    let deadline = session.deadline_ms();
    assert_eq!(
        session.validated_activity(deadline),
        Err(SessionError::Expired)
    );
    assert_eq!(session.deadline_ms(), deadline);
}

#[test]
fn restarted_network_owner_cannot_reuse_a_live_process_generation() {
    let endpoint = "127.0.0.1:19000".parse().unwrap();
    let old = SessionRegistry::new(1, 1)
        .unwrap()
        .admit(endpoint, 0)
        .unwrap();
    let mut replacement = SessionRegistry::new(1, 1).unwrap();
    let current = replacement.admit(endpoint, 0).unwrap();
    assert_eq!(old.id, current.id);
    assert_ne!(old.generation, current.generation);
    assert_eq!(
        replacement.verified_login(old, challenge(old), 1, 60_000),
        Err(AdmissionError::StaleSession)
    );
    assert_eq!(
        replacement.drain_completed(old),
        Err(AdmissionError::StaleSession)
    );
    assert_eq!(
        replacement.get(current).unwrap().lifecycle().state(),
        SessionState::AuthLoginRequest
    );
}
