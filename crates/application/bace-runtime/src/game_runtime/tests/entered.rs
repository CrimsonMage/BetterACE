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
#[path = "entered/appraisal.rs"]
mod appraisal;
mod client;
#[path = "entered/healing.rs"]
mod healing;
#[path = "entered/inventory.rs"]
mod inventory;
#[path = "entered/quickslots.rs"]
mod quickslots;
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
    fixture_with_early_action(records, start_area, false, false, false).await
}
pub(in crate::game_runtime) async fn fixture_with_healer() -> EnteredFixture {
    fixture_with_early_action(Vec::new(), 0, false, true, false).await
}
pub(in crate::game_runtime) async fn fixture_with_twohanded() -> EnteredFixture {
    fixture_with_early_action(Vec::new(), 0, false, false, true).await
}
async fn fixture_with_early_action(
    mut records: Vec<bace_storage_codec::PackRecord>,
    start_area: u32,
    early_login_complete: bool,
    starter_healer: bool,
    twohanded: bool,
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
    let fixture_root = std::env::var_os("BACE_ENTRY_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../.local/entry-fixtures")
        });
    std::fs::create_dir_all(&fixture_root).unwrap();
    let directory = tempfile::tempdir_in(fixture_root).unwrap();
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
    if starter_healer {
        let key = bace_storage_codec::PackKey {
            namespace: 1,
            id: 12748,
        };
        let bace_storage_codec::PackLookup::Record(record) = source.lookup(key).unwrap() else {
            panic!("starter wand template missing");
        };
        let mut template: bace_content::WeenieV1 =
            bace_content_tools::decode(record.bytes()).unwrap();
        template.weenie_type = 28;
        for (id, value) in [(19, 90), (89, 1), (90, 20), (91, 3), (92, 3)] {
            if let Some(property) = template.properties.ints.iter_mut().find(|p| p.id == id) {
                property.value = value;
            } else {
                template
                    .properties
                    .ints
                    .push(bace_content::Property { id, value });
            }
        }
        if let Some(property) = template.properties.floats.iter_mut().find(|p| p.id == 100) {
            property.value = 1.0;
        } else {
            template.properties.floats.push(bace_content::Property {
                id: 100,
                value: 1.0,
            });
        }
        records.push(bace_storage_codec::PackRecord {
            key,
            schema: 1,
            value: Some(bace_content_tools::compile_template(&template).unwrap()),
        });
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
    let mut assets = crate::region_activation::RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
        store: None,
    };
    let portal =
        bace_dat::DatArchive::open_verified(&assets.portal, &assets.portal_sha256).unwrap();
    let cell = bace_dat::DatArchive::open_verified(&assets.cell, &assets.cell_sha256).unwrap();
    let language = bace_dat::DatArchive::open_verified(
        dat.join("client_local_English.dat"),
        "e85c820280c88fac7df6c8043f5e24596e9c8774193af4123d756546f78fb2bb",
    )
    .unwrap();
    assets.store = Some(Arc::new(
        bace_dat::DatStore::new(portal, cell, language).unwrap(),
    ));
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
        dat_directory: Some(dat.clone()),
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
        world_index: None,
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
    let diagnostics = Arc::new(
        bace_observability::LogStore::open(&directory.path().join("game-diagnostics")).unwrap(),
    );
    runtime.attach_diagnostic_log(diagnostics.clone());
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
    poll_until(&mut runtime, &mut client, |_, client| {
        [
            bace_wire::opcode::GameMessageOpcode::CharacterList.0,
            bace_wire::opcode::GameMessageOpcode::ServerName.0,
            bace_wire::opcode::GameMessageOpcode::DDD_Interrogation.0,
        ]
        .into_iter()
        .all(|opcode| {
            client
                .messages
                .iter()
                .any(|message| message.bytes.starts_with(&opcode.to_le_bytes()))
        })
    })
    .await;
    let list = client
        .messages
        .iter()
        .position(|message| {
            message.bytes.starts_with(
                &bace_wire::opcode::GameMessageOpcode::CharacterList
                    .0
                    .to_le_bytes(),
            )
        })
        .unwrap();
    let name = client
        .messages
        .iter()
        .position(|message| {
            message.bytes.starts_with(
                &bace_wire::opcode::GameMessageOpcode::ServerName
                    .0
                    .to_le_bytes(),
            )
        })
        .unwrap();
    let interrogation = client
        .messages
        .iter()
        .position(|message| {
            message.bytes.starts_with(
                &bace_wire::opcode::GameMessageOpcode::DDD_Interrogation
                    .0
                    .to_le_bytes(),
            )
        })
        .unwrap();
    assert!(
        list < name && name < interrogation,
        "authenticated CharacterList, ServerName, DDD interrogation order"
    );
    assert!(
        !runtime.dat.as_ref().unwrap().ready_for_world(key),
        "world entry stays fenced before DAT completion"
    );
    client.send_message_on_queue(&runtime, key, 5, &current_ddd_response(&runtime));
    poll_until(&mut runtime, &mut client, |_, client| {
        client.messages.iter().any(|message| {
            message.queue == 5 && message.bytes == bace_wire::DddControl::End.encode()
        })
    })
    .await;
    poll_until(&mut runtime, &mut client, |runtime, _| {
        runtime.dat.as_ref().unwrap().ready_for_world(key)
    })
    .await;
    // A stock client may echo EndDDD even when no BeginDDD/patch was sent.
    // The pinned ACE handler ignores it and keeps the authenticated session.
    // ACE dispatches DDD_EndDDD by opcode and state, independent of the
    // fragment's queue group. A stock client can echo it after a match.
    for queue in 0..12 {
        client.send_message_on_queue(&runtime, key, queue, &bace_wire::DddControl::End.encode());
    }
    for _ in 0..10 {
        let _ = runtime.poll(runtime.clock.monotonic.elapsed());
        client.drain(&runtime);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(runtime.sessions.get(&key).is_some_and(|s| !s.closing));
    assert!(!client.messages.iter().any(|message| {
        message.bytes.starts_with(
            &bace_wire::opcode::GameMessageOpcode::AccountBoot
                .0
                .to_le_bytes(),
        )
    }));
    eprintln!("entry fixture: creating character");
    let create = creation_message_with_start_area(
        runtime.sessions[&key].account.name.as_str(),
        color,
        "Entryprobe",
        start_area,
        twohanded,
        starter_healer,
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
    if starter_healer {
        let stored = runtime
            .bootstrap
            .store
            .load(actor.0)
            .await
            .unwrap()
            .unwrap();
        let mut saved = bace_storage_codec::PlayerSaveV6::decode_or_migrate(&stored.bytes).unwrap();
        let health = saved
            .player
            .entity
            .state
            .properties
            .secondary_attributes
            .iter_mut()
            .find(|p| p.id == 1)
            .expect("starter current health");
        assert!(health.value.current_level > 1);
        health.value.current_level -= 1;
        saved.player.entity.mutation_revision += 1;
        let lease = runtime
            .bootstrap
            .store
            .character_lease(actor.0)
            .await
            .unwrap()
            .unwrap();
        runtime
            .bootstrap
            .store
            .save_offline(
                lease,
                &bace_persistence::SaveSnapshot {
                    object_id: actor.0,
                    mutation_revision: saved.player.entity.mutation_revision,
                    expected_version: stored.persisted_version,
                    bytes: saved.encode().unwrap(),
                },
            )
            .await
            .unwrap();
    }
    eprintln!("entry fixture: entering {actor:?}");
    // ACE's character selection handshake asks whether the world server is
    // ready before sending the selected character and account name.
    let ready_start = client.messages.len();
    client.send_message(
        &runtime,
        key,
        &bace_wire::opcode::GameMessageOpcode::CharacterEnterWorldRequest
            .0
            .to_le_bytes(),
    );
    poll_until(&mut runtime, &mut client, |_, client| {
        client.messages.iter().skip(ready_start).any(|message| {
            message.queue == 9
                && message.bytes
                    == bace_wire::opcode::GameMessageOpcode::CharacterEnterWorldServerReady
                        .0
                        .to_le_bytes()
        })
    })
    .await;
    let entry_start = client.messages.len();
    let mut request = bace_wire::Writer::new();
    request.u32(bace_wire::opcode::GameMessageOpcode::CharacterEnterWorld.0);
    request.u32(actor.0);
    request
        .string16(runtime.sessions[&key].account.name.as_str())
        .unwrap();
    client.send_message(&runtime, key, &request.into_bytes());
    if early_login_complete {
        poll_until(&mut runtime, &mut client, |runtime, _| {
            runtime.entry_pending.contains(&key)
        })
        .await;
        let action = bace_wire::GameActionEnvelope {
            sequence: 1,
            action: bace_wire::opcode::GameActionType::LoginComplete,
            payload: &[],
        }
        .encode(1024)
        .unwrap();
        client.send_message_on_queue(&runtime, key, 3, &action);
        let allegiance = bace_wire::GameActionEnvelope {
            sequence: 2,
            action: bace_wire::opcode::GameActionType::AllegianceUpdateRequest,
            payload: &0u32.to_le_bytes(),
        }
        .encode(1024)
        .unwrap();
        client.send_message_on_queue(&runtime, key, 3, &allegiance);
    }
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
    if early_login_complete {
        assert!(!runtime.sessions[&key].terminated);
        poll_until(&mut runtime, &mut client, |_, client| {
            client.messages.iter().skip(entry_start).any(|message| {
                message.queue == 9
                    && message.bytes.get(..4)
                        == Some(
                            &bace_wire::opcode::GameMessageOpcode::GameEvent
                                .0
                                .to_le_bytes(),
                        )
                    && message.bytes.get(12..16)
                        == Some(
                            &bace_wire::opcode::GameEventType::AllegianceAllegianceUpdateDone
                                .0
                                .to_le_bytes(),
                        )
            })
        })
        .await;
        poll_until(&mut runtime, &mut client, |runtime, _| {
            !runtime.entry_pending.contains(&key) && runtime.input.is_empty()
        })
        .await;
        assert!(!runtime.sessions[&key].terminated);
        assert!(
            diagnostics
                .recent(0)
                .records
                .iter()
                .any(|record| record.event == "game.entry_action_deferred")
        );
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let events: Vec<_> = diagnostics
                .recent(0)
                .records
                .into_iter()
                .map(|record| record.event)
                .collect();
            if [
                "game.character_create_request",
                "game.character_created",
                "game.world_server_ready",
                "game.enter_world_request",
                "game.character_loaded",
                "game.world_entered",
            ]
            .into_iter()
            .all(|event| events.iter().any(|recorded| recorded == event))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("creation and entry milestones reached the game diagnostic log");
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

fn current_ddd_response(runtime: &GameRuntime) -> Vec<u8> {
    let dat = runtime.dat.as_ref().expect("admitted DAT catalog");
    let mut writer = bace_wire::Writer::new();
    writer.u32(bace_wire::opcode::GameMessageOpcode::DDD_InterrogationResponse.0);
    writer.u32(1); // ACE English client language.
    writer.u32(3);
    for database in [
        bace_wire::DddDatabase::Portal,
        bace_wire::DddDatabase::Language,
        bace_wire::DddDatabase::Cell,
    ] {
        let (kind, id) = database.wire_identity();
        let iteration = dat
            .database_iteration(database)
            .expect("approved DAT iteration");
        writer.u32(kind);
        writer.u32(id);
        writer.u32(iteration);
        if iteration > 1 {
            writer.u32((-(iteration as i32)) as u32);
        }
        writer.u32(1);
    }
    writer.into_bytes()
}
pub(in crate::game_runtime) async fn poll_until(
    runtime: &mut GameRuntime,
    client: &mut client::Client,
    ready: impl Fn(&GameRuntime, &client::Client) -> bool,
) {
    let mut degraded_polls = 0u64;
    let mut last_degraded = None;
    let started = std::time::Instant::now();
    let mut diagnosed = false;
    let progress = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            // Production records a degraded service poll and keeps serving
            // independent session/lifecycle lanes. An unrelated generator
            // hold must not end this real UDP entry replay early.
            if let Err(error) = runtime.poll(runtime.clock.monotonic.elapsed()) {
                degraded_polls += 1;
                if degraded_polls == 1 {
                    eprintln!("entry fixture first degraded poll: {error}");
                }
                last_degraded = Some(error.to_string());
            }
            client.drain(runtime);
            if !diagnosed && started.elapsed() > Duration::from_secs(10) {
                diagnosed = true;
                eprintln!(
                    "entry wait: messages={:?}, retained={}, dat_admissions={}, staff_phase={:?}, staff_failure={:?}, world_job={}, world_pending={}, sim_finished={}, drain_reason={}, healing_pending={}, sessions={}, logout={}, request_regions={}",
                    client
                        .messages
                        .iter()
                        .rev()
                        .take(8)
                        .map(|m| (
                            m.queue,
                            m.bytes
                                .get(..4)
                                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                        ))
                        .collect::<Vec<_>>(),
                    runtime.network_output.len(),
                    runtime
                        .dat
                        .as_ref()
                        .map_or(0, ddd::DatRuntime::pending_admissions),
                    runtime.staff_pending_phase(runtime.sessions.keys().next().copied().unwrap()),
                    runtime.staff_failure(),
                    runtime.world_job.is_some(),
                    runtime.world.as_ref().is_some_and(|world| world.pending.is_some()),
                    runtime.simulation.is_finished(),
                    runtime.pending_drain_reason(),
                    runtime.healing_kits.has_pending(),
                    runtime.sessions.len(),
                    runtime.logout.len(),
                    runtime.request_regions.len(),
                );
            }
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
    .await;
    assert!(
        progress.is_ok(),
        "bounded real entry progress; degraded polls {degraded_polls}, last {last_degraded:?}; world job {}, preparation pending {}; player failures {:?}; loading {:?}",
        runtime.world_job.is_some(),
        runtime.preparation.pending(),
        runtime
            .sessions
            .iter()
            .map(|(key, session)| (*key, session.failure.clone()))
            .collect::<Vec<_>>(),
        runtime
            .sessions
            .iter()
            .map(|(key, session)| {
                (
                    *key,
                    session
                        .loading
                        .as_ref()
                        .map(|load| (load.phase as u8, load.cold_token, load.region_requested)),
                )
            })
            .collect::<Vec<_>>()
    );
    if degraded_polls != 0 {
        eprintln!("entry replay: {degraded_polls} degraded polls; last {last_degraded:?}");
    }
}
fn creation_message(account: &str, color: u32, name: &str) -> Vec<u8> {
    creation_message_with_start_area(account, color, name, 0, false, false)
}
fn creation_message_with_start_area(
    account: &str,
    color: u32,
    name: &str,
    start_area: u32,
    twohanded: bool,
    starter_healer: bool,
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
            if bace_character::is_locked_skill(id)
                || [16, 33].contains(&id)
                || twohanded && id == 41
                || starter_healer && id == 21
            {
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

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn early_retail_login_complete_waits_for_world_entry() {
    let fixture = Box::pin(fixture_with_early_action(Vec::new(), 0, true, false, false)).await;
    assert!(fixture.runtime.players.entered(fixture.binding.actor));
    fixture.shutdown().await;
}

async fn poll_until_pair(
    runtime: &mut GameRuntime,
    first: &mut client::Client,
    second: &mut client::Client,
    ready: impl Fn(&GameRuntime, &client::Client, &client::Client) -> bool,
) {
    let mut degraded_polls = 0u64;
    let mut last_degraded = None;
    let progress = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            if let Err(error) = runtime.poll(runtime.clock.monotonic.elapsed()) {
                degraded_polls += 1;
                last_degraded = Some(error.to_string());
            }
            first.drain(runtime);
            second.drain(runtime);
            if let Some((key, session)) = runtime
                .sessions
                .iter()
                .find(|(_, session)| session.failure.is_some())
            {
                panic!(
                    "two-player entry {key:?} account {} phase {:?}: {}",
                    session.account.name.as_str(),
                    session.loading.as_ref().map(|loading| loading.phase as u8),
                    session.failure.as_deref().unwrap()
                );
            }
            if ready(runtime, first, second) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
    assert!(
        progress.is_ok(),
        "bounded two-player entry progress; degraded polls {degraded_polls}, last {last_degraded:?}; first messages {}, second messages {}, login job {}, queue {}, preparation {}, world job {}; sessions {:?}",
        first.messages.len(),
        second.messages.len(),
        runtime.login_job.is_some(),
        runtime.login_queue.len(),
        runtime.preparation.pending(),
        runtime.world_job.is_some(),
        runtime
            .sessions
            .iter()
            .map(|(key, session)| {
                (
                    *key,
                    session.account.name.as_str(),
                    session.connected,
                    session.terminated,
                    session.loading.as_ref().map(|load| load.phase as u8),
                )
            })
            .collect::<Vec<_>>()
    );
    if degraded_polls != 0 {
        eprintln!("two-player replay: {degraded_polls} degraded polls; last {last_degraded:?}");
    }
}

#[tokio::test]
#[ignore = "requires approved DATs, accepted full native pack and PostgreSQL binaries"]
async fn disconnected_failed_session_saves_and_releases_online_lease() {
    let mut fixture = Box::pin(fixture()).await;
    let key = fixture.key;
    let actor = fixture.binding.actor.0;
    fixture.runtime.sessions.get_mut(&key).unwrap().failure =
        Some("retained visibility projection".into());
    fixture.runtime.input.push_back(NetworkEvent::Terminated {
        key,
        reason: crate::network::NetworkStopReason::PeerDisconnected,
    });
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let _ = fixture
                .runtime
                .poll(fixture.runtime.clock.monotonic.elapsed());
            fixture.client.drain(&fixture.runtime);
            let lease = fixture
                .runtime
                .bootstrap
                .store
                .character_lease(actor)
                .await
                .unwrap();
            if !fixture.runtime.sessions.contains_key(&key)
                && lease
                    .is_some_and(|lease| lease.state == bace_persistence::OwnershipState::Offline)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("failed disconnected session must release its durable lease");
    fixture.shutdown().await;
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
    let mut second = client::Client::new(&fixture.runtime, account.name.as_str());
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
    eprintln!("two-player replay: second UDP authenticated");
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
    eprintln!("two-player replay: login preparation idle");
    second.send_message(
        &fixture.runtime,
        key,
        &creation_message(account.name.as_str(), fixture.creation_color, "Entrytwo"),
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
    eprintln!("two-player replay: second durable creation response");
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
    enter.string16(account.name.as_str()).unwrap();
    second.send_message(&fixture.runtime, key, &enter.into_bytes());
    eprintln!("two-player replay: second EnterWorld sent");
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
    eprintln!("two-player replay: reciprocal create visibility");
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
        |runtime, first, second| {
            runtime
                .sessions
                .get(&key)
                .is_some_and(|s| s.loading.is_none())
                && !runtime.players.entered(actor)
                && first.has_delete(actor.0, first_start)
                && second.messages.iter().any(|message| {
                    message.queue == 9
                        && message.bytes
                            == bace_wire::opcode::GameMessageOpcode::CharacterLogOff
                                .0
                                .to_le_bytes()
                })
        },
    )
    .await;
    eprintln!("two-player replay: second logged off after save");
    second.disconnect(&fixture.runtime, key);
    poll_until_pair(
        &mut fixture.runtime,
        &mut fixture.client,
        &mut second,
        |runtime, _, _| !runtime.sessions.contains_key(&key),
    )
    .await;
    eprintln!("two-player replay: second UDP disconnected");
    drop(second);

    let mut reconnect = client::Client::new(&fixture.runtime, account.name.as_str());
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
    eprintln!("two-player replay: second UDP reauthenticated");
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
    enter.string16(account.name.as_str()).unwrap();
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
    eprintln!("two-player replay: second reentered with saved UI");
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
        |runtime, _, reconnect| {
            runtime
                .sessions
                .get(&new_key)
                .is_some_and(|session| session.loading.is_none())
                && reconnect.messages.iter().any(|message| {
                    message.queue == 9
                        && message.bytes
                            == bace_wire::opcode::GameMessageOpcode::CharacterLogOff
                                .0
                                .to_le_bytes()
                })
        },
    )
    .await;
    reconnect.disconnect(&fixture.runtime, new_key);
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
