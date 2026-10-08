use std::process::Command;
struct Cluster {
    directory: tempfile::TempDir,
}
impl Cluster {
    fn start() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let data = directory.path().join("data");
        let output = Command::new("initdb")
            .args([
                "-A",
                "trust",
                "-U",
                "bace_test",
                "--no-locale",
                "-E",
                "UTF8",
                "-D",
            ])
            .arg(&data)
            .output()
            .expect("PostgreSQL initdb must be installed for database tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let options = format!("-h '' -k {}", directory.path().display());
        let output = Command::new("pg_ctl")
            .arg("-D")
            .arg(&data)
            .arg("-l")
            .arg(directory.path().join("log"))
            .args(["-o", &options, "-w", "start"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Self { directory }
    }
    fn url(&self) -> String {
        format!(
            "postgresql:///postgres?host={}&user=bace_test",
            self.directory.path().display()
        )
    }
}
impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new("pg_ctl")
            .arg("-D")
            .arg(self.directory.path().join("data"))
            .args(["-m", "immediate", "-w", "stop"])
            .output();
    }
}
use bace_auth::{
    AccessLevel, AccountAdminRepository, AccountName, CharacterPrivileges, StaffPrincipal,
};
use bace_db_postgres::PgStore;
use bace_runtime::{authentication::PasswordExecutor, staff_commands::*};
#[tokio::test]
async fn account_commands_preserve_exact_pending_write_through_cancel_and_recheck_authority() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let service = StaffAccountService::new(
        store.clone(),
        PasswordExecutor::new(1).unwrap(),
        1,
        AccessLevel::Player,
    )
    .unwrap();
    let PreparedStaffAccount::Write(mut create) = service
        .prepare(
            &StaffCommandIdentity::Host,
            "accountcreate Rune old-secret Admin",
            [1; 16],
        )
        .await
        .unwrap()
    else {
        panic!("write required")
    };
    assert!(matches!(
        service
            .prepare(&StaffCommandIdentity::Host, "accountget Rune", [0; 16])
            .await,
        Err(StaffCommandError::Busy)
    ));
    let account = create.commit().await.unwrap();
    assert_eq!(account.access, AccessLevel::Admin);
    assert_eq!(create.commit().await.unwrap(), account);
    let identity = StaffCommandIdentity::Game {
        account: account.account,
        name: AccountName::parse("Rune").unwrap(),
        principal: StaffPrincipal {
            account_access: AccessLevel::Admin,
            character: CharacterPrivileges::from_account(AccessLevel::Admin),
            in_world: true,
        },
    };
    assert!(matches!(
        service
            .prepare(&identity, "passwd wrong new-secret", [2; 16])
            .await,
        Err(StaffCommandError::Credentials)
    ));
    let PreparedStaffAccount::Write(mut password) = service
        .prepare(&identity, "passwd old-secret new-secret", [2; 16])
        .await
        .unwrap()
    else {
        panic!("write required")
    };
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR UPDATE")
        .bind(account.account.0 as i64)
        .execute(&mut *lock)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), password.commit())
            .await
            .is_err()
    );
    assert!(password.uncertain());
    assert!(matches!(
        service.prepare(&identity, "accountget Rune", [0; 16]).await,
        Err(StaffCommandError::Busy)
    ));
    lock.rollback().await.unwrap();
    let changed = password.commit().await.unwrap();
    assert_eq!(changed.revision, 2);
    assert!(!password.uncertain());
    let PreparedStaffAccount::Write(mut demote) = service
        .prepare(&identity, "set-accountaccess Rune Player", [3; 16])
        .await
        .unwrap()
    else {
        panic!("write required")
    };
    demote.commit().await.unwrap();
    assert!(matches!(
        service
            .prepare(&identity, "set-accountaccess Rune Admin", [4; 16])
            .await,
        Err(StaffCommandError::Forbidden)
    ));
    assert_eq!(
        store
            .account_for_admin(&AccountName::parse("Rune").unwrap())
            .await
            .unwrap()
            .unwrap()
            .account
            .access_level,
        AccessLevel::Player
    );
    pool.close().await;
    store.close().await;
}
