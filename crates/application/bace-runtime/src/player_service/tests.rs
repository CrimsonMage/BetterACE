//! Real PostgreSQL leases and routine saves with authored geometry/capabilities.
//! Packet assertions exercise server-side entry only; no client qualification.
use super::*;
use crate::{
    online_player_saves::OnlinePlayerSaveService,
    saves::{SaveWorkerConfig, spawn_save_worker},
    simulation::{SimulationConfig, SimulationWorker},
};
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_persistence::OwnershipState;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use std::time::Duration;
pub(crate) mod cluster;
mod detach;
mod entry;
pub(crate) mod fixture;
mod turbine;
async fn receive<T>(receiver: &std::sync::mpsc::Receiver<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match receiver.try_recv() {
                Ok(value) => return value,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(2)).await
                }
                Err(e) => panic!("owner output closed: {e}"),
            }
        }
    })
    .await
    .expect("bounded owner response")
}
#[tokio::test]
async fn admission_online_entry_and_owned_routine_save_use_one_accepted_owner() {
    let cluster = cluster::Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 4)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("player-service-lifecycle").unwrap(),
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
    let mut service =
        PlayerService::new(GameLoginService::new(store.clone(), 2, 11).unwrap(), 4).unwrap();
    service.authenticated(key, &account).unwrap();
    let work = service.begin_io(key, PlayerIoAction::Roster).unwrap();
    assert!(service.io_busy());
    assert!(service.begin_io(key, PlayerIoAction::Roster).is_err());
    assert!(matches!(
        service
            .accept_io(work.execute().await)
            .unwrap_or_else(|_| panic!("I/O owner")),
        Ok(PlayerIoResult::Roster)
    ));
    let NetworkCommand::Send { bytes, .. } = service.network.pop_front().unwrap() else {
        panic!("roster")
    };
    assert!(
        bace_wire::CharacterList::decode(&bytes, 11, 128)
            .unwrap()
            .characters
            .is_empty()
    );
    let actor = EntityId(service.allocate_character_id(key).await.unwrap());
    let binding = CharacterBinding {
        session: bace_gameplay_api::SessionId(key.generation),
        account: account.id,
        actor,
    };
    // Build saved source from the same authored prepared state through the actual
    // snapshot freezer, then discard this test-only preparation owner.
    let (mut staging, prepared) = fixture::fixture(binding);
    assert!(staging.admit_player(prepared).is_ok());
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: actor.0,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "lifecycle_fixture".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let saved = crate::player_saves::freeze_player_snapshot(
        &saved,
        &staging.read_player_snapshot(binding).unwrap(),
        0,
    )
    .unwrap();
    drop(staging);
    assert!(
        service
            .stage_creation(
                key,
                crate::character_creation::FrozenCharacterCreation {
                    player: saved.player.clone(),
                    items: vec![]
                },
                0
            )
            .is_ok()
    );
    let work = service.begin_io(key, PlayerIoAction::Create).unwrap();
    assert!(
        service
            .stage_creation(
                key,
                crate::character_creation::FrozenCharacterCreation {
                    player: saved.player.clone(),
                    items: vec![]
                },
                1
            )
            .is_err(),
        "in-flight creation retains sole identity"
    );
    assert!(matches!(
        service
            .accept_io(work.execute().await)
            .unwrap_or_else(|_| panic!("I/O owner")),
        Ok(PlayerIoResult::Created(_))
    ));
    let NetworkCommand::Send { bytes, .. } = service.network.pop_front().unwrap() else {
        panic!("creation")
    };
    assert_eq!(
        bytes,
        bace_wire::CharacterReply::Created {
            object_id: actor.0,
            name: "Alice".into()
        }
        .encode()
        .unwrap()
    );
    let work = service
        .begin_io(key, PlayerIoAction::Load(actor.0))
        .unwrap();
    let Ok(PlayerIoResult::Loaded(loaded)) = service
        .accept_io(work.execute().await)
        .unwrap_or_else(|_| panic!("I/O owner"))
    else {
        panic!("loaded")
    };
    assert_eq!(loaded.lease.state, OwnershipState::Loading);
    assert!(service.finish_online(key).await.is_err());
    let (kernel, prepared) = fixture::fixture(binding);
    let worker = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            command_capacity: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let token = service
        .stage_admission(key, prepared)
        .unwrap_or_else(|(e, _)| panic!("{e}"));
    assert_eq!(service.flush_admissions(&worker.input(), 1), 1);
    let outcome = receive(worker.player_admission_outcomes()).await;
    assert_eq!(outcome.correlation, token);
    assert!(outcome.result.is_ok());
    assert!(service.accept_admission(outcome).is_ok());
    assert!(
        service.replication(actor).is_none(),
        "world acceptance is not Online lease acknowledgment"
    );
    let work = service.begin_io(key, PlayerIoAction::Online).unwrap();
    let Ok(PlayerIoResult::Online(online)) = service
        .accept_io(work.execute().await)
        .unwrap_or_else(|_| panic!("I/O owner"))
    else {
        panic!("online")
    };
    let receipt = store.online_login_receipt(online).await.unwrap();
    service.install_login_receipt(key, receipt).unwrap();
    assert_eq!(online.state, OwnershipState::Online);
    assert_eq!(service.finish_online(key).await.unwrap(), online);
    let description = bace_wire::PlayerDescription::default();
    let titles = bace_wire::CharacterTitle {
        current: 0,
        titles: vec![],
    };
    let friends = bace_wire::FriendsUpdate {
        kind: bace_wire::FriendsUpdateKind::Full,
        friends: vec![],
    };
    let object = entry::object(actor.0);
    service
        .queue_entry(
            key,
            LoginProjection {
                description: &description,
                titles: &titles,
                friends: &friends,
                self_object: &object,
                possessions: &[],
            },
            entry::limits(),
        )
        .unwrap();
    assert!(
        service
            .queue_entry(
                key,
                LoginProjection {
                    description: &description,
                    titles: &titles,
                    friends: &friends,
                    self_object: &object,
                    possessions: &[]
                },
                entry::limits()
            )
            .is_err()
    );
    assert_eq!(service.network.len(), 1);
    assert_eq!(
        service.flush_entered(&worker.input(), 1),
        0,
        "network backpressure must keep UI locked"
    );
    assert_eq!(
        service.replication(actor).unwrap().events.next_sequence(),
        3
    );
    let NetworkCommand::SendOrderedBatch { messages, .. } = service.network.pop_front().unwrap()
    else {
        panic!("entry batch")
    };
    assert_eq!(
        messages.iter().map(|m| m.0).collect::<Vec<_>>(),
        [9, 9, 9, 10, 10]
    );
    assert!(!service.entered(actor));
    assert_eq!(service.flush_entered(&worker.input(), 1), 1);
    let entered = receive(worker.player_entered_outcomes()).await;
    assert_eq!(service.accept_entered(entered).unwrap(), key);
    assert!(service.entered(actor));
    let origin = tokio::time::Instant::now() - Duration::from_secs(60);
    let mut routine = OnlinePlayerSaveService::new(2, 1024 * 1024, origin).unwrap();
    routine
        .register_loaded(&loaded, online, Duration::ZERO)
        .unwrap();
    worker
        .input()
        .try_submit(Command::Ui {
            context: bace_gameplay_api::ActionContext {
                session: binding.session,
                account: binding.account,
                actor,
                sequence: 1,
            },
            request: bace_gameplay_api::UiRequest::Filters(7),
        })
        .unwrap();
    routine.mark_dirty(actor.0, Duration::from_secs(1)).unwrap();
    assert!(
        routine
            .request_capture(&worker.input(), 99, Duration::from_secs(6), false)
            .unwrap()
    );
    let capture = receive(worker.player_snapshot_outcomes()).await;
    assert!(capture.result.is_ok());
    routine.accept_capture(capture, 6_000).unwrap();
    let saves = spawn_save_worker(store.clone(), SaveWorkerConfig::default()).unwrap();
    assert_eq!(
        routine
            .submit_due(&saves.handle, Duration::from_secs(6), false, 1)
            .unwrap(),
        1
    );
    assert!(!routine.critical_ready(&[actor.0]).unwrap());
    tokio::time::timeout(Duration::from_secs(5), async {
        while routine.poll_writes(1).unwrap() == 0 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let durable = store.load(actor.0).await.unwrap().unwrap();
    let restored = PlayerSaveV6::decode_or_migrate(&durable.bytes).unwrap();
    assert_eq!(restored.ui.spellbook_filters, 7);
    assert_eq!(routine.baseline(actor.0).unwrap().0, &restored);
    assert!(routine.critical_ready(&[actor.0]).unwrap());
    // A late accepted UI mutation must enter the atomic detached final capture,
    // not be dropped by the previous routine snapshot's revision.
    worker
        .input()
        .try_submit(Command::Ui {
            context: bace_gameplay_api::ActionContext {
                session: binding.session,
                account: binding.account,
                actor,
                sequence: 2,
            },
            request: bace_gameplay_api::UiRequest::Filters(8),
        })
        .unwrap();
    let revision = routine
        .baseline(actor.0)
        .unwrap()
        .0
        .player
        .entity
        .mutation_revision;
    worker
        .input()
        .try_submit(Command::DetachPlayer(
            bace_simulation::PlayerDetachRequest {
                correlation: 1000,
                binding,
                expected_revision: revision,
                expected_items: vec![],
                capture_final: true,
            },
        ))
        .unwrap();
    let detached = receive(worker.player_detach_outcomes())
        .await
        .result
        .unwrap_or_else(|e| panic!("detach {e:?}"));
    assert_eq!(detached.actor.id, actor);
    assert_eq!(detached.snapshot.character().ui().unwrap().state.filters, 8);
    routine
        .stage_detached_snapshot(detached.snapshot.clone(), Duration::from_secs(7), 7000)
        .unwrap();
    assert_eq!(
        routine
            .submit_due(&saves.handle, Duration::from_secs(7), true, 1)
            .unwrap(),
        1
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !routine.player_clean(actor.0) {
            routine.poll_writes(1).unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let (saved, version, _) = routine.baseline(actor.0).unwrap();
    assert_eq!(saved.ui.spellbook_filters, 8);
    let snapshot = bace_persistence::SaveSnapshot {
        object_id: actor.0,
        mutation_revision: saved.player.entity.mutation_revision,
        expected_version: version,
        bytes: saved.encode().unwrap(),
    };
    let offline = crate::game_runtime::finish_detached_logout(&store, online, &snapshot)
        .await
        .unwrap();
    assert_eq!(
        crate::game_runtime::finish_detached_logout(&store, online, &snapshot)
            .await
            .unwrap(),
        offline,
        "replayed finalization resolves exact bytes without another write"
    );
    let work = service
        .begin_io(key, PlayerIoAction::Logout(offline))
        .unwrap();
    assert!(matches!(
        service
            .accept_io(work.execute().await)
            .unwrap_or_else(|_| panic!("I/O owner")),
        Ok(PlayerIoResult::LoggedOut(_))
    ));
    routine.forget_clean(actor.0).unwrap();
    drop(detached); // Only after all exact durable fences above.
    let loaded_again = service.load(key, actor.0).await.unwrap();
    assert_eq!(loaded_again.player.ui.spellbook_filters, 8);
    assert_eq!(loaded_again.lease.epoch, offline.epoch + 1);
    service.abort_loading(key).await.unwrap();
    let exit = worker.shutdown_recover().unwrap();
    assert!(exit.kernel.character(actor).is_none());
    assert!(exit.unprocessed_commands.is_empty());
    saves.handle.close();
    saves.task.await.unwrap();
    store.close().await;
}

#[test]
fn entered_requires_exact_binding_and_retains_next_request_under_output_pressure() {
    let binding = CharacterBinding {
        session: bace_gameplay_api::SessionId(91),
        account: bace_types::AccountId(2),
        actor: EntityId(1),
    };
    let (mut kernel, prepared) = fixture::fixture(binding);
    assert!(kernel.admit_player(prepared).is_ok());
    let context = bace_gameplay_api::ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence: 1,
    };
    assert_eq!(
        kernel.apply_ui(context, bace_gameplay_api::UiRequest::Filters(3)),
        Err(bace_gameplay_api::UiError::BeforeEntry)
    );
    kernel
        .try_enqueue(Command::PlayerEntered(
            bace_simulation::PlayerEnteredRequest {
                correlation: 8,
                binding: CharacterBinding {
                    session: bace_gameplay_api::SessionId(90),
                    ..binding
                },
            },
        ))
        .unwrap_or_else(|_| panic!("bounded owner command"));
    kernel
        .try_enqueue(Command::PlayerEntered(
            bace_simulation::PlayerEnteredRequest {
                correlation: 9,
                binding,
            },
        ))
        .unwrap_or_else(|_| panic!("bounded owner command"));
    kernel.step().unwrap();
    kernel.step().unwrap();
    assert_eq!(
        kernel.apply_ui(
            bace_gameplay_api::ActionContext {
                sequence: 2,
                ..context
            },
            bace_gameplay_api::UiRequest::Filters(3)
        ),
        Err(bace_gameplay_api::UiError::BeforeEntry)
    );
    let wrong = kernel.take_player_entered_outcome().unwrap();
    assert_eq!(wrong.correlation, 8);
    assert_eq!(wrong.result, Err(bace_gameplay_api::UiError::Ownership));
    assert!(kernel.peek_player_entered_outcome().is_none());
    kernel.step().unwrap();
    let accepted = kernel.take_player_entered_outcome().unwrap();
    assert_eq!(accepted.correlation, 9);
    assert_eq!(accepted.result, Ok(()));
    assert!(
        kernel
            .apply_ui(
                bace_gameplay_api::ActionContext {
                    sequence: 3,
                    ..context
                },
                bace_gameplay_api::UiRequest::Filters(3)
            )
            .is_ok()
    );
}
