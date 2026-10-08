use bace_runtime::region_activation::{RegionAssetManifest, VerifiedRegionAssets};
use std::{path::PathBuf, sync::Arc};

#[test]
#[ignore = "requires approved BACE_DAT_DIRECTORY and complete BACE_WORLD_MANIFEST with derived indexes"]
fn complete_accepted_world_and_dat_prepare_real_startup_dependencies() {
    let directory = PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let path =
        PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").expect("accepted world manifest"));
    let manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let generation = Arc::new(
        manifest
            .open(path.parent().unwrap(), Default::default())
            .unwrap(),
    );
    let approved = RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&approved).unwrap();
    let prepared = assets
        .prepare_runtime_startup_assets(&generation, Default::default())
        .unwrap();
    assert!(prepared.runtime.spell_rows.len() > 5000);
    assert!(!prepared.runtime.projectile_shapes.is_empty());
    assert!(!prepared.runtime.component_templates.is_empty());
    assert!(prepared.treasure.templates.len() > 1400);
    assert!(!prepared.treasure.scrolls_by_spell.is_empty());
    assert_eq!(prepared.creatures.corpse_decay_ticks, 5400);
    let source = prepared.treasure.templates.len();
    let rows = prepared.runtime.spell_rows.len();
    let shapes = prepared.runtime.projectile_shapes.len();
    println!(
        "startup closure: {source} treasure templates, {rows} spell rows, {shapes} projectile shapes"
    );
}
