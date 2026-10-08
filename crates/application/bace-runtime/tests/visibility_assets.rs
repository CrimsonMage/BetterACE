//! Actual approved DAT PVS metadata is preserved with the collision generation.
use bace_runtime::region_activation::{RegionAssetManifest, VerifiedRegionAssets};
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn complete_region_pvs_matches_raw_env_cells_and_has_closed_references() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: dir.join("client_portal.dat"),
        cell: dir.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let (geometry, rows) = assets.prepare_geometry_with_visibility(0xa260).unwrap();
    assert!(!rows.is_empty());
    let mut cell = bace_dat::DatArchive::open(&manifest.cell).unwrap();
    for row in &rows {
        assert!(geometry.cell(row.cell.0).is_some());
        let raw = bace_dat::EnvCell::decode(&cell.read(row.cell.0).unwrap()).unwrap();
        assert_eq!(row.seen_outside, raw.flags & 1 != 0);
        let mut expected: Vec<_> = raw
            .visible_cells
            .iter()
            .map(|id| bace_types::CellId((raw.id & 0xffff0000) | u32::from(*id)))
            .collect();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(row.visible_cells.as_ref(), expected.as_slice());
    }
    let mut world = bace_world::World::default();
    world.install_geometry(geometry).unwrap();
    world.validate_cell_visibility(&rows).unwrap();
    world.install_cell_visibility(rows).unwrap();
}
