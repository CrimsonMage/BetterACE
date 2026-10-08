use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
async fn account(runtime: &mut GameRuntime) -> (SessionKey, bace_types::AccountId) {
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("creation-driver").unwrap(),
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
    let id = account.id;
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
    (key, id)
}
fn message(color: u32) -> bace_transport::ReceivedMessage {
    // Source CharacterCreateInfo wire order, ASCII subset common to UTF8/CP1252.
    let mut w = bace_wire::Writer::new();
    w.u32(0xf656);
    w.string16("creation-driver").unwrap();
    for value in [1, 1, 1] {
        w.u32(value);
    }
    for value in [
        0,
        0,
        0,
        0,
        0,
        0,
        u32::MAX,
        color,
        0,
        color,
        0,
        color,
        0,
        color,
    ] {
        w.u32(value);
    }
    for _ in 0..6 {
        w.f64(0.5);
    }
    w.u32(0);
    for _ in 0..6 {
        w.u32(10);
    }
    for value in [0, 0, 55] {
        w.u32(value);
    }
    for id in 0..55 {
        w.u32(if bace_character::is_locked_skill(id) {
            2
        } else {
            0
        });
    }
    w.string16("Creationprobe").unwrap();
    for value in [0, 0, 0] {
        w.u32(value);
    }
    bace_transport::ReceivedMessage {
        sequence: 1,
        id: 1,
        queue: 9,
        bytes: w.into_bytes(),
    }
}
async fn complete(runtime: &mut GameRuntime) {
    tokio::time::timeout(Duration::from_secs(60), async {
        while runtime.creation.pending() {
            let elapsed = runtime.clock.monotonic.elapsed();
            let unix = runtime.clock.unix_millis + elapsed.as_millis() as u64;
            runtime.last_elapsed = elapsed;
            runtime.poll_lifecycle(elapsed, unix).unwrap();
            runtime.poll_creation(unix).unwrap();
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn missing_assets_disconnect_retains_allocations_until_exact_cold_failure() {
    let (_cluster, _dir, mut runtime) = super::super::tests::fixture::fixture().await;
    let (key, account) = account(&mut runtime).await;
    assert!(matches!(
        runtime.handle_creation_message(key, &message(1)).unwrap(),
        CreationIngress::Accepted
    ));
    assert!(runtime.creation.pending());
    assert!(matches!(
        runtime.handle_creation_message(key, &message(1)).unwrap(),
        CreationIngress::Blocked
    ));
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    runtime
        .poll_logout(Duration::ZERO, runtime.clock.unix_millis)
        .unwrap();
    assert!(runtime.sessions.contains_key(&key));
    complete(&mut runtime).await;
    assert!(
        runtime
            .bootstrap
            .store
            .players_for_account(account.0)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        runtime.network_output.is_empty(),
        "disconnected failure does not invent a success packet"
    );
    runtime
        .creation
        .shutdown()
        .unwrap()
        .unwrap()
        .join()
        .unwrap();
}
#[tokio::test]
#[ignore = "requires approved actual DATs and accepted full native pack"]
async fn actual_creation_commits_after_disconnect_and_before_success() {
    let (_cluster, _dir, mut runtime) = super::super::tests::fixture::fixture().await;
    let (key, account) = account(&mut runtime).await;
    let path = std::path::PathBuf::from(std::env::var_os("BACE_CREATION_PACK_MANIFEST").unwrap());
    let manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = Arc::new(
        manifest
            .open(path.parent().unwrap(), Default::default())
            .unwrap(),
    );
    let dat = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let assets = crate::region_activation::RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut verified = crate::region_activation::VerifiedRegionAssets::open(&assets).unwrap();
    let closure = verified
        .prepare_creation_assets(&generation, 1, &runtime.creation.profile)
        .unwrap();
    let color = closure.creation.genders[0].clothing_colors[0];
    drop(verified);
    runtime
        .creation
        .shutdown()
        .unwrap()
        .unwrap()
        .join()
        .unwrap();
    runtime.creation = CreationRuntime::new(assets, 1, 11).unwrap();
    runtime.bootstrap.pack = crate::pack_io::PreparedPack {
        manifest,
        generation,
    };
    assert!(matches!(
        runtime
            .handle_creation_message(key, &message(color))
            .unwrap(),
        CreationIngress::Accepted
    ));
    assert!(
        runtime
            .bootstrap
            .store
            .players_for_account(account.0)
            .await
            .unwrap()
            .is_empty()
    );
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime.sessions.get_mut(&key).unwrap().terminated = true;
    complete(&mut runtime).await;
    let roster = runtime
        .bootstrap
        .store
        .players_for_account(account.0)
        .await
        .unwrap();
    assert_eq!(roster.len(), 1);
    assert_eq!(roster[0].name, "Creationprobe");
    let stored = runtime
        .bootstrap
        .store
        .load(roster[0].object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.bytes.is_empty());
    runtime
        .creation
        .shutdown()
        .unwrap()
        .unwrap()
        .join()
        .unwrap();
}
