//! Authenticated cold-program preparation and retained worker failures. Synthetic
//! server rows qualify integration only; DAT formula/motion behavior is real.
use bace_runtime::{region_activation::RegionAssetManifest, staff_native_magic::*};
use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
fn source() -> bace_content::WeenieV1 {
    serde_json::from_value(serde_json::json!({"schema_version":1,"weenie_id":1,"class_name":"avatar_fixture","weenie_type":10,"properties":{"data_ids":[{"id":2,"value":0x09000001u32}]}})).unwrap()
}
fn generation(directory: &std::path::Path) -> Arc<bace_storage_codec::PackGeneration> {
    let built =
        bace_content_tools::build_world_pack(&[source()], &[], directory, &AtomicBool::new(false))
            .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    Arc::new(manifest.open(directory, Default::default()).unwrap())
}
fn work(id: u32) -> StaffNativeMagicWork {
    StaffNativeMagicWork{token:7,binding:bace_gameplay_api::CharacterBinding{actor:bace_types::EntityId(1),account:bace_types::AccountId(1),session:bace_gameplay_api::SessionId(1)},expected_character_revision:9,source:Arc::new(source()),account_cp1252:b"native-test".to_vec(),top_level_templates:vec![],row:Arc::new(serde_json::from_value(serde_json::json!({"id":id,"name":"fixture","last_modified":"fixture","damage_type":128,"boost":10,"boost_variance":5})).unwrap())}
}
#[test]
fn native_worker_rejects_missing_assets_and_retains_exact_request_through_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = RegionAssetManifest {
        portal: directory.path().join("missing.dat"),
        cell: directory.path().join("missing-cell.dat"),
        portal_sha256: "00".repeat(32),
        cell_sha256: "00".repeat(32),
    };
    let mut owner = StaffNativeMagicWorker::start(manifest, generation(directory.path())).unwrap();
    let work = Arc::new(work(1));
    assert!(owner.submit(work.clone()).is_ok());
    assert!(owner.submit(work.clone()).is_err());
    let mut owner = match owner.shutdown() {
        Err(owner) => *owner,
        Ok(()) => panic!("pending work must remain owned"),
    };
    let mut result = None;
    for _ in 0..200 {
        if let Some(value) = owner.poll().unwrap() {
            result = Some(value);
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = result.expect("worker completion");
    assert!(Arc::ptr_eq(&result.work, &work));
    assert!(result.result.is_err());
    assert!(owner.shutdown().is_ok());
}
#[test]
#[ignore = "requires approved user DAT archive; set BACE_DAT_DIRECTORY"]
fn actual_dat_native_program_retains_actor_formula_and_verified_motion() {
    let directory = tempfile::tempdir().unwrap();
    let dat =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: dat.join("client_portal.dat"),
        cell: dat.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: String::new(),
    };
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let spells =
        bace_dat::SpellTable::decode(&portal.read(bace_dat::SpellTable::RECORD_ID).unwrap())
            .unwrap();
    let (&id, base) = spells
        .spells
        .iter()
        .find(|(_, s)| {
            s.meta_type == 3 && s.school == 2 && s.flags & 4 != 0 && !s.formula.is_empty()
        })
        .unwrap();
    let mut assets = StaffNativeMagicAssets::open(&manifest, generation(directory.path())).unwrap();
    let work = work(id);
    let a = assets.prepare(&work).unwrap();
    assert_eq!(a.binding, work.binding);
    assert_eq!(a.expected_character_revision, 9);
    assert!(!a.definition.spell.gestures.is_empty());
    assert!(a.definition.spell.gestures.iter().all(|g| {
        g.motion_chain
            .as_ref()
            .is_some_and(|c| c.is_rootless() && c.stop_chain().is_some())
    }));
    let components = bace_dat::SpellComponents::decode(
        &portal.read(bace_dat::SpellComponents::RECORD_ID).unwrap(),
    )
    .unwrap();
    let mapper = bace_dat::DualDidMapper::decode(&portal.read(0x27000002).unwrap()).unwrap();
    let expected = bace_runtime::magic_preparation::prepare_component_plan(
        base,
        &components,
        &mapper,
        &work.account_cp1252,
        [false; 5],
        &[],
    )
    .unwrap();
    assert_eq!(a.definition.spell.components, expected.requirements);
    let mut b_work = work.clone();
    b_work.account_cp1252 = b"second-account".to_vec();
    b_work.binding.actor = bace_types::EntityId(2);
    b_work.binding.account = bace_types::AccountId(2);
    b_work.top_level_templates = vec![15270];
    let b = assets.prepare(&b_work).unwrap();
    let expected = bace_runtime::magic_preparation::prepare_component_plan(
        base,
        &components,
        &mapper,
        &b_work.account_cp1252,
        [false; 5],
        &[15270],
    )
    .unwrap();
    assert_eq!(b.definition.spell.components, expected.requirements);
    assert_eq!(a.definition.spell.spell, b.definition.spell.spell);
    assert_eq!(a.definition.spell.gestures, b.definition.spell.gestures);
    assert_eq!(
        assets.prepare(&work).unwrap().definition.spell,
        a.definition.spell
    );
}
