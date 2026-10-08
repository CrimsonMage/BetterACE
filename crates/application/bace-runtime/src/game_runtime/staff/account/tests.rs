use super::*;
use bace_auth::{AccountAdminRepository, AccountName};
#[tokio::test]
async fn live_account_lane_waits_for_commit_and_retains_missing_private_recipient() {
    let (_cluster, _directory, mut runtime) = crate::game_runtime::tests::fixture::fixture().await;
    let service = StaffAccountService::new(
        runtime.bootstrap.store.clone(),
        crate::authentication::PasswordExecutor::new(1).unwrap(),
        1,
        bace_auth::AccessLevel::Player,
    )
    .unwrap();
    let PreparedStaffAccount::Write(write) = service
        .prepare(
            &StaffCommandIdentity::Host,
            "accountcreate staff-lane fixture-secret Player",
            [19; 16],
        )
        .await
        .unwrap()
    else {
        panic!("write")
    };
    let context = bace_gameplay_api::ActionContext {
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
        phase: Phase::Account(Box::new(AccountPending {
            kind: "accountcreate",
            phase: AccountPhase::Write(Some(write)),
        })),
    });
    assert!(
        runtime
            .bootstrap
            .store
            .account_for_admin(&AccountName::parse("staff-lane").unwrap())
            .await
            .unwrap()
            .is_none()
    );
    for _ in 0..200 {
        runtime.poll_staff_account().unwrap();
        if matches!(runtime.staff.pending.as_ref().map(|p|&p.phase),Some(Phase::Account(p)) if matches!(p.phase,AccountPhase::Response(_)))
        {
            break;
        }
        assert!(runtime.staff.output.is_empty());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        runtime
            .bootstrap
            .store
            .account_for_admin(&AccountName::parse("staff-lane").unwrap())
            .await
            .unwrap()
            .is_some()
    );
    assert!(runtime.poll_staff_account().is_err());
    assert!(runtime.staff.has_pending());
    assert!(
        runtime.staff.output.is_empty(),
        "no fabricated recipient or success packet"
    );
    // The test has no accepted player. The retained response remains observable.
    assert!(
        matches!(runtime.staff.pending.as_ref().map(|p|&p.phase),Some(Phase::Account(p)) if matches!(p.phase,AccountPhase::Response(_)))
    );
    runtime.staff.pending = None;
    runtime.quiesce(Duration::ZERO).unwrap();
}
