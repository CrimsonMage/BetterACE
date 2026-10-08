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
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_db_postgres::PgStore;
use bace_persistence::{OwnershipState, SaveSnapshot};
use bace_runtime::{
    game_lifecycle::GameLoginPhase, game_login::*, game_messages::*, network::NetworkCommand,
};
use bace_session::SessionKey;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};
use bace_wire::{CharacterList, CharacterReply};
async fn new_account(store: &PgStore, name: &str) -> bace_auth::AccountRecord {
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse(name).unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("fresh account")
    };
    account
}
fn player(id: u32, account: u64) -> PlayerSaveV1 {
    PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "synthetic_player".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account,
        name: "Synthetic Player".into(),
        quests: vec![],
        metadata: Default::default(),
    }
}
#[tokio::test]
async fn roster_creation_ownership_fenced_load_abort_admission_and_durable_logout() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 4).await.unwrap();
    store.migrate().await.unwrap();
    let account = new_account(&store, "synthetic-one").await;
    let other = new_account(&store, "synthetic-two").await;
    let key = SessionKey {
        id: 1,
        generation: 100,
    };
    let outsider = SessionKey {
        id: 2,
        generation: 101,
    };
    let mut service = GameLoginService::new(store.clone(), 2, 11).unwrap();
    service.register_authenticated(key, &account).unwrap();
    service.register_authenticated(outsider, &other).unwrap();
    let NetworkCommand::Send { bytes, queue, .. } = roster_command(&service, key).await.unwrap()
    else {
        panic!("roster send")
    };
    assert_eq!(queue, 9);
    assert!(
        CharacterList::decode(&bytes, 11, 100)
            .unwrap()
            .characters
            .is_empty()
    );
    let id = service.allocate_character_id(key).await.unwrap();
    let mut prepared = player(id, account.id.0);
    prepared.account_id = other.id.0;
    assert!(matches!(
        service
            .create_prepared(
                key,
                PreparedCharacter {
                    player: &prepared,
                    slot: 0,
                    items: &[]
                }
            )
            .await,
        Err(GameLoginError::InvalidPreparation)
    ));
    prepared.account_id = account.id.0;
    let item = EntitySaveV1 {
        object_id: 0x80000001,
        ..prepared.entity.clone()
    };
    let items = [(item.clone(), 0)];
    let result = service
        .create_prepared(
            key,
            PreparedCharacter {
                player: &prepared,
                slot: 0,
                items: &items,
            },
        )
        .await
        .unwrap();
    assert_eq!(result.lease.state, OwnershipState::Offline);
    let NetworkCommand::Send { bytes, .. } = creation_command(&result).unwrap() else {
        panic!()
    };
    assert_eq!(
        bytes,
        CharacterReply::Created {
            object_id: id,
            name: prepared.name.clone()
        }
        .encode()
        .unwrap()
    );
    let NetworkCommand::Send { bytes, .. } = roster_command(&service, key).await.unwrap() else {
        panic!()
    };
    assert_eq!(
        CharacterList::decode(&bytes, 11, 100).unwrap().characters[0].object_id,
        id
    );
    assert!(matches!(
        service.load_character(outsider, id).await,
        Err(GameLoginError::NotOwned)
    ));
    let loaded = service.load_character(key, id).await.unwrap();
    assert_eq!(loaded.player.player, prepared);
    assert_eq!(loaded.inventory.len(), 1);
    assert_eq!(loaded.inventory[0].entity, item);
    assert_eq!(service.phase(key).unwrap(), GameLoginPhase::AwaitingWorld);
    assert!(matches!(
        service.retire_roster(key),
        Err(GameLoginError::DrainRequired)
    ));
    assert!(service.load_character(key, id).await.is_err());
    let offline = service.abort_loading(key).await.unwrap();
    assert_eq!(offline.state, OwnershipState::Offline);
    let loaded = service.load_character(key, id).await.unwrap();
    assert!(
        service
            .world_admitted(outsider, loaded.binding)
            .await
            .is_err()
    );
    let online = service.world_admitted(key, loaded.binding).await.unwrap();
    assert_eq!(online.state, OwnershipState::Online);
    // Synthetic test owner receipt only: production must supply admitted geometry.
    assert_eq!(service.online_lease(key).unwrap(), online);
    let draining = store.begin_logout(online).await.unwrap();
    prepared.entity.mutation_revision += 1;
    let (offline, _) = store
        .finish_logout(
            draining,
            &SaveSnapshot {
                object_id: id,
                mutation_revision: prepared.entity.mutation_revision,
                expected_version: loaded.persisted_version,
                bytes: prepared.encode().unwrap(),
            },
        )
        .await
        .unwrap();
    service.logout_drained(key, offline).await.unwrap();
    service.retire_roster(key).unwrap();
    let next = SessionKey {
        id: 1,
        generation: 102,
    };
    service.register_authenticated(next, &account).unwrap();
    assert!(matches!(
        service.roster(key).await,
        Err(GameLoginError::StaleSession)
    ));
    let loaded = service.load_character(next, id).await.unwrap();
    assert_eq!(loaded.player.player.entity.mutation_revision, 2);
    service.abort_loading(next).await.unwrap();
    assert_eq!(
        store
            .item_location(item.object_id)
            .await
            .unwrap()
            .unwrap()
            .container,
        id
    );
    store.close().await;
}
#[tokio::test]
async fn refused_wire_names_and_duplicate_slots_never_report_created() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 2).await.unwrap();
    store.migrate().await.unwrap();
    let account = new_account(&store, "synthetic-three").await;
    let key = SessionKey {
        id: 1,
        generation: 200,
    };
    let mut service = GameLoginService::new(store.clone(), 1, 1).unwrap();
    service.register_authenticated(key, &account).unwrap();
    let id = service.allocate_character_id(key).await.unwrap();
    let mut prepared = player(id, account.id.0);
    prepared.name = "🦀".into();
    assert!(
        service
            .create_prepared(
                key,
                PreparedCharacter {
                    player: &prepared,
                    slot: 0,
                    items: &[]
                }
            )
            .await
            .is_err()
    );
    assert!(store.load(id).await.unwrap().is_none());
    prepared.name = "Synthetic Player".into();
    service
        .create_prepared(
            key,
            PreparedCharacter {
                player: &prepared,
                slot: 0,
                items: &[],
            },
        )
        .await
        .unwrap();
    prepared.entity.object_id = service.allocate_character_id(key).await.unwrap();
    prepared.name = "Other Player".into();
    assert!(
        service
            .create_prepared(
                key,
                PreparedCharacter {
                    player: &prepared,
                    slot: 0,
                    items: &[]
                }
            )
            .await
            .is_err()
    );
    assert_eq!(service.phase(key).unwrap(), GameLoginPhase::Roster);
    assert_eq!(service.roster(key).await.unwrap().len(), 1);
    store.close().await;
}

