use super::*;
pub(in crate::game_runtime) mod entered;
pub(in crate::game_runtime) mod fixture;
#[tokio::test]
async fn unsupported_input_closes_peer_while_unmatched_owner_outcomes_remain_held() {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    let (_cluster, _directory, mut runtime) = fixture::fixture().await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("driver-retention").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let key = SessionKey {
        id: 7,
        generation: 11,
    };
    runtime.players.authenticated(key, &account).unwrap();
    runtime.sessions.insert(
        key,
        Session {
            account,
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    let unknown = NetworkEvent::Message {
        key,
        message: bace_transport::ReceivedMessage {
            sequence: 1,
            id: 1,
            queue: 9,
            bytes: vec![0xff; 4],
        },
    };
    runtime.input.push_back(unknown);
    runtime.input_bytes = 4;
    runtime.poll_ingress().unwrap();
    assert!(
        runtime.input.is_empty(),
        "unaccepted unknown packets cannot occupy the shared ingress queue indefinitely"
    );
    assert!(runtime.sessions[&key].terminated);
    assert!(
        runtime.sessions[&key].failure.is_none(),
        "rejection cannot poison the durable logout owner"
    );
    runtime.input.push_back(NetworkEvent::Terminated {
        key,
        reason: crate::network::NetworkStopReason::PeerDisconnected,
    });
    runtime.poll_ingress().unwrap();
    assert!(runtime.sessions[&key].terminated);
    assert!(runtime.sessions[&key].disconnected);
    runtime.poll_ingress().unwrap();
    assert!(
        runtime.input.is_empty(),
        "only unaccepted input is cancelled"
    );
    assert_eq!(runtime.input_bytes, 0);
    let world = runtime.world.as_mut().unwrap();
    world.pending = Some(bace_simulation::GeneratorCommandOutcome {
        correlation: 999,
        admission: None,
        request: None,
        result: Err(bace_simulation::GeneratorServiceError::Invalid),
    });
    assert!(
        runtime
            .poll_world(Duration::ZERO, 1_800_000_000_000)
            .is_err()
    );
    assert_eq!(
        runtime
            .world
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .correlation,
        999
    );
    runtime.bootstrap.world_owner.check().await.unwrap();
    // Exact injected unknown read-result remains available; neither output value
    // nor error may be mistaken for the routine capture currently owned elsewhere.
    runtime.unexpected_snapshot = Some(bace_simulation::PlayerSnapshotOutcome {
        correlation: 123,
        result: Err(bace_simulation::CharacterRegistrationError::DurabilityPending),
    });
    assert!(
        runtime
            .poll_durability(Duration::ZERO, 1_800_000_000_000)
            .is_err()
    );
    assert_eq!(
        runtime.unexpected_snapshot.as_ref().unwrap().correlation,
        123
    );
    runtime.world.as_mut().unwrap().pending = None;
    runtime.unexpected_snapshot = None;
    // The test injected no gameplay owner. Explicitly quiesce the empty service
    // rather than treating constructor success as a durability receipt.
    runtime.quiesce(Duration::ZERO).unwrap();
    for _ in 0..80 {
        let _ = runtime.poll(runtime.clock.monotonic.elapsed());
        tokio::time::sleep(Duration::from_millis(5)).await;
        if runtime.sessions.is_empty() && runtime.regions_quiesced {
            break;
        }
    }
    assert!(runtime.sessions.is_empty());
    assert!(
        runtime.regions_quiesced,
        "quiesce={:?}, failure={:?}, world_pending={:?}, login={:?}/{}, preparation={}, requests={}",
        runtime.regions_quiesce,
        runtime.failure,
        runtime
            .world
            .as_ref()
            .and_then(|w| w.pending.as_ref())
            .map(|p| (p.correlation, p.result)),
        runtime.login_key,
        runtime.login_queue.len(),
        runtime.preparation.pending(),
        runtime.request_regions.len()
    );
}
