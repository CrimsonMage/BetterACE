use super::*;
#[tokio::test]
#[ignore = "requires approved DATs and PostgreSQL binaries"]
async fn unavailable_staff_text_responds_privately_without_disconnecting() {
    let mut fixture = Box::pin(crate::game_runtime::tests::entered::fixture()).await;
    let context = ActionContext {
        actor: fixture.binding.actor,
        account: fixture.binding.account,
        session: fixture.binding.session,
        sequence: 31,
    };
    for line in ["@teleloc 0x01010000", "@sudo teleloc 0x01010000", "@deaf"] {
        assert!(matches!(
            fixture.runtime.handle_staff_dispatch(
                fixture.key,
                crate::gameplay_dispatch::GameplayDispatch::StaffLine {
                    context,
                    line: line.into(),
                },
            ),
            Ok(crate::game_runtime::social::SocialIngress::Accepted)
        ));
        assert!(fixture.runtime.staff.pending.is_none());
        assert!(!fixture.runtime.sessions[&fixture.key].terminated);
        let Some(NetworkCommand::SendOrderedBatch { key, messages }) =
            fixture.runtime.staff.output.pop_front()
        else {
            panic!("missing private unsupported response")
        };
        assert_eq!(key, fixture.key);
        assert_eq!(
            messages,
            vec![(
                9,
                bace_wire::ChatMessage::System {
                    text: "This command is unsupported.",
                    chat_type: 0,
                }
                .encode()
                .unwrap(),
            )]
        );
    }
    fixture.shutdown().await;
}
#[tokio::test]
async fn staff_logout_cancels_only_unsubmitted_work() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    use bace_auth::{
        AccessLevel, AccountName, AccountRepository, CharacterPrivileges, CreateAccountOutcome,
        NewAccount, PasswordService, StaffPrincipal,
    };
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("staff-logout-cancel").unwrap(),
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
        id: 1,
        generation: 7,
    };
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
        sequence: 1,
    };
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
    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 11,
        phase: Phase::Capture(StaffRequest::Line("@regen".into())),
    });
    assert!(!runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_some());
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    assert!(runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_none());

    let principal = StaffPrincipal {
        account_access: AccessLevel::Envoy,
        character: CharacterPrivileges::from_account(AccessLevel::Envoy),
        in_world: true,
    };
    let command = bace_admin::authorize_command(
        bace_admin::parse_command("@trophies").unwrap(),
        Some(principal),
    )
    .unwrap();
    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 12,
        phase: Phase::Other {
            line: "@trophies".into(),
            command,
            principal,
        },
    });
    assert!(runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_none());

    let map_request = MapTeleportRequest {
        cell: 0x0101_0000,
        origin: [1.0, 2.0, 3.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };
    let map_work = StaffMapWork {
        token: 13,
        request: map_request,
        expected_epoch: 1,
    };
    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 13,
        phase: Phase::MapPrepare(map_work),
    });
    assert!(runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_none());

    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 14,
        phase: Phase::MapPreparing(map_work),
    });
    assert!(!runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_some());

    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 15,
        phase: Phase::Awaiting,
    });
    assert!(!runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_some());
    runtime.staff.pending = None;
    runtime.sessions.get_mut(&key).unwrap().terminated = false;
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime.staff.pending = Some(Pending {
        key,
        context,
        token: 16,
        phase: Phase::Capture(StaffRequest::Line("@regen".into())),
    });
    assert!(runtime.cancel_staff_unsubmitted_for_logout(key));
    assert!(runtime.staff.pending.is_none());
    runtime.sessions.remove(&key);
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn broadcast_log_obligation_is_exact_bounded_and_survives_network_fanout() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    runtime.configure_staff_broadcast_logging(true).unwrap();
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
        sequence: 1,
    };
    let event = StaffEvent::Broadcast {
        token: 9,
        context,
        sender: "Staff".into(),
        text: "Broadcast from Staff> hello".into(),
        recipients: vec![bace_gameplay_api::CharacterBinding {
            actor: context.actor,
            account: context.account,
            session: context.session,
        }],
    };
    assert!(runtime.retain_staff_broadcast(&event).unwrap());
    let record = runtime.peek_staff_broadcast_record().unwrap().clone();
    let mut wrong = record.clone();
    wrong.token += 1;
    assert!(!runtime.acknowledge_staff_broadcast_record(&wrong));
    // No replacement or missing session receives a stale accepted audience message.
    assert!(!runtime.poll_staff_broadcast().unwrap());
    assert!(runtime.staff.output.is_empty());
    assert_eq!(runtime.peek_staff_broadcast_record(), Some(&record));
    assert!(runtime.shutdown_staff_workers().is_err());
    assert!(runtime.acknowledge_staff_broadcast_record(&record));
    for _ in 0..256 {
        assert!(runtime.retain_staff_broadcast(&event).unwrap());
        assert!(!runtime.poll_staff_broadcast().unwrap());
    }
    assert!(!runtime.retain_staff_broadcast(&event).unwrap());
    while let Some(record) = runtime.peek_staff_broadcast_record().cloned() {
        assert!(runtime.acknowledge_staff_broadcast_record(&record));
    }
    runtime.configure_staff_broadcast_logging(false).unwrap();
    assert!(runtime.retain_staff_broadcast(&event).unwrap());
    assert!(!runtime.poll_staff_broadcast().unwrap());
    assert!(runtime.peek_staff_broadcast_record().is_none());
    let log_directory = tempfile::tempdir().unwrap();
    let logs = Arc::new(bace_observability::LogStore::open(log_directory.path()).unwrap());
    runtime.attach_staff_broadcast_log(logs.clone()).unwrap();
    runtime.configure_staff_broadcast_logging(true).unwrap();
    assert!(runtime.retain_staff_broadcast(&event).unwrap());
    runtime.poll_staff_broadcast_log().unwrap();
    assert!(runtime.peek_staff_broadcast_record().is_none());
    assert!(!runtime.poll_staff_broadcast().unwrap());
    for _ in 0..100 {
        if !logs.recent(0).records.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(
        logs.recent(0).records[0].message,
        "[CHAT][GLOBAL] Staff issued a world broadcast, \"Broadcast from Staff> hello\""
    );
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn staff_output_retains_unknown_receipt_and_observer_batch_until_exact_handoff() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let context = ActionContext {
        actor: bace_types::EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
        sequence: 1,
    };
    runtime.staff.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context,
        token: 9,
        phase: Phase::Awaiting,
    });
    runtime.staff.event = Some(StaffEvent::Outcome {
        token: 10,
        actor: Some(context.actor),
        result: Ok(()),
    });
    assert!(runtime.poll_staff_output().is_err());
    assert!(runtime.staff.event.is_some());
    assert!(runtime.staff.pending.is_some());
    runtime.staff.event = Some(StaffEvent::Scripts {
        context,
        targets: vec![(context.actor, 28), (context.actor, 29)],
    });
    runtime.poll_staff_output().unwrap();
    let work = runtime.pending_reward_observers().unwrap();
    assert_eq!(work.actor, context.actor);
    assert_eq!(work.messages.len(), 2);
    let sequence = work.sequence;
    assert!(runtime.acknowledge_reward_observers(sequence + 1).is_err());
    assert_eq!(
        runtime.pending_reward_observers().unwrap().sequence,
        sequence
    );
    runtime.staff.event = Some(StaffEvent::Outcome {
        token: 9,
        actor: Some(context.actor),
        result: Ok(()),
    });
    runtime.poll_staff_output().unwrap();
    assert!(runtime.staff.pending.is_none());
    assert!(runtime.requires_drain());
    runtime.acknowledge_reward_observers(sequence).unwrap();
    assert!(runtime.pending_reward_observers().is_none());
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
