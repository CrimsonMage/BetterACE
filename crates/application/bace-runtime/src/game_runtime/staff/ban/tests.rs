use super::*;
use bace_auth::{AccountRepository, CreateAccountOutcome, NewAccount, PasswordService};
use bace_types::{AccountId, EntityId};

#[test]
fn source_duration_checks_each_component_and_retains_fractional_time() {
    let now = 1_000_000;
    assert_eq!(
        parsed_duration(&["0.5".into(), "1".into(), "0.25".into()], now).unwrap(),
        now + 43_200_000 + 3_600_000 + 15_000
    );
    for (input, expected) in [
        (["bad", "0", "0"], "Days must not be less than 0."),
        (["0", "-1", "0"], "Hours must not be less than 0."),
        (["0", "0", "NaN"], "Minutes must not be less than 0."),
    ] {
        assert_eq!(
            parsed_duration(&input.map(str::to_owned), now).unwrap_err(),
            expected
        );
    }
}

#[test]
fn pinned_banlist_row_uses_server_local_date_and_original_reason() {
    let entry = AccountBanListEntry {
        canonical_name: "AccountName".into(),
        issuer_canonical_name: Some("Sentinel".into()),
        ban: bace_persistence::AccountBanRecord {
            account_id: 7,
            account_revision: 9,
            started_unix_millis: 0,
            expires_unix_millis: 1_704_067_200_000,
            issuer_account_id: Some(3),
            reason: Some("repeated abuse".into()),
        },
    };
    assert_eq!(
        banlist_row(&entry, -5 * 3600).unwrap(),
        "AccountName -- banned by account Sentinel until server time Dec 31 2023  7:00PM -- Reason: repeated abuse"
    );
}

#[tokio::test]
async fn ban_waits_for_durable_receipt_before_audit_and_private_output() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let name = AccountName::parse("ban-receipt").unwrap();
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: name.clone(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"ban-fixture")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let versioned = runtime
        .bootstrap
        .store
        .account_for_admin(&name)
        .await
        .unwrap()
        .unwrap();
    let now = 1_704_067_200_000;
    let operation = AccountBanOperation {
        operation_id: [27; 16],
        account_id: account.id.0,
        expected_revision: versioned.revision,
        issuer_account_id: Some(account.id.0),
        change: AccountBanChange::Ban {
            started_unix_millis: now,
            expires_unix_millis: now + 300_000,
            reason: Some("repeated abuse".into()),
        },
    };
    runtime.staff.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(account.id.0),
            session: bace_gameplay_api::SessionId(7),
            sequence: 1,
        },
        token: 27,
        phase: Phase::Ban(Box::new(BanPending {
            mode: BanMode::Ban {
                name: name.as_str().into(),
                duration: ["0".into(), "0".into(), "5".into()],
                reason: Some("repeated abuse".into()),
            },
            sudo: false,
            now,
            state: BanState::Write(operation.clone()),
            audit: VecDeque::new(),
            responses: Vec::new(),
            boot: None,
        })),
    });
    assert!(matches!(
        runtime
            .bootstrap
            .store
            .account_ban_verdict(account.id.0, now)
            .await
            .unwrap(),
        AccountBanVerdict::Allowed
    ));
    runtime.poll_staff_ban().unwrap();
    assert!(
        matches!(runtime.staff.pending.as_ref().map(|p| &p.phase), Some(Phase::Ban(b)) if matches!(b.state, BanState::Writing { .. }))
    );
    assert!(runtime.staff.output.is_empty());
    for _ in 0..200 {
        runtime.poll_staff_ban().unwrap();
        if matches!(runtime.staff.pending.as_ref().map(|p| &p.phase), Some(Phase::Ban(b)) if matches!(b.state, BanState::AuditSubmit))
        {
            break;
        }
        assert!(runtime.staff.output.is_empty());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        matches!(runtime.staff.pending.as_ref().map(|p| &p.phase), Some(Phase::Ban(b)) if matches!(b.state, BanState::AuditSubmit) && b.audit.len() == 1)
    );
    assert!(matches!(
        runtime
            .bootstrap
            .store
            .account_ban_verdict(account.id.0, now)
            .await
            .unwrap(),
        AccountBanVerdict::Banned(_)
    ));
    assert!(runtime.staff.output.is_empty());
    runtime.staff.pending = None;
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}

