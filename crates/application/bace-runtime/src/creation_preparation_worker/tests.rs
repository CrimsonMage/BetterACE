use super::*;
use bace_storage_codec::{PackKey, PackLimits, PackManifest, PackRecord};
fn request(generation: Arc<PackGeneration>, correlation: u64) -> CreationPreparationRequest {
    CreationPreparationRequest{correlation,key:SessionKey{id:1,generation:1},account:Arc::new(bace_auth::AccountRecord{id:AccountId(1),name:bace_auth::AccountName::parse("creator").unwrap(),password_hash:bace_auth::PasswordHashRecord::parse("$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").unwrap(),access_level:bace_auth::AccessLevel::Player,disabled:false}),wire:wire(),entity:EntityId(0x50000001),item_ids:vec![0x80000001],slot:0,maximum_slots:11,generation,profile:Arc::new(crate::creation_profile::builtin_creation_profile().unwrap()),now_unix_millis:0}
}
fn wire() -> bace_wire::CharacterCreateRequest {
    bace_wire::CharacterCreateRequest {
        account: "creator".into(),
        unknown_constant: 1,
        heritage: 1,
        gender: 1,
        appearance: bace_wire::CharacterAppearance {
            eyes: 0,
            nose: 0,
            mouth: 0,
            hair_color: 0,
            eye_color: 0,
            hair_style: 0,
            headgear_style: u32::MAX,
            headgear_color: 0,
            shirt_style: 0,
            shirt_color: 0,
            pants_style: 0,
            pants_color: 0,
            footwear_style: 0,
            footwear_color: 0,
            skin_hue: 0.5,
            hair_hue: 0.5,
            headgear_hue: 0.5,
            shirt_hue: 0.5,
            pants_hue: 0.5,
            footwear_hue: 0.5,
        },
        template_option: 0,
        abilities: bace_wire::CharacterAbilities {
            strength: 10,
            endurance: 10,
            coordination: 10,
            quickness: 10,
            focus: 10,
            self_ability: 10,
        },
        character_slot: 0,
        class_id: 0,
        skill_advancement_classes: vec![0; 55],
        name: "Nativeprobe".into(),
        start_area: 0,
        requested_admin: false,
        requested_sentinel: false,
        trailing_bytes: 0,
    }
}
fn fixture() -> (tempfile::TempDir, Arc<PackGeneration>) {
    let dir = tempfile::tempdir().unwrap();
    let base = bace_storage_codec::compile_pack(
        dir.path(),
        [Ok(PackRecord {
            key: PackKey {
                namespace: 1,
                id: 1,
            },
            schema: 1,
            value: Some(vec![0]),
        })],
        PackLimits::default(),
    )
    .unwrap();
    let generation = PackManifest {
        version: 1,
        generation: 1,
        base,
        deltas: vec![],
    }
    .open(dir.path(), PackLimits::default())
    .unwrap();
    (dir, Arc::new(generation))
}
#[test]
fn running_and_unclaimed_results_retain_capacity_and_exact_retry_inputs() {
    let (_dir, generation) = fixture();
    let (started, start) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let mut worker = CreationPreparationWorker::start_with(1, move |request| {
        started.send(()).unwrap();
        gate.recv().unwrap();
        CreationPreparationCompletion {
            correlation: request.correlation,
            key: request.key,
            account: request.account.id,
            entity: request.entity,
            slot: request.slot,
            generation: request.generation.clone(),
            result: Err(crate::creation_assets::CreationFailure::unavailable(
                "missing admitted asset",
            )),
            request: Box::new(request),
        }
    })
    .unwrap();
    let mut first = request(generation.clone(), 1);
    first.key.id = 0; // The real network registry's first allocated slot is zero.
    assert!(worker.try_submit(first).is_ok());
    start.recv().unwrap();
    let refused = worker
        .try_submit(request(generation.clone(), 2))
        .err()
        .unwrap();
    assert_eq!(refused.item_ids, [0x80000001]);
    assert!(Arc::ptr_eq(&refused.generation, &generation));
    let held = worker.try_shutdown().err().unwrap();
    worker = *held;
    release.send(()).unwrap();
    // Capacity counts the outstanding receipt even once the worker has finished.
    assert_eq!(worker.pending(), 1);
    assert!(worker.try_submit(*refused).is_err());
    let completion = loop {
        if let Some(value) = worker.try_recv().unwrap() {
            break value;
        }
        thread::yield_now();
    };
    assert!(completion.result.is_err());
    assert_eq!(completion.key.id, 0);
    assert_eq!(completion.request.now_unix_millis, 0);
    assert_eq!(completion.request.item_ids, [0x80000001]);
    assert_eq!(worker.pending(), 0);
    worker.try_shutdown().ok().unwrap().join().unwrap();
}
#[test]
fn malformed_authenticated_headers_never_enter_cold_lane() {
    let (_dir, generation) = fixture();
    let mut worker =
        CreationPreparationWorker::start_with(1, |_| panic!("invalid request entered worker"))
            .unwrap();
    for case in 0..5 {
        let mut r = request(generation.clone(), 1);
        match case {
            0 => r.correlation = 0,
            1 => r.wire.account = "different".into(),
            2 => r.wire.character_slot = 11,
            3 => r.now_unix_millis = u64::MAX,
            _ => r.entity = EntityId(1),
        }
        assert!(worker.try_submit(r).is_err());
    }
    assert_eq!(worker.pending(), 0);
    worker.try_shutdown().ok().unwrap().join().unwrap();
}
#[test]
#[ignore = "requires approved DATs and accepted native full world pack"]
fn real_dat_native_closure_name_geometry_and_freeze() {
    let manifest_path =
        std::path::PathBuf::from(std::env::var_os("BACE_CREATION_PACK_MANIFEST").unwrap());
    let limits = PackLimits::default();
    let manifest = PackManifest::decode(&std::fs::read(&manifest_path).unwrap(), limits).unwrap();
    let generation = Arc::new(
        manifest
            .open(manifest_path.parent().unwrap(), limits)
            .unwrap(),
    );
    let dat = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let manifest = RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let mut r = request(generation, 1);
    let closure = assets
        .prepare_creation_assets(&r.generation, 1, &r.profile)
        .unwrap();
    for id in closure.character.skill_table().skills.keys() {
        if bace_character::is_locked_skill(*id) && (*id as usize) < 55 {
            r.wire.skill_advancement_classes[*id as usize] = 2;
        }
    }
    let color = closure.creation.genders[0].clothing_colors[0];
    r.wire.appearance.shirt_color = color;
    r.wire.appearance.pants_color = color;
    r.wire.appearance.footwear_color = color;
    r.item_ids = (0x80000001..0x80000400).collect();
    let prepared = prepare(&mut assets, &r).unwrap();
    assert!(!prepared.frozen.items.is_empty());
    assert_eq!(
        prepared
            .frozen
            .player
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|p| p.id == 43)
            .unwrap()
            .value,
        "01 January 1970"
    );
    assert!(prepared.unused_item_ids.len() < r.item_ids.len());
}
