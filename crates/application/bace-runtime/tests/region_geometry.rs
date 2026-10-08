use bace_dat::{CombatManeuverTable, DatArchive, Landblock, RegionLand};
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn source_static_primary_spheres_resolve_four_8602_cells() {
    use bace_runtime::region_activation::{RegionAssetManifest, VerifiedRegionAssets};
    // Pinned Patches v0.9.295 landblock_instance GUIDs. GDLE
    // ObjCell.find_cell_list uses the primary sphere center; EnvCell's visible
    // list supplies neighboring candidates. This is source-derived admission
    // evidence, not a complete parity claim.
    let directory =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let (geometry, visibility) = assets.prepare_geometry_with_visibility(0x8602).unwrap();
    let mut portal = DatArchive::open(&manifest.portal).unwrap();
    for (guid, authored, setup_id, position, expected) in [
        (
            0x7860_200f,
            0x8602_0169,
            0x0200_01b3,
            [158.641, -149.516, -6.063],
            0x8602_0169,
        ),
        (
            0x7860_2052,
            0x8602_0273,
            0x0200_01b3,
            [70.0, -40.0, -0.063],
            0x8602_0273,
        ),
        (
            0x7860_2065,
            0x8602_0331,
            0x0200_01b3,
            [90.0, -60.0, 11.937],
            0x8602_0331,
        ),
        (
            0x7860_20aa,
            0x8602_0134,
            0x0200_026b,
            [119.849, -154.436, -5.995],
            0x8602_0133,
        ),
    ] {
        let setup = bace_dat::CollisionSetup::decode(&portal.read(setup_id).unwrap()).unwrap();
        let shape = bace_runtime::world_admission::prepare_collision_shape(&setup, 1.0).unwrap();
        let visible: Vec<_> = visibility
            .iter()
            .find(|row| row.cell.0 == authored)
            .unwrap()
            .visible_cells
            .iter()
            .map(|cell| cell.0)
            .collect();
        let position = bace_geometry::Vec3::new(position[0], position[1], position[2]);
        assert_eq!(
            geometry.placement_cell(authored, position, &shape, &visible),
            Ok(expected),
            "pinned static GUID {guid:08x}"
        );
        assert!(geometry.cell(expected).is_some());
    }
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_human_cast_action_chains_are_rootless_and_complete() {
    use bace_runtime::world_admission::{MotionChainRequest, prepare_motion_chain};
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = DatArchive::open(path).unwrap();
    let motions = bace_dat::MotionTable::decode(&dat.read(0x09000001).unwrap()).unwrap();
    let components =
        bace_dat::SpellComponents::decode(&dat.read(bace_dat::SpellComponents::RECORD_ID).unwrap())
            .unwrap();
    let ids: std::collections::BTreeSet<_> = motions
        .links
        .values()
        .flat_map(|m| m.values())
        .chain(motions.cycles.values())
        .flat_map(|d| d.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = std::collections::BTreeMap::new();
    for id in ids {
        animations.insert(
            id,
            bace_dat::Animation::decode(&dat.read(id).unwrap()).unwrap(),
        );
    }
    let gestures: std::collections::BTreeSet<_> = components
        .components
        .values()
        .map(|c| c.gesture)
        .filter(|g| g & 0xffff != 0)
        .map(|g| if g == 0x1000012f { 0x13000132 } else { g })
        .collect();
    assert!(!gestures.is_empty());
    for gesture in &gestures {
        let chain = prepare_motion_chain(
            &motions,
            &animations,
            MotionChainRequest {
                style: 0x80000049,
                current_motion: 0x41000003,
                current_speed: 1.0,
                action: *gesture,
                action_speed: 2.0,
                scale: 1.0,
                modifiers: &[],
            },
        )
        .unwrap_or_else(|e| panic!("gesture {gesture:08x}: {e}"));
        assert!(chain.is_rootless(), "gesture {gesture:08x} has root motion");
        let mut playback = bace_motion::MotionPlayback::new(
            chain,
            bace_motion::MotionToken {
                domain: bace_motion::MotionDomain::Casting,
                owner: 1,
                sequence: 1,
            },
        )
        .unwrap();
        let mut events = Vec::with_capacity(4096);
        let mut completed = 0;
        for _ in 0..1800 {
            events.clear();
            playback
                .advance(f64::from(1.0f32 / 30.0), &mut events, 4096)
                .unwrap();
            completed += events
                .iter()
                .filter(|e| matches!(e, bace_motion::MotionExecutionEvent::Completed))
                .count();
        }
        assert_eq!(completed, 1, "gesture {gesture:08x}");
    }
    eprintln!(
        "qualified {} human component gesture chains",
        gestures.len()
    );
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_terrain_and_combat_tables_prepare() {
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
    let mut portal = DatArchive::open(portal).unwrap();
    let mut cells = DatArchive::open(cell).unwrap();
    let land = RegionLand::decode_prefix(&portal.read(0x13000000).unwrap()).unwrap();
    eprintln!(
        "land lattice: {} {} {} heights {} {}",
        land.square_length,
        land.landblock_length,
        land.vertices_per_cell,
        land.heights[0],
        land.heights[255]
    );
    let ids: Vec<_> = cells
        .records()
        .keys()
        .copied()
        .filter(|id| id & 0xffff == 0xffff)
        .step_by(257)
        .take(128)
        .collect();
    let mut prepared = 0;
    for id in ids {
        let block = Landblock::decode(&cells.read(id).unwrap()).unwrap();
        let terrain = bace_runtime::region_geometry::terrain_cells(&block, &land)
            .unwrap_or_else(|e| panic!("{id:08x}: {e}"));
        assert_eq!(terrain.len(), 64);
        assert!(bace_physics::GeometryRegion::prepare(terrain).is_ok());
        prepared += 1;
    }
    let ids: Vec<_> = portal
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 0x30)
        .collect();
    let mut maneuvers = 0;
    for id in &ids {
        let table = CombatManeuverTable::decode(&portal.read(*id).unwrap()).unwrap();
        assert_eq!(table.id, *id);
        maneuvers += table.maneuvers.len();
    }
    eprintln!(
        "prepared terrain blocks={prepared}, CMT records={}, maneuvers={maneuvers}",
        ids.len()
    );
    assert!(prepared > 20);
    assert!(!ids.is_empty());
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_human_attack_timelines() {
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = directory.join("client_portal.dat");
    assert_eq!(
        bace_dat::fingerprint(&path).unwrap(),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
    );
    let mut dat = DatArchive::open(path).unwrap();
    let cmt = CombatManeuverTable::decode(&dat.read(0x30000000).unwrap()).unwrap();
    let motions = bace_dat::MotionTable::decode(&dat.read(0x09000001).unwrap()).unwrap();
    let ids: std::collections::BTreeSet<_> = motions
        .links
        .values()
        .flat_map(|m| m.values())
        .flat_map(|d| d.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = std::collections::BTreeMap::new();
    for id in ids {
        animations.insert(
            id,
            bace_dat::Animation::decode(&dat.read(id).unwrap()).unwrap(),
        );
    }
    let prepared =
        bace_runtime::world_admission::prepare_combat_maneuvers(&cmt, &motions, &animations)
            .unwrap();
    let hooks: usize = prepared.iter().map(|m| m.hooks.len()).sum();
    assert_eq!(prepared.len(), cmt.maneuvers.len());
    assert!(hooks > 0);
    eprintln!("human maneuvers={} attack hooks={hooks}", prepared.len());
}
fn load_collision_assets(
    dat: &mut DatArchive,
    ids: impl Iterator<Item = u32>,
) -> (
    std::collections::BTreeMap<u32, bace_dat::GraphicsObject>,
    std::collections::BTreeMap<u32, bace_dat::CollisionSetup>,
) {
    let mut pending: Vec<_> = ids.collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut graphics = std::collections::BTreeMap::new();
    let mut setups = std::collections::BTreeMap::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        assert!(seen.len() <= 4096);
        match id >> 24 {
            1 => {
                graphics.insert(
                    id,
                    bace_dat::GraphicsObject::decode(&dat.read(id).unwrap()).unwrap(),
                );
            }
            2 => {
                let setup = bace_dat::CollisionSetup::decode(&dat.read(id).unwrap()).unwrap();
                pending.extend(setup.parts.iter().copied());
                setups.insert(id, setup);
            }
            _ => panic!("unexpected static asset {id:08x}"),
        }
    }
    (graphics, setups)
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_holtburg_complete_outdoor_static_geometry() {
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
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
    let mut portal = DatArchive::open(portal).unwrap();
    let mut dat = DatArchive::open(cell).unwrap();
    let block = Landblock::decode(&dat.read(0xa260ffff).unwrap()).unwrap();
    let info = bace_dat::LandblockInfo::decode(&dat.read(0xa260fffe).unwrap()).unwrap();
    let land = RegionLand::decode_prefix(&portal.read(0x13000000).unwrap()).unwrap();
    let (graphics, setups) = load_collision_assets(
        &mut portal,
        info.objects
            .iter()
            .map(|o| o.id)
            .chain(info.buildings.iter().map(|b| b.model)),
    );
    let cells = bace_runtime::region_geometry::outdoor_block(
        &block,
        Some(&info),
        &land,
        &graphics,
        &setups,
    )
    .unwrap();
    let faces: usize = cells.iter().map(|c| c.faces.len()).sum();
    let solids: usize = cells.iter().map(|c| c.solids.len()).sum();
    assert!(faces > 128);
    assert!(solids > 0);
    assert_eq!(cells.len(), 64);
    bace_physics::GeometryRegion::prepare(cells).unwrap();
    eprintln!(
        "Holtburg static objects={} buildings={} meshes={} setups={} faces={faces} BSPinstances={solids}",
        info.objects.len(),
        info.buildings.len(),
        graphics.len(),
        setups.len()
    );
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn supplied_holtburg_indoor_outdoor_region_and_authoritative_body() {
    use bace_geometry::Vec3;
    use bace_physics::{Body, GeometrySpawn};
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
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
    let mut portal = DatArchive::open(portal).unwrap();
    let mut dat = DatArchive::open(cell).unwrap();
    let block = Landblock::decode(&dat.read(0xa260ffff).unwrap()).unwrap();
    let info = bace_dat::LandblockInfo::decode(&dat.read(0xa260fffe).unwrap()).unwrap();
    let land = RegionLand::decode_prefix(&portal.read(0x13000000).unwrap()).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id & 0xffff0000 == 0xa2600000 && (0x100..=0xfffd).contains(&(id & 0xffff)))
        .collect();
    let mut indoors = Vec::new();
    let mut environments = std::collections::BTreeMap::new();
    for id in ids {
        let c = bace_dat::EnvCell::decode(&dat.read(id).unwrap()).unwrap();
        if let std::collections::btree_map::Entry::Vacant(entry) =
            environments.entry(c.environment_id)
        {
            entry.insert(
                bace_dat::Environment::decode(&portal.read(c.environment_id).unwrap()).unwrap(),
            );
        }
        indoors.push(c);
    }
    let (graphics, setups) = load_collision_assets(
        &mut portal,
        info.objects
            .iter()
            .map(|o| o.id)
            .chain(info.buildings.iter().map(|b| b.model))
            .chain(
                indoors
                    .iter()
                    .flat_map(|c| c.static_objects.iter().map(|o| o.id)),
            ),
    );
    let mut outdoors = bace_runtime::region_geometry::outdoor_block(
        &block,
        Some(&info),
        &land,
        &graphics,
        &setups,
    )
    .unwrap();
    let mut geometry = Vec::new();
    for c in &indoors {
        geometry.push(
            bace_runtime::region_geometry::indoor_cell_with_assets(
                c,
                &environments[&c.environment_id],
                &graphics,
                &setups,
                &mut outdoors,
            )
            .unwrap_or_else(|e| panic!("{:08x}: {e}", c.id)),
        );
    }
    let candidates: Vec<_> = outdoors
        .iter()
        .flat_map(|c| {
            c.faces.iter().take(2).map(move |face| {
                let v = face.polygon.vertices();
                let p = (v[0] + v[1] + v[2]) * (1.0 / 3.0);
                (c.id, p)
            })
        })
        .collect();
    geometry.extend(outdoors);
    let region = bace_physics::GeometryRegion::prepare(geometry).unwrap();
    let chargen = bace_dat::CharGen::decode(&portal.read(0x0e000002).unwrap()).unwrap();
    let gender = &chargen.heritage_groups[&1].genders[&1];
    let setup = bace_dat::CollisionSetup::decode(&portal.read(gender.setup).unwrap()).unwrap();
    let shape = bace_runtime::world_admission::prepare_collision_shape(&setup, 1.0).unwrap();
    let motions =
        bace_dat::MotionTable::decode(&portal.read(gender.motion_table).unwrap()).unwrap();
    let ids: std::collections::BTreeSet<_> = motions
        .cycles
        .values()
        .flat_map(|c| c.animations.iter().map(|a| a.animation_id))
        .collect();
    let mut animations = std::collections::BTreeMap::new();
    for id in ids {
        animations.insert(
            id,
            bace_dat::Animation::decode(&portal.read(id).unwrap()).unwrap(),
        );
    }
    let animated = bace_runtime::world_admission::prepare_animated_locomotion(
        &motions,
        motions.default_style,
        &animations,
        1.0,
    )
    .unwrap();
    let locomotion = &animated.profile;
    let mut admitted = None;
    for (cell, p) in candidates {
        if let Ok(body) = Body::spawn_geometry(
            &region,
            GeometrySpawn {
                cell,
                position: p + Vec3::new(0.0, 0.0, 0.01),
                shape: shape.clone(),
                capabilities: bace_motion::Capabilities {
                    speed: 4.0,
                    jump_impulse: 1.0,
                },
                heading: 0.0,
                maximum_turn_rate: locomotion.turn.omega.z.abs(),
            },
        ) {
            admitted = Some((cell, body));
            break;
        }
    }
    let (mut cell, mut body) = admitted.expect("at least one actual terrain placement");
    let start = body.accepted().position();
    let rate = bace_character::run_rate(bace_character::RunInput {
        skill: 300,
        burden: 0.0,
        scale: 1.0,
        exhausted: false,
    })
    .unwrap();
    body.submit_animated_locomotion(
        0,
        1,
        animated.clone(),
        bace_motion::LocomotionControls {
            forward: 1.0,
            run: true,
            ..Default::default()
        },
        rate,
    )
    .unwrap();
    for _ in 0..90 {
        cell = body.step_geometry(&region, cell, 7, &[]).unwrap();
    }
    eprintln!(
        "motion profile={locomotion:?}; start={start:?}; end={:?}",
        body.accepted()
    );
    assert!((body.accepted().position() - start).length_squared() > 1.0);
    assert!(body.accepted().position().is_finite());
    eprintln!(
        "Holtburg connected indoor cells={}, 90 actual-geometry ticks: {:?}->{:?} cell={cell:08x}",
        indoors.len(),
        start,
        body.accepted().position()
    );
}