#[tokio::test]
async fn uncertain_ban_commit_retains_exact_operation_for_retry() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let operation = AccountBanOperation {
        operation_id: [29; 16],
        account_id: 8,
        expected_revision: 1,
        issuer_account_id: Some(3),
        change: AccountBanChange::Unban,
    };
    runtime.staff.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(3),
            session: bace_gameplay_api::SessionId(7),
            sequence: 1,
        },
        token: 29,
        phase: Phase::Ban(Box::new(BanPending {
            mode: BanMode::Unban {
                name: "Target".into(),
            },
            sudo: false,
            now: 1,
            state: BanState::Writing {
                operation: operation.clone(),
                job: Box::pin(async { Err(StoreError::CommitUncertain(sqlx::Error::PoolClosed)) }),
            },
            audit: VecDeque::new(),
            responses: Vec::new(),
            boot: None,
        })),
    });
    assert!(runtime.poll_staff_ban().unwrap_err().contains("uncertain"));
    let Some(Phase::Ban(ban)) = runtime.staff.pending.as_ref().map(|p| &p.phase) else {
        panic!("retained ban")
    };
    let BanState::Write(retained) = &ban.state else {
        panic!("exact retry phase")
    };
    assert_eq!(retained, &operation);
    assert!(runtime.staff.output.is_empty());
    runtime.staff.pending = None;
    runtime.retry_staff();
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}

#[tokio::test]
async fn disconnected_issuer_and_replaced_target_generation_receive_no_stale_output() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let name = AccountName::parse("ban-generation").unwrap();
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: name.clone(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"ban-fixture")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let issuer = SessionKey {
        id: 1,
        generation: 7,
    };
    let old_target = SessionKey {
        id: 2,
        generation: 8,
    };
    let replacement = SessionKey {
        id: 2,
        generation: 9,
    };
    runtime.sessions.insert(
        issuer,
        Session {
            account: account.clone(),
            connected: false,
            closing: true,
            terminated: false,
            disconnected: true,
            loading: None,
            failure: None,
        },
    );
    runtime.sessions.insert(
        replacement,
        Session {
            account: account.clone(),
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    let pending = Pending {
        key: issuer,
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(account.id.0),
            session: bace_gameplay_api::SessionId(7),
            sequence: 1,
        },
        token: 30,
        phase: Phase::Awaiting,
    };
    let ban = BanPending {
        mode: BanMode::Ban {
            name: name.as_str().into(),
            duration: ["0".into(), "0".into(), "5".into()],
            reason: None,
        },
        sudo: false,
        now: 1,
        state: BanState::Response,
        audit: VecDeque::new(),
        responses: vec![
            format!("Booting account {}.", name.as_str()),
            format!(
                "Banned account {} for 0 days, 0 hours and 5 minutes.",
                name.as_str()
            ),
        ],
        boot: Some((old_target, account.id)),
    };
    runtime.publish_ban_response(&pending, &ban).unwrap();
    assert!(runtime.staff.output.is_empty());
    assert!(!runtime.sessions.get(&replacement).unwrap().terminated);
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}

#[tokio::test]
async fn one_audit_receipt_completes_both_online_ban_lines_once() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let context = ActionContext {
        actor: EntityId(1),
        account: AccountId(3),
        session: bace_gameplay_api::SessionId(7),
        sequence: 1,
    };
    runtime.staff.pending = Some(Pending {
        key: SessionKey {
            id: 1,
            generation: 7,
        },
        context,
        token: 31,
        phase: Phase::Ban(Box::new(BanPending {
            mode: BanMode::Ban {
                name: "Target".into(),
                duration: ["0".into(), "0".into(), "5".into()],
                reason: None,
            },
            sudo: false,
            now: 1,
            state: BanState::AuditAwait,
            audit: VecDeque::from([
                "Booting account Target.".into(),
                "Banned account Target for 0 days, 0 hours and 5 minutes.".into(),
            ]),
            responses: vec![
                "Booting account Target.".into(),
                "Banned account Target for 0 days, 0 hours and 5 minutes.".into(),
            ],
            boot: None,
        })),
    });
    assert!(
        runtime
            .accept_staff_ban_audit_outcome(&StaffEvent::Outcome {
                token: 31,
                actor: Some(context.actor),
                result: Ok(()),
            })
            .unwrap()
    );
    let Some(Phase::Ban(ban)) = runtime.staff.pending.as_ref().map(|p| &p.phase) else {
        panic!("ban phase")
    };
    assert!(matches!(ban.state, BanState::Response));
    assert!(ban.audit.is_empty());
    assert!(runtime.staff.output.is_empty());
    runtime.staff.pending = None;
    runtime.shutdown_staff_workers().unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
