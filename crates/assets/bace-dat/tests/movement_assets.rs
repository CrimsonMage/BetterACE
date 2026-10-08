#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_contracts_static_collision_and_landblock_info() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let portal = directory.join("client_portal.dat");
    let cell = directory.join("client_cell_1.dat");
    assert_eq!(
        bace_dat::fingerprint(&portal).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    assert_eq!(
        bace_dat::fingerprint(&cell).unwrap(),
        "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e"
    );
    let mut portal = bace_dat::DatArchive::open(portal).unwrap();
    let mut cells = bace_dat::DatArchive::open(cell).unwrap();
    let contracts = bace_dat::ContractTable::decode(&portal.read(0x0e00001d).unwrap()).unwrap();
    assert!(contracts.contracts.len() > 100);
    assert!(
        contracts
            .contracts
            .iter()
            .all(|(id, c)| *id == c.id && *id > 0)
    );
    let infos: Vec<_> = cells
        .records()
        .keys()
        .copied()
        .filter(|id| id & 0xffff == 0xfffe)
        .step_by(127)
        .take(256)
        .collect();
    let mut objects = 0;
    let mut buildings = 0;
    for id in &infos {
        let info = bace_dat::LandblockInfo::decode(&cells.read(*id).unwrap())
            .unwrap_or_else(|e| panic!("{id:08x}: {e}"));
        assert_eq!(info.id, *id);
        objects += info.objects.len();
        buildings += info.buildings.len();
    }
    let ids: Vec<_> = portal
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 1)
        .step_by(31)
        .take(512)
        .collect();
    let mut polygons = 0;
    let mut nodes = 0;
    for id in &ids {
        let gfx = bace_dat::GraphicsObject::decode(&portal.read(*id).unwrap())
            .unwrap_or_else(|e| panic!("{id:08x}: {e}"));
        polygons += gfx.physics_polygons.len();
        nodes += gfx.physics_bsp.as_ref().map_or(0, |b| b.nodes.len());
    }
    eprintln!(
        "contracts={}, landblock_infos={} objects={objects} buildings={buildings}, graphics={} physics_polygons={polygons} nodes={nodes}",
        contracts.contracts.len(),
        infos.len(),
        ids.len()
    );
    assert!(objects > 0);
    assert!(polygons > 0);
}
