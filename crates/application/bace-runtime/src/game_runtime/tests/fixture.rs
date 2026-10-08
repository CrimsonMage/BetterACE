//! Idle adapter fixture: missing DATs intentionally cannot admit actors. Real
//! PostgreSQL, pack mappings, services, network and simulation owners are used.
use super::*;
use std::sync::atomic::AtomicBool;
pub(in crate::game_runtime) async fn fixture() -> (
    crate::player_service::tests::cluster::Cluster,
    tempfile::TempDir,
    Box<GameRuntime>,
) {
    let cluster = crate::player_service::tests::cluster::Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 4)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let key = directory.path().join("fixture.key");
    crate::random_keys::initialize_random_key(&key, 1).unwrap();
    let random = crate::game_random::load_and_bind(&store, &key)
        .await
        .unwrap();
    let template = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "fixture_generic".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    let built = bace_content_tools::build_world_pack(
        &[template],
        &[],
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
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
    let save_pressure = Arc::new(AtomicBool::new(false));
    let mut config = bace_config::ServerConfig {
        max_sessions: 2,
        command_capacity: 16,
        bind_address: "127.0.0.1:0".into(),
        pack_directory: Some(directory.path().to_owned()),
        ..Default::default()
    };
    config.world.preloading.enabled = false;
    let bootstrap = GameBootstrap {
        world_settings: crate::world_settings::prepare_world_settings(&config.world).unwrap(),
        config,
        world_owner: store.acquire_world_owner().await.unwrap(),
        store,
        random,
        assets: crate::region_activation::RegionAssetManifest {
            portal: directory.path().join("missing-portal.dat"),
            cell: directory.path().join("missing-cell.dat"),
            portal_sha256: String::new(),
            cell_sha256: String::new(),
        },
        pack: crate::pack_io::PreparedPack {
            manifest,
            generation,
        },
        pack_io: Some(
            crate::pack_io::PackIoWorker::start(
                directory.path().to_owned(),
                1,
                save_pressure.clone(),
            )
            .unwrap(),
        ),
        save_pressure,
        allegiance_nodes: vec![],
        allegiance_metadata: vec![],
    };
    let recall_locations = crate::portal_preparation::prepare_recall_locations(
        &bootstrap.pack.generation,
        &bace_content::TemplateClassIndexV1 {
            schema_version: 1,
            entries: vec![],
        },
    )
    .unwrap();
    let assets = GameRuntimeStartupAssets {
        recall_locations: Arc::new(recall_locations),
        runtime: GameRuntimeAssets {
            world_open: true,
            local_offset_seconds: 0,
            staff_definitions: Arc::from([]),
            spell_rows: Arc::new(BTreeMap::new()),
            component_templates: Arc::from([]),
            projectile_shapes: Arc::new(BTreeMap::new()),
            projectile_visibility: Arc::new(BTreeMap::new()),
            creatures: crate::generator_preparation::CreatureAdmissionPolicy {
                corpse_template: 1,
                think_interval: 1,
                corpse_decay_ticks: 30,
            },
            policy: Default::default(),
        },
        staff_magic: Arc::new(crate::staff_magic_assets::PreparedStaffMagicAssets {
            definitions: vec![],
            plans: vec![],
            enchantments: vec![],
        }),
        levels: Arc::new(
            bace_character::CharacterLevelTable::prepare(vec![0, 100], vec![0, 1]).unwrap(),
        ),
        treasure: Arc::new(Default::default()),
        creatures: crate::generator_preparation::CreatureAdmissionPolicy {
            corpse_template: 1,
            think_interval: 1,
            corpse_decay_ticks: 30,
        },
        generators: Default::default(),
        aetheria_drop_rate: 0.0,
    };
    let clock = GameClock::from_pair(1_800_000_000_000, std::time::Instant::now()).unwrap();
    let mut startup = GameRuntimeStartup::new(
        bootstrap,
        assets,
        GameRuntimeLimits {
            sessions: 2,
            loading: 1,
            messages: 16,
            message_bytes: 1 << 20,
            work_per_poll: 16,
        },
        clock,
    )
    .unwrap_or_else(|(e, _)| panic!("{e}"));
    // Port zero chooses a fresh client port, but the required adjacent server
    // port can be occupied by another parallel test. Keep the first socket
    // owned by the network worker during each attempt and retry only that
    // transient bind failure; prebinding and releasing a pair would race too.
    for _ in 0..64 {
        match startup.advance().await {
            Ok(Some(runtime)) => return (cluster, directory, runtime),
            Ok(None) => {}
            Err(error)
                if error.starts_with("network startup failed: ") && {
                    let description = error.to_ascii_lowercase();
                    description.contains("address in use")
                        || description.contains("address already in use")
                        || description.contains("only one usage of each socket address")
                } => {}
            Err(error) => panic!("{error}"),
        }
    }
    panic!("bounded startup did not finish after ephemeral network bind retries")
}
