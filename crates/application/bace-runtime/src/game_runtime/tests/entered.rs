//! Real PostgreSQL + accepted native pack + approved DAT lifecycle harness. UDP
//! handshake, character requests, and reliable output use real UDP. No entered
//! flags or world actors are manufactured.
use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_gameplay_api::CharacterBinding;
use bace_types::EntityId;
use std::{path::PathBuf, sync::atomic::AtomicBool};
mod client;
pub(in crate::game_runtime) struct EnteredFixture {
    pub runtime: Box<GameRuntime>,
    pub key: SessionKey,
    pub binding: CharacterBinding,
    pub client: client::Client,
    pub creation_color: u32,
    pub _directory: tempfile::TempDir,
    pub _cluster: crate::player_service::tests::cluster::Cluster,
}
pub(in crate::game_runtime) async fn fixture() -> EnteredFixture {
    fixture_with_start_area(Vec::new(), 0).await
}
pub(in crate::game_runtime) async fn fixture_with_start_area(
    records: Vec<bace_storage_codec::PackRecord>,
    start_area: u32,
) -> EnteredFixture {
    eprintln!(
        "entry fixture: runtime={} startup={}",
        std::mem::size_of::<GameRuntime>(),
        std::mem::size_of::<GameRuntimeStartup>()
    );
    let cluster = crate::player_service::tests::cluster::Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 8)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = PathBuf::from(
        std::env::var_os("BACE_WORLD_MANIFEST")
            .or_else(|| std::env::var_os("BACE_CREATION_PACK_MANIFEST"))
            .expect("accepted native pack"),
    );
    let mut manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    for segment in std::iter::once(&manifest.base).chain(&manifest.deltas) {
        std::fs::copy(
            path.parent().unwrap().join(&segment.file_name),
            directory.path().join(&segment.file_name),
        )
        .unwrap();
    }
    // Some locally accepted historical generations predate the native ACE
    // treasure-table record. Publish only that repository-authored record into
    // this test's private immutable delta; never alter the accepted source pack.
    let source = manifest.open(directory.path(), Default::default()).unwrap();
    if matches!(
        source
            .lookup(bace_storage_codec::PackKey {
                namespace: 52,
                id: 1,
            })
            .unwrap(),
        bace_storage_codec::PackLookup::Missing
    ) {
        let tables = bace_content_tools::parse_treasure_table_set(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../gameplay/bace-loot/data/ace-treasure-tables.toml"
        )))
        .unwrap();
        assert_eq!(tables.id, 1);
        let bytes = bace_content_tools::compile_treasure_table_set(&tables).unwrap();
        let delta = bace_storage_codec::compile_pack(
            directory.path(),
            [Ok(bace_storage_codec::PackRecord {
                key: bace_storage_codec::PackKey {
                    namespace: 52,
                    id: 1,
                },
                schema: 1,
                value: Some(bytes),
            })],
            Default::default(),
        )
        .unwrap();
        manifest.deltas.push(delta);
        manifest.generation += 1;
        bace_storage_codec::write_manifest(directory.path(), &manifest, Default::default())
            .unwrap();
    }
    drop(source);
    if !records.is_empty() {
        assert!(records.len() <= 64, "bounded private test delta");
        assert!(
            records
                .iter()
                .map(|record| record.value.as_ref().map_or(0, Vec::len))
                .sum::<usize>()
                <= 4 * 1024 * 1024,
            "bounded private test bytes"
        );
        let delta = bace_storage_codec::compile_pack(
            directory.path(),
            records.into_iter().map(Ok),
            Default::default(),
        )
        .unwrap();
        manifest.deltas.push(delta);
        manifest.generation += 1;
        bace_storage_codec::write_manifest(directory.path(), &manifest, Default::default())
            .unwrap();
    }
    let generation = Arc::new(manifest.open(directory.path(), Default::default()).unwrap());
    store
        .accept_mapped(
            None,
            &bace_persistence::MappedGeneration {
                manifest_hash: manifest.content_hash(Default::default()).unwrap(),
                parent_hash: None,
                base_hash: manifest.base.generation,
                accepted_revision: 0,
                manifest_bytes: manifest.encode(Default::default()).unwrap(),
            },
        )
        .await
        .unwrap();
    let key_file = directory.path().join("synthetic-fixture.key");
    // Deterministic, explicitly synthetic test secret in the normal frozen key
    // format. The production loader and PostgreSQL fingerprint binding run.
    {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        let mut bytes = Vec::from(*b"BACERNG1");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[7; 32]);
        let digest = Sha256::digest(&bytes);
        bytes.extend_from_slice(&digest);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&key_file).unwrap().write_all(&bytes).unwrap();
    }
    let random = crate::game_random::load_and_bind(&store, &key_file)
        .await
        .unwrap();
    let dat =
        PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("approved DAT directory"));
    let assets = crate::region_activation::RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut verified = crate::region_activation::VerifiedRegionAssets::open(&assets).unwrap();
    eprintln!("entry fixture: preparing startup assets");
    let startup_assets = verified
        .prepare_runtime_startup_assets(&generation, Default::default())
        .unwrap();
    let creation = verified
        .prepare_creation_assets(
            &generation,
            1,
            &crate::creation_profile::builtin_creation_profile().unwrap(),
        )
        .unwrap();
    let color = creation.creation.genders[0].clothing_colors[0];
    drop(verified);
    let mut config = bace_config::ServerConfig {
        max_sessions: 4,
        command_capacity: 4096,
        bind_address: "127.0.0.1:0".into(),
        pack_directory: Some(directory.path().to_owned()),
        ..Default::default()
    };
    config.world.preloading.enabled = false;
    let pressure = Arc::new(AtomicBool::new(false));
    let bootstrap = GameBootstrap {
        world_settings: crate::world_settings::prepare_world_settings(&config.world).unwrap(),
        config,
        world_owner: store.acquire_world_owner().await.unwrap(),
        store,
        random,
        assets,
        pack: crate::pack_io::PreparedPack {
            manifest,
            generation,
        },
        pack_io: Some(
            crate::pack_io::PackIoWorker::start(directory.path().to_owned(), 2, pressure.clone())
                .unwrap(),
        ),
        save_pressure: pressure,
        allegiance_nodes: vec![],
        allegiance_metadata: vec![],
    };
    let mut startup = GameRuntimeStartup::new(
        bootstrap,
        startup_assets,
        GameRuntimeLimits {
            sessions: 4,
            loading: 2,
            messages: 256,
            message_bytes: 1 << 20,
            work_per_poll: 64,
        },
        GameClock::from_pair(1_800_000_000_000, std::time::Instant::now()).unwrap(),
    )
    .unwrap_or_else(|(e, _)| panic!("{e}"));
    eprintln!("entry fixture: advancing startup");
    let mut runtime = loop {
        if let Some(runtime) = startup.advance().await.unwrap() {
            break runtime;
        }
    };
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("Account").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account");
    };
    eprintln!("entry fixture: authenticating UDP");
    let mut client = client::Client::new(&runtime, "Account");
    poll_until(&mut runtime, &mut client, |runtime, _| {
        runtime.sessions.values().any(|s| s.connected)
    })
    .await;
    let key = *runtime.sessions.keys().next().unwrap();
    client.bind(key);
    assert_eq!(runtime.sessions[&key].account.id, account.id);
    poll_until(&mut runtime, &mut client, |runtime, _| {
        runtime.login_job.is_none() && runtime.login_queue.is_empty()
    })
    .await;
    eprintln!("entry fixture: creating character");
    let create = creation_message_with_start_area(
        runtime.sessions[&key].account.name.as_str(),
        color,
        "Entryprobe",
        start_area,
    );
    client.send_message(&runtime, key, &create);
    poll_until(&mut runtime, &mut client, |_, client| {
        client.messages.iter().any(|message| {
            message.bytes.starts_with(
                &bace_wire::opcode::GameMessageOpcode::CharacterCreateResponse
                    .0
                    .to_le_bytes(),
            )
        })
    })
    .await;
    let roster = runtime
        .bootstrap
        .store
        .players_for_account(account.id.0)
        .await
        .unwrap();
    assert_eq!(
        roster.len(),
        1,
        "real durable creation must precede selection"
    );
    let actor = EntityId(roster[0].object_id);
    eprintln!("entry fixture: entering {actor:?}");
    let entry_start = client.messages.len();
    let mut request = bace_wire::Writer::new();
    request.u32(bace_wire::opcode::GameMessageOpcode::CharacterEnterWorld.0);
    request.u32(actor.0);
    request
        .string16(runtime.sessions[&key].account.name.as_str())
        .unwrap();
    client.send_message(&runtime, key, &request.into_bytes());
    poll_until(&mut runtime, &mut client, |runtime, _| {
        runtime.players.entered(actor)
            && runtime
                .sessions
                .get(&key)
                .and_then(|s| s.loading.as_ref())
                .is_some_and(|l| l.phase == lifecycle::Phase::Entered)
    })
    .await;
    let binding = runtime.sessions[&key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .binding;
    assert_eq!(binding.actor, actor);
    assert!(runtime.players.replication(actor).is_some());
    poll_until(&mut runtime, &mut client, |_, client| {
        client.has_create(actor.0, entry_start)
    })
    .await;
    client.assert_entry_order(actor.0, entry_start);
    EnteredFixture {
        runtime,
        key,
        binding,
        client,
        creation_color: color,
        _directory: directory,
        _cluster: cluster,
    }
}
pub(in crate::game_runtime) async fn poll_until(
    runtime: &mut GameRuntime,
    client: &mut client::Client,
    ready: impl Fn(&GameRuntime, &client::Client) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            runtime
                .poll(runtime.clock.monotonic.elapsed())
                .unwrap_or_else(|e| {
                    panic!(
                        "entered controller: {e}; player failures {:?}; loading {:?}",
                        runtime
                            .sessions
                            .iter()
                            .map(|(k, s)| (*k, s.failure.clone()))
                            .collect::<Vec<_>>(),
                        runtime
                            .sessions
                            .iter()
                            .map(|(k, s)| (*k, s.loading.as_ref().map(|l| l.phase as u8)))
                            .collect::<Vec<_>>()
                    )
                });
            client.drain(runtime);
            if let Some((key, failure)) = runtime
                .sessions
                .iter()
                .find_map(|(key, s)| s.failure.as_ref().map(|e| (*key, e)))
            {
                panic!("entry {key:?}: {failure}");
            }
            if ready(runtime, client) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("bounded real entry progress");
}
fn creation_message(account: &str, color: u32, name: &str) -> Vec<u8> {
    creation_message_with_start_area(account, color, name, 0)
}
fn creation_message_with_start_area(
    account: &str,
    color: u32,
    name: &str,
    start_area: u32,
) -> Vec<u8> {
    let mut w = bace_wire::Writer::new();
    w.u32(0xf656);
    w.string16(account).unwrap();
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
    for value in [10, 10, 10, 10, 100, 100] {
        w.u32(value);
    }
    for value in [0, 0, 55] {
        w.u32(value);
    }
    for id in 0..55 {
        w.u32(
            if bace_character::is_locked_skill(id) || [16, 33].contains(&id) {
                2
            } else if [15, 22, 24, 31, 32, 34, 43, 44].contains(&id) {
                // The avatar admission model requires these ordinary skills,
                // including untrained movement and magic-defense values.
                1
            } else {
                0
            },
        );
    }
    w.string16(name).unwrap();
    for value in [start_area, 0, 0] {
        w.u32(value);
    }
    w.into_bytes()
}
#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn native_character_creation_and_login_reach_real_entered_owner() {
    let fixture = Box::pin(fixture()).await;
    assert!(fixture.runtime.players.entered(fixture.binding.actor));
    assert!(
        !fixture.client.messages.is_empty(),
        "actual reliable UDP delivered entry output"
    );
    fixture.shutdown().await;
}

