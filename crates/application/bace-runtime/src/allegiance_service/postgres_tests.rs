//! Real PostgreSQL fences joined to the actual allegiance simulation owner.
use super::*;
use crate::player_service::tests::cluster;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_persistence::{AllegianceWrite, OwnershipState};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};

#[tokio::test]
async fn metadata_controller_commits_under_offline_fence_before_owner_adoption() {
    let cluster = cluster::Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 4)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("allegiance-controller").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"test-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("account")
    };
    let id = 0x50000001;
    let player = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "allegiance_controller_player".into(),
                weenie_type: 10,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Offline Monarch".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&player, 0, 11, &[]).await.unwrap();
    let node = bace_allegiance::AllegianceNode {
        character: bace_types::EntityId(id),
        account: account.id,
        name: player.name.clone(),
        gender: 1,
        heritage: 1,
        patron: None,
        monarch: bace_types::EntityId(id),
        vassals: vec![],
        rank: 1,
        followers: 0,
        level: 100,
        leadership: 291,
        loyalty: 291,
        sworn_at: 0,
        online_seconds: 1,
        may_pass_up: false,
        received_total: 0,
        tithed_total: 0,
        unclaimed: 0,
    };
    let metadata = bace_allegiance::AllegianceMetadata::new(node.character, 0x80000001);
    store
        .allegiance_operation(&AllegianceOperation {
            operation_id: "controller-initial".into(),
            nodes: vec![AllegianceWrite {
                character: id,
                expected_version: 0,
                mutation_revision: 1,
                bytes: Some(
                    crate::social_saves::freeze_allegiance_node(&node)
                        .unwrap()
                        .encode()
                        .unwrap(),
                ),
            }],
            metadata: vec![AllegianceWrite {
                character: id,
                expected_version: 0,
                mutation_revision: 1,
                bytes: Some(
                    crate::social_saves::freeze_allegiance_metadata(&metadata)
                        .unwrap()
                        .encode()
                        .unwrap(),
                ),
            }],
            players: vec![],
            leases: vec![lease],
        })
        .await
        .unwrap();
    let (nodes, metadata) = store.load_allegiance_forest(2, 1024 * 1024).await.unwrap();
    let mut kernel =
        bace_simulation::Kernel::with_gameplay_limits(bace_world::World::default(), 32, 8, 32)
            .unwrap();
    kernel
        .register_allegiances(
            crate::social_saves::restore_allegiances(&nodes, &metadata, 2).unwrap(),
            0.,
        )
        .unwrap();
    kernel
        .prepare_allegiance_management(
            node.character,
            bace_allegiance::AllegianceManagement::Name(Some("Durable Name".into())),
        )
        .unwrap();
    let ticket = kernel.take_allegiance_proposal().unwrap();
    let fences = store
        .character_leases(&AllegianceService::participants(&ticket))
        .await
        .unwrap();
    assert_eq!(fences, vec![lease]);
    assert_eq!(lease.state, OwnershipState::Offline);
    assert!(store.character_leases(&[id, id]).await.is_err());
    assert!(store.character_leases(&vec![id; 1025]).await.is_err());
    assert!(store.character_leases(&[id + 1]).await.unwrap().is_empty());
    let mut service = AllegianceService::new(9, &nodes, &metadata).unwrap();
    service.stage(ticket.clone(), vec![], fences).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    let worker =
        crate::saves::spawn_save_worker(store.clone(), crate::saves::SaveWorkerConfig::default())
            .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if service
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase, Phase::Delivering { .. }))
            {
                break;
            }
            service
                .poll_with(&mut online, &worker.handle, 1, |_| {
                    panic!("early owner completion")
                })
                .unwrap();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        kernel.allegiance_relation(node.character).unwrap().revision,
        1
    );
    let (_, committed) = store.load_allegiance_forest(2, 1024 * 1024).await.unwrap();
    assert_eq!(
        bace_storage_codec::AllegianceMetadataV1::decode(&committed[0].bytes)
            .unwrap()
            .name
            .as_deref(),
        Some("Durable Name")
    );
    let mut outcome = None;
    service
        .poll_with(&mut online, &worker.handle, 2, |command| {
            let Command::SocialControl(control) = command else {
                panic!("control")
            };
            let SocialControlAction::Commit(ticket) = control.action else {
                panic!("commit")
            };
            outcome = Some(SocialControlOutcome {
                sequence: control.sequence,
                result: kernel
                    .confirm_allegiance_committed(&ticket)
                    .map(|()| Some(ticket.operation)),
            });
            Ok(())
        })
        .unwrap();
    service.accept_control(outcome.unwrap()).unwrap();
    service
        .poll_with(&mut online, &worker.handle, 3, |_| panic!())
        .unwrap();
    assert!(service.take_completion().unwrap().committed);
    assert_eq!(
        kernel.allegiance_relation(node.character).unwrap().revision,
        2
    );
    assert!(!service.requires_drain());
    // A lease captured before a competing ownership transition cannot commit;
    // the rejected proposal must still be released by the exact live owner.
    kernel
        .prepare_allegiance_management(
            node.character,
            bace_allegiance::AllegianceManagement::Name(Some("Must Not Commit".into())),
        )
        .unwrap();
    let rejected = kernel.take_allegiance_proposal().unwrap();
    let mut stale = lease;
    stale.epoch += 1;
    service
        .stage(rejected.clone(), vec![], vec![stale])
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if service
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase, Phase::Delivering { .. }))
            {
                break;
            }
            service
                .poll_with(&mut online, &worker.handle, 4, |_| {
                    panic!("early owner rejection")
                })
                .unwrap();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut outcome = None;
    service
        .poll_with(&mut online, &worker.handle, 4, |command| {
            let Command::SocialControl(control) = command else {
                panic!("control")
            };
            let SocialControlAction::Reject(ticket) = control.action else {
                panic!("expected rollback")
            };
            assert_eq!(*ticket, rejected);
            outcome = Some(SocialControlOutcome {
                sequence: control.sequence,
                result: kernel
                    .reject_allegiance_proposal(&ticket)
                    .map(|()| Some(ticket.operation)),
            });
            Ok(())
        })
        .unwrap();
    service.accept_control(outcome.unwrap()).unwrap();
    service
        .poll_with(&mut online, &worker.handle, 5, |_| panic!())
        .unwrap();
    assert!(!service.take_completion().unwrap().committed);
    assert_eq!(
        kernel.allegiance_relation(node.character).unwrap().revision,
        2
    );
    assert_eq!(
        store
            .load_allegiance_forest(2, 1024 * 1024)
            .await
            .unwrap()
            .1,
        committed
    );
    worker.handle.close();
    worker.task.await.unwrap();
}
