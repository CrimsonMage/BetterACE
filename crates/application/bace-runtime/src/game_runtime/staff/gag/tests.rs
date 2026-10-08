use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_persistence::{CharacterLease, OfflineStaffPlayer, StoredAggregate};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1};
async fn player(store: &bace_db_postgres::PgStore) -> (PlayerSaveV6, CharacterLease) {
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("gagcontroller").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"fixture-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("account")
    };
    let saved = PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: store.allocate_player_id().await.unwrap(),
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "gag_controller".into(),
                weenie_type: 1,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: account.id.0,
        name: "Gag Controller".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let lease = store.create_player(&saved, 0, 11, &[]).await.unwrap();
    (PlayerSaveV6::migrate_v1(saved).unwrap(), lease)
}
fn proposal(saved: &PlayerSaveV6) -> StaffGagProposal {
    let target = SocialIdentity {
        character: bace_types::EntityId(saved.player.entity.object_id),
        account: bace_types::AccountId(saved.player.account_id),
        name: saved.player.name.clone(),
    };
    StaffGagProposal {
        operation: 42,
        context: ActionContext {
            actor: target.character,
            account: target.account,
            session: bace_gameplay_api::SessionId(7),
            sequence: 1,
        },
        requested_name: target.name.clone(),
        target,
        before_revision: None,
        enabled: true,
        unix_seconds: 123.,
    }
}
async fn wait_owner(runtime: &mut GameRuntime) {
    for _ in 0..1000 {
        runtime.poll_staff_gag().unwrap();
        if matches!(
            runtime.staff.gag.as_ref().unwrap().phase,
            GagPhase::Owner { .. }
        ) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("gag controller deadline");
}
#[tokio::test]
async fn completed_offline_load_survives_decode_failure_without_repolling_future() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let (saved, lease) = player(&runtime.bootstrap.store).await;
    runtime.staff.gag = Some(GagPending {
        proposal: proposal(&saved),
        phase: GagPhase::Offline(Box::pin(async move {
            Ok(OfflineStaffPlayer {
                lease,
                snapshot: StoredAggregate {
                    object_id: lease.character_id,
                    persisted_version: 1,
                    bytes: vec![0],
                },
            })
        })),
    });
    runtime.poll_staff_gag().unwrap();
    assert!(matches!(
        runtime.staff.gag.as_ref().unwrap().phase,
        GagPhase::Loaded(_)
    ));
    for _ in 0..2 {
        assert!(runtime.poll_staff_gag().is_err());
        assert!(matches!(
            runtime.staff.gag.as_ref().unwrap().phase,
            GagPhase::Loaded(_)
        ));
    }
    // Repair only the cold fixture input. The same retained proposal proceeds.
    if let GagPhase::Loaded(loaded) = &mut runtime.staff.gag.as_mut().unwrap().phase {
        loaded.snapshot.bytes = saved.encode().unwrap();
    }
    wait_owner(&mut runtime).await;
    assert!(matches!(
        runtime.staff.gag.as_ref().unwrap().phase,
        GagPhase::Owner {
            row: Some(_),
            queued: false
        }
    ));
    runtime.staff.gag = None;
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn lost_gag_receipt_retries_same_snapshot_after_login_and_keeps_uncertainty_sticky() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let (saved, lease) = player(&runtime.bootstrap.store).await;
    let p = proposal(&saved);
    let request = operation(runtime.bootstrap.world_owner.epoch(), &p, &saved, 1, lease).unwrap();
    let receipt = runtime
        .bootstrap
        .store
        .apply_staff_gag(&request)
        .await
        .unwrap();
    let loading = runtime.bootstrap.store.begin_login(lease).await.unwrap();
    runtime.staff.gag = Some(GagPending {
        proposal: p.clone(),
        phase: GagPhase::Writing {
            operation: Box::new(request.clone()),
            uncertain: true,
            job: Box::pin(async { Err(bace_db_postgres::StoreError::OwnershipConflict) }),
        },
    });
    assert!(runtime.poll_staff_gag().is_err());
    assert!(
        matches!(&runtime.staff.gag.as_ref().unwrap().phase,GagPhase::Write{operation,uncertain:true} if **operation==request)
    );
    wait_owner(&mut runtime).await;
    let GagPhase::Owner {
        row: Some(row),
        queued: false,
    } = &runtime.staff.gag.as_ref().unwrap().phase
    else {
        panic!("receipt")
    };
    assert_eq!(
        row.expected_version,
        receipt.acknowledgement.persisted_version
    );
    assert_eq!(row.bytes, request.snapshot.bytes);
    assert!(
        runtime.finish_staff_gag(42, Ok(())).is_err(),
        "unsubmitted owner receipt must not be fabricated"
    );
    runtime.staff.gag = None;
    runtime
        .bootstrap
        .store
        .abort_loading(loading.lease)
        .await
        .unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn login_race_definitely_rejects_offline_gag_before_owner_adoption() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let (saved, lease) = player(&runtime.bootstrap.store).await;
    let p = proposal(&saved);
    let loaded = runtime
        .bootstrap
        .store
        .load_offline_staff_player(lease.character_id)
        .await
        .unwrap();
    let loading = runtime.bootstrap.store.begin_login(lease).await.unwrap();
    runtime.staff.gag = Some(GagPending {
        proposal: p,
        phase: GagPhase::Loaded(loaded),
    });
    wait_owner(&mut runtime).await;
    assert!(matches!(
        runtime.staff.gag.as_ref().unwrap().phase,
        GagPhase::Owner {
            row: None,
            queued: false
        }
    ));
    assert!(
        !crate::staff_gags::restore_gag(
            &PlayerSaveV6::decode_or_migrate(&loading.snapshot.bytes)
                .unwrap()
                .player
                .entity
                .state
        )
        .unwrap()
        .state
        .active
    );
    runtime.staff.gag = None;
    runtime
        .bootstrap
        .store
        .abort_loading(loading.lease)
        .await
        .unwrap();
    runtime.quiesce(Duration::ZERO).unwrap();
}
#[tokio::test]
async fn online_gag_baseline_changes_only_after_correlated_owner_receipt() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let (saved, offline) = player(&runtime.bootstrap.store).await;
    let loading = runtime.bootstrap.store.begin_login(offline).await.unwrap();
    let online = runtime
        .bootstrap
        .store
        .finish_login(loading.lease)
        .await
        .unwrap();
    let mut p = proposal(&saved);
    p.before_revision = Some(saved.player.entity.mutation_revision);
    runtime
        .online_saves
        .register(
            bace_gameplay_api::CharacterBinding {
                actor: p.target.character,
                account: p.target.account,
                session: p.context.session,
            },
            online,
            saved.clone(),
            1,
            Duration::ZERO,
        )
        .unwrap();
    runtime
        .online_saves
        .begin_critical(&[p.target.character.0])
        .unwrap();
    let request = operation(runtime.bootstrap.world_owner.epoch(), &p, &saved, 1, online).unwrap();
    runtime.staff.gag = Some(GagPending {
        proposal: p.clone(),
        phase: GagPhase::Write {
            operation: Box::new(request),
            uncertain: false,
        },
    });
    wait_owner(&mut runtime).await;
    assert_eq!(
        runtime
            .online_saves
            .baseline(p.target.character.0)
            .unwrap()
            .1,
        1
    );
    assert!(runtime.finish_staff_gag(p.operation + 1, Ok(())).is_err());
    assert!(runtime.staff.gag.is_some());
    // This adapter test supplies the correlated receipt boundary. Source owner
    // adoption and output are independently qualified by kernel/social_gags.
    if let GagPhase::Owner { queued, .. } = &mut runtime.staff.gag.as_mut().unwrap().phase {
        *queued = true;
    }
    assert!(runtime.finish_staff_gag(p.operation, Ok(())).unwrap());
    let (baseline, version, _) = runtime.online_saves.baseline(p.target.character.0).unwrap();
    let baseline = baseline.clone();
    assert_eq!(version, 2);
    assert!(
        crate::staff_gags::restore_gag(&baseline.player.entity.state)
            .unwrap()
            .state
            .active
    );
    assert!(
        runtime
            .online_saves
            .critical_ready(&[p.target.character.0])
            .unwrap()
    );
    let mut final_saved = baseline.clone();
    final_saved.player.entity.mutation_revision += 1;
    let leaving = runtime.bootstrap.store.begin_logout(online).await.unwrap();
    runtime
        .bootstrap
        .store
        .finish_logout(
            leaving,
            &SaveSnapshot {
                object_id: p.target.character.0,
                mutation_revision: final_saved.player.entity.mutation_revision,
                expected_version: version,
                bytes: final_saved.encode().unwrap(),
            },
        )
        .await
        .unwrap();
    runtime
        .online_saves
        .forget_clean(p.target.character.0)
        .unwrap();
    assert!(runtime.staff.gag.is_none());
    runtime.quiesce(Duration::ZERO).unwrap();
}