async fn poll_until_pair(
    runtime: &mut GameRuntime,
    first: &mut client::Client,
    second: &mut client::Client,
    ready: impl Fn(&GameRuntime, &client::Client, &client::Client) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            runtime
                .poll(runtime.clock.monotonic.elapsed())
                .unwrap_or_else(|error| panic!("two-player entry: {error}"));
            first.drain(runtime);
            second.drain(runtime);
            if let Some(error) = runtime.sessions.values().find_map(|s| s.failure.as_ref()) {
                panic!("two-player entry: {error}");
            }
            if ready(runtime, first, second) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("bounded two-player entry progress");
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn native_two_player_visibility_save_and_reconnect() {
    let mut fixture = Box::pin(fixture()).await;
    let CreateAccountOutcome::Created(account) = fixture
        .runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("AcctTwo").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("second account created");
    };
    let mut second = client::Client::new(&fixture.runtime, "AcctTwo");
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |runtime, _, _| {
            runtime
                .sessions
                .values()
                .any(|s| s.connected && s.account.id == account.id)
        },
    )
    .await;
    let key = *fixture
        .runtime
        .sessions
        .iter()
        .find(|(_, s)| s.account.id == account.id)
        .unwrap()
        .0;
    second.bind(key);
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |runtime, _, _| runtime.login_job.is_none() && runtime.login_queue.is_empty(),
    )
    .await;
    second.send_message(
        &fixture.runtime,
        key,
        &creation_message("AcctTwo", fixture.creation_color, "Entrytwo"),
    );
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |_, _, second| {
            second.messages.iter().any(|m| {
                m.bytes.starts_with(
                    &bace_wire::opcode::GameMessageOpcode::CharacterCreateResponse
                        .0
                        .to_le_bytes(),
                )
            })
        },
    )
    .await;
    let roster = fixture
        .runtime
        .bootstrap
        .store
        .players_for_account(account.id.0)
        .await
        .unwrap();
    assert_eq!(roster.len(), 1);
    let actor = EntityId(roster[0].object_id);
    let first_start = fixture.client.messages.len();
    let second_start = second.messages.len();
    let mut enter = bace_wire::Writer::new();
    enter.u32(bace_wire::opcode::GameMessageOpcode::CharacterEnterWorld.0);
    enter.u32(actor.0);
    enter.string16("AcctTwo").unwrap();
    second.send_message(&fixture.runtime, key, &enter.into_bytes());
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |runtime, first, second| {
            runtime.players.entered(actor)
                && first.has_create(actor.0, first_start)
                && second.has_create(actor.0, second_start)
                && second.has_create(fixture.binding.actor.0, second_start)
        },
    )
    .await;
    second.assert_entry_order(actor.0, second_start);

    // Change a real player UI bit, then require logout's durable save before
    // reopening this account and checking the frozen value and login instance.
    let original = fixture.runtime.sessions[&key]
        .loading
        .as_ref()
        .unwrap()
        .loaded
        .player
        .player
        .metadata
        .options1;
    let enabled = original & 2 == 0;
    let mut payload = bace_wire::Writer::new();
    payload.u32(0); // Pinned CharacterOption 0 maps to options1 bit 2.
    payload.u32(u32::from(enabled));
    let ui = bace_wire::GameActionEnvelope {
        sequence: 1,
        action: bace_wire::opcode::GameActionType::SetSingleCharacterOption,
        payload: &payload.into_bytes(),
    }
    .encode(1024)
    .unwrap();
    second.send_message(&fixture.runtime, key, &ui);
    let mut logoff = bace_wire::Writer::new();
    logoff.u32(bace_wire::opcode::GameMessageOpcode::CharacterLogOff.0);
    second.send_message(&fixture.runtime, key, &logoff.into_bytes());
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |runtime, first, _| {
            !runtime.sessions.contains_key(&key)
                && !runtime.players.entered(actor)
                && first.has_delete(actor.0, first_start)
        },
    )
    .await;
    drop(second);

    let mut reconnect = client::Client::new(&fixture.runtime, "AcctTwo");
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut reconnect,
        |runtime, _, _| {
            runtime
                .sessions
                .values()
                .any(|s| s.connected && s.account.id == account.id)
        },
    )
    .await;
    let new_key = *fixture
        .runtime
        .sessions
        .iter()
        .find(|(_, s)| s.account.id == account.id)
        .unwrap()
        .0;
    assert_ne!(key.generation, new_key.generation);
    reconnect.bind(new_key);
    let first_reentry = fixture.client.messages.len();
    let mut enter = bace_wire::Writer::new();
    enter.u32(bace_wire::opcode::GameMessageOpcode::CharacterEnterWorld.0);
    enter.u32(actor.0);
    enter.string16("AcctTwo").unwrap();
    reconnect.send_message(&fixture.runtime, new_key, &enter.into_bytes());
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut reconnect,
        |runtime, first, _| {
            runtime.players.entered(actor) && first.has_create(actor.0, first_reentry)
        },
    )
    .await;
    let loading = fixture.runtime.sessions[&new_key].loading.as_ref().unwrap();
    assert_eq!(loading.receipt.unwrap().total_logins, 2);
    assert_eq!(
        loading.loaded.player.player.metadata.options1 & 2 != 0,
        enabled
    );
    let mut logoff = bace_wire::Writer::new();
    logoff.u32(bace_wire::opcode::GameMessageOpcode::CharacterLogOff.0);
    reconnect.send_message(&fixture.runtime, new_key, &logoff.into_bytes());
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut reconnect,
        |runtime, _, _| !runtime.sessions.contains_key(&new_key),
    )
    .await;
    drop(reconnect);
    fixture.shutdown().await;
}