#[tokio::test]
async fn cancelled_creation_retains_identity_and_retries_only_exact_prepared_state() {
    let cluster = Cluster::start();
    let store = PgStore::connect(&cluster.url(), 3).await.unwrap();
    store.migrate().await.unwrap();
    let account = new_account(&store, "synthetic-cancel").await;
    let key = SessionKey {
        id: 1,
        generation: 300,
    };
    let mut service = GameLoginService::new(store.clone(), 1, 11).unwrap();
    service.register_authenticated(key, &account).unwrap();
    let id = service.allocate_character_id(key).await.unwrap();
    let prepared = player(id, account.id.0);
    // Hold the same account lock as creation to force cancellation after admission
    // but before the transaction can report success. Only test code contains SQL.
    let pool = sqlx::PgPool::connect(&cluster.url()).await.unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR UPDATE")
        .bind(account.id.0 as i64)
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            service.create_prepared(
                key,
                PreparedCharacter {
                    player: &prepared,
                    slot: 0,
                    items: &[]
                }
            )
        )
        .await
        .is_err()
    );
    assert_eq!(service.phase(key).unwrap(), GameLoginPhase::Creating);
    assert!(matches!(
        service.retire_roster(key),
        Err(GameLoginError::DrainRequired)
    ));
    assert!(matches!(
        service.resolve_creation(key).await,
        Err(GameLoginError::Unresolved)
    ));
    blocker.rollback().await.unwrap();
    assert!(matches!(
        service
            .retry_creation(
                key,
                PreparedCharacter {
                    player: &prepared,
                    slot: 1,
                    items: &[]
                }
            )
            .await,
        Err(GameLoginError::InvalidPreparation)
    ));
    let result = service
        .retry_creation(
            key,
            PreparedCharacter {
                player: &prepared,
                slot: 0,
                items: &[],
            },
        )
        .await
        .unwrap();
    assert_eq!(result.object_id, id);
    assert_eq!(service.roster(key).await.unwrap().len(), 1);
    pool.close().await;
    store.close().await;
}
