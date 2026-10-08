use bace_dat::{BspTree, BspTreeKind, DatTableLimits, EnvCell};
#[test]
fn bsp_depth_nodes_counts_nonfinite_tags_and_trailing_bytes_are_bounded() {
    let mut leaf = 0x4c454146u32.to_le_bytes().to_vec();
    leaf.extend_from_slice(&(-7i32).to_le_bytes());
    assert_eq!(
        BspTree::decode(&leaf, BspTreeKind::Cell).unwrap().nodes[0].leaf_index,
        Some(-7)
    );
    let mut deep = Vec::new();
    for _ in 0..128 {
        deep.extend_from_slice(&0x42506e6eu32.to_le_bytes());
        deep.extend_from_slice(&[0; 16]);
    }
    deep.extend_from_slice(&leaf);
    assert!(BspTree::decode(&deep, BspTreeKind::Cell).is_err());
    assert!(
        BspTree::decode_with_limits(
            &leaf,
            BspTreeKind::Cell,
            DatTableLimits {
                max_entries: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut bad = leaf.clone();
    bad[0] = 0;
    assert!(BspTree::decode(&bad, BspTreeKind::Cell).is_err());
    let mut trailing = leaf.clone();
    trailing.push(0);
    assert!(BspTree::decode(&trailing, BspTreeKind::Cell).is_err());
    let mut physics = leaf;
    physics.extend_from_slice(&1u32.to_le_bytes());
    physics.extend_from_slice(&[0; 16]);
    physics.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(BspTree::decode(&physics, BspTreeKind::Physics).is_err());
    physics[28..32].copy_from_slice(&0u32.to_le_bytes());
    physics[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(BspTree::decode(&physics, BspTreeKind::Physics).is_err());
}
#[test]
#[ignore = "requires approved user-supplied cell DAT; set BACE_DAT_DIRECTORY"]
fn supplied_cell_archive_envcell_records() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("set BACE_DAT_DIRECTORY"),
    );
    let path = directory.join("client_cell_1.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e"
    );
    let mut dat = bace_dat::DatArchive::open(path).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| (0x100..=0xfffd).contains(&(id & 0xffff)))
        .step_by(997)
        .take(256)
        .collect();
    assert!(ids.len() > 20);
    let mut static_objects = 0;
    let mut portals = 0;
    for id in &ids {
        let bytes = dat.read(*id).unwrap();
        let cell = EnvCell::decode(&bytes).unwrap_or_else(|e| panic!("cell {id:08x}: {e}"));
        assert_eq!(cell.id, *id);
        static_objects += cell.static_objects.len();
        portals += cell.portals.len();
    }
    eprintln!(
        "decoded {} sampled actual EnvCells, {portals} portals, {static_objects} static objects",
        ids.len()
    );
}
#[test]
fn motion_and_animation_counts_duplicates_nonfinite_and_unknown_hooks_fail() {
    let mut m = 0x09000001u32.to_le_bytes().to_vec();
    m.extend_from_slice(&[0; 20]);
    assert!(bace_dat::MotionTable::decode(&m).is_ok());
    m[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(bace_dat::MotionTable::decode(&m).is_err());
    let mut a = 0x03000001u32.to_le_bytes().to_vec();
    a.extend_from_slice(&[0; 12]);
    assert!(bace_dat::Animation::decode(&a).is_ok());
    a[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(bace_dat::Animation::decode(&a).is_err());
    a[12..16].copy_from_slice(&1u32.to_le_bytes());
    a.extend_from_slice(&1u32.to_le_bytes());
    a.extend_from_slice(&u32::MAX.to_le_bytes());
    a.extend_from_slice(&0u32.to_le_bytes());
    assert!(bace_dat::Animation::decode(&a).is_err());
}
#[test]
#[ignore = "requires approved user-supplied portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_portal_motion_and_animation_records() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("set BACE_DAT_DIRECTORY"),
    );
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = bace_dat::DatArchive::open(path).unwrap();
    let motions: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 9)
        .collect();
    let animations: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 3)
        .step_by(29)
        .take(512)
        .collect();
    assert!(motions.len() > 10);
    assert!(animations.len() > 10);
    for id in &motions {
        let bytes = dat.read(*id).unwrap();
        let m = bace_dat::MotionTable::decode(&bytes)
            .unwrap_or_else(|e| panic!("motion {id:08x}: {e}"));
        assert_eq!(m.id, *id);
    }
    let mut attacks = 0;
    for id in &animations {
        let bytes = dat.read(*id).unwrap();
        let a = bace_dat::Animation::decode(&bytes)
            .unwrap_or_else(|e| panic!("animation {id:08x}: {e}"));
        assert_eq!(a.id, *id);
        attacks += a.attack_hooks().count();
    }
    eprintln!(
        "decoded {} actual motion tables, {} sampled animations, {attacks} attack hooks",
        motions.len(),
        animations.len()
    );
}
#[test]
#[ignore = "requires approved user-supplied portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_portal_environment_geometry_records() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("set BACE_DAT_DIRECTORY"),
    );
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = bace_dat::DatArchive::open(path).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 0x0d)
        .collect();
    assert!(ids.len() > 10);
    let mut cells = 0;
    let mut nodes = 0;
    for id in &ids {
        let bytes = dat.read(*id).unwrap();
        let e = bace_dat::Environment::decode(&bytes)
            .unwrap_or_else(|e| panic!("environment {id:08x}: {e}"));
        assert_eq!(e.id, *id);
        cells += e.cells.len();
        for cell in e.cells.values() {
            nodes += cell.physics_bsp.nodes.len();
        }
    }
    eprintln!(
        "decoded {} actual environment models, {cells} cells, {nodes} physics BSP nodes",
        ids.len()
    );
}
#[test]
#[ignore = "requires approved user-supplied portal DAT; set BACE_DAT_DIRECTORY"]
fn supplied_portal_collision_setup_records() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("set BACE_DAT_DIRECTORY"),
    );
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = bace_dat::DatArchive::open(path).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 2)
        .collect();
    assert!(ids.len() > 10);
    let mut spheres = 0;
    let mut cylinders = 0;
    for id in &ids {
        let raw = dat.read(*id).unwrap();
        let setup = bace_dat::CollisionSetup::decode(&raw)
            .unwrap_or_else(|e| panic!("setup {id:08x}: {e}"));
        assert_eq!(setup.id, *id);
        spheres += setup.spheres.len();
        cylinders += setup.cylinders.len();
    }
    eprintln!(
        "decoded {} actual collision setups, {spheres} spheres, {cylinders} cylinders",
        ids.len()
    );
}