impl EnteredFixture {
    pub async fn shutdown(self) {
        let Self {
            mut runtime,
            key,
            binding: _,
            mut client,
            creation_color: _,
            _directory,
            _cluster,
        } = self;
        runtime.input.push_back(NetworkEvent::Terminated {
            key,
            reason: crate::network::NetworkStopReason::PeerDisconnected,
        });
        runtime.quiesce_now().unwrap();
        poll_until(&mut runtime, &mut client, |runtime, _| {
            !runtime.requires_drain()
        })
        .await;
        let mut shutdown = runtime
            .into_shutdown()
            .unwrap_or_else(|(e, _)| panic!("{e}"));
        tokio::time::timeout(Duration::from_secs(60), async {
            while !shutdown.poll().unwrap() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        assert!(shutdown.complete());
        drop((_directory, _cluster));
    }
}
#[test]
#[ignore = "requires accepted full native pack; source diagnostic"]
fn academy_researcher_native_quality_evidence() {
    let path =
        PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").expect("accepted native pack"));
    let manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = manifest
        .open(path.parent().unwrap(), Default::default())
        .unwrap();
    let bace_storage_codec::PackLookup::Record(record) = generation
        .lookup(bace_storage_codec::PackKey {
            namespace: 1,
            id: 30997,
        })
        .unwrap()
    else {
        panic!("native NPC");
    };
    let source: bace_content::WeenieV1 = bace_content_tools::decode(record.bytes()).unwrap();
    println!(
        "{} type={} bools={:?} ints={:?} data_ids={:?}",
        source.class_name,
        source.weenie_type,
        source.properties.bools,
        source.properties.ints,
        source.properties.data_ids
    );
    assert!(source.properties.data_ids.iter().all(|p| p.id != 4));
}
