use super::*;
fn prepared(name: &str, args: &[&str]) -> bace_admin::PreparedShardCommand {
    prepare_shard_command(&AuthorizedCommand {
        spec: bace_admin::command_catalog::command(name).unwrap(),
        raw_arguments: args.join(" "),
        arguments: args.iter().map(|v| (*v).into()).collect(),
        sudo: false,
    })
    .unwrap()
    .unwrap()
}
#[test]
fn closed_world_preserves_source_staff_exception_but_shutdown_does_not() {
    let mut shard = ShardRuntime::new(false, None);
    for access in [
        AccessLevel::Player,
        AccessLevel::Advocate,
        AccessLevel::Sentinel,
        AccessLevel::Envoy,
        AccessLevel::Developer,
        AccessLevel::Admin,
    ] {
        assert_eq!(shard.accepts(access), access != AccessLevel::Player);
    }
    let clock = ShardClock {
        monotonic_millis: 0,
        unix_millis: 1_800_000_000_000,
        local_offset_seconds: 0,
    };
    shard
        .control
        .apply(&prepared("stop-now", &[]), ShardIssuer::HostConsole, clock)
        .unwrap();
    shard
        .control
        .advance(ShardClock {
            monotonic_millis: 1,
            ..clock
        })
        .unwrap();
    assert!(!shard.accepts(AccessLevel::Admin));
    assert!(!shard.accepts(AccessLevel::Player));
}
#[tokio::test]
async fn host_records_require_exact_ack_and_pressure_retains_original_event() {
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let clock = runtime.shard_clock().unwrap();
    runtime
        .shard
        .control
        .apply(
            &prepared("world", &["close"]),
            ShardIssuer::HostConsole,
            clock,
        )
        .unwrap();
    runtime.poll_shard_output(clock).unwrap();
    assert!(!runtime.shard.accepts(AccessLevel::Player));
    let sequence = runtime.pending_shard_host_record().unwrap().event.sequence;
    assert!(runtime.acknowledge_shard_host_record(sequence + 1).is_err());
    assert_eq!(
        runtime.pending_shard_host_record().unwrap().event.sequence,
        sequence
    );
    runtime.acknowledge_shard_host_record(sequence).unwrap();
    while let Some(record) = runtime.pending_shard_host_record() {
        runtime
            .acknowledge_shard_host_record(record.event.sequence)
            .unwrap();
    }
    // Fill the host sink, then verify that its upstream retains one exact event.
    for _ in 0..256 {
        runtime
            .shard
            .control
            .apply(&prepared("world", &[]), ShardIssuer::HostConsole, clock)
            .unwrap();
        runtime.poll_shard_output(clock).unwrap();
    }
    runtime
        .shard
        .control
        .apply(&prepared("world", &[]), ShardIssuer::HostConsole, clock)
        .unwrap();
    let original = runtime.shard.control.peek_event().unwrap().clone();
    runtime.poll_shard_output(clock).unwrap();
    assert_eq!(runtime.shard.control.peek_event(), Some(&original));
    let sequence = runtime.pending_shard_host_record().unwrap().event.sequence;
    runtime.acknowledge_shard_host_record(sequence).unwrap();
    runtime.poll_shard_output(clock).unwrap();
    assert!(runtime.shard.control.peek_event().is_none());
    assert_eq!(runtime.shard.host.back().unwrap().event, original);
    while let Some(record) = runtime.pending_shard_host_record() {
        runtime
            .acknowledge_shard_host_record(record.event.sequence)
            .unwrap();
    }
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn closed_connect_retains_error_and_waits_for_network_termination_before_retiring() {
    use bace_auth::{
        AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
    };
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("closed-shard").unwrap(),
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
            connected: false,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    runtime.shard = ShardRuntime::new(false, None);
    runtime.input.push_back(NetworkEvent::Connected { key });
    runtime.poll_ingress().unwrap();
    assert!(runtime.sessions[&key].closing && runtime.sessions[&key].terminated);
    assert!(runtime.login_queue.is_empty());
    let Some(NetworkCommand::TerminateAfterFlush {
        key: actual,
        queue: 9,
        bytes,
    }) = runtime.network_output.front()
    else {
        panic!("retained source close message")
    };
    assert_eq!(*actual, key);
    assert_eq!(
        *bytes,
        bace_wire::CharacterReply::Error(bace_wire::opcode::CharacterError::LogonServerFull)
            .encode()
            .unwrap()
    );
    runtime
        .poll_logout(Duration::ZERO, runtime.clock.unix_millis)
        .unwrap();
    assert!(
        runtime.login_queue.is_empty(),
        "flush admission cannot retire the owner"
    );
    runtime.input.push_back(NetworkEvent::Terminated {
        key,
        reason: crate::network::NetworkStopReason::Requested,
    });
    runtime.poll_ingress().unwrap();
    runtime
        .poll_logout(Duration::ZERO, runtime.clock.unix_millis)
        .unwrap();
    assert!(
        runtime
            .login_queue
            .iter()
            .any(|(k, action)| *k == key && matches!(action, PlayerIoAction::Retire))
    );
    assert!(
        runtime
            .network_output
            .iter()
            .any(|c| matches!(c,NetworkCommand::DrainCompleted {key:k} if *k==key))
    );
    runtime.quiesce(Duration::ZERO).unwrap();
}
