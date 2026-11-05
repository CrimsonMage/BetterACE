use bace_dat::{CombatManeuverTable, DatArchive, Landblock, RegionLand};

#[test]
#[ignore = "requires approved DATs and accepted full native pack"]
fn supplied_e74e_placed_npcs_admit_without_unneeded_generator_targets() {
    use bace_runtime::{
        generator_preparation::CreatureAdmissionPolicy,
        region_activation::{RegionActivationRequest, RegionAssetManifest, VerifiedRegionAssets},
    };
    use bace_storage_codec::{PackLimits, load_manifest};
    use std::{path::PathBuf, sync::Arc};

    let directory = PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").unwrap());
    let manifest = load_manifest(&path, PackLimits::default()).unwrap();
    let hash = manifest.content_hash(PackLimits::default()).unwrap();
    let generation = Arc::new(
        manifest
            .open(path.parent().unwrap(), PackLimits::default())
            .unwrap(),
    );
    let mut assets = VerifiedRegionAssets::open(&RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
        store: None,
    })
    .unwrap();
    let region = assets
        .prepare(&RegionActivationRequest {
            token: 1,
            activation_epoch: 1,
            landblock: 0xe74e,
            generation,
            creature_policy: Some(CreatureAdmissionPolicy {
                corpse_template: 1,
                think_interval: 30,
                corpse_decay_ticks: 900,
            }),
            treasure_assets: None,
            content_hash: hash,
            aetheria_drop_rate: 0.,
        })
        .unwrap();
    let invalid: Vec<_> = region
        .creatures
        .iter()
        .filter_map(|(id, result)| result.as_ref().err().map(|error| (*id, error.clone())))
        .collect();
    assert_eq!(
        invalid.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [23623]
    );
    let controller = region.creatures.get(&71241).unwrap().as_ref().unwrap();
    assert_eq!(controller.blueprint.combat.maximum_health, 0);
    assert_eq!(controller.blueprint.capabilities.speed, 0.0);
    assert!(controller.combat_assets.is_none());
    let controller_source = region.content.templates.get(&71241).unwrap();
    let mut controller_loadout = bace_runtime::generator_equipment::prepare_creature_loadout(
        bace_types::EntityId(0x8000_0001),
        controller_source,
        &[],
        controller,
        &Default::default(),
    )
    .unwrap();
    assets
        .prepare_npc_loadout_motions(
            controller_source,
            &mut controller_loadout,
            &[],
            &region.generation,
        )
        .unwrap();
    assert!(controller_loadout.physical_motions.is_empty());
    let sign = region.creatures.get(&25722).unwrap().as_ref().unwrap();
    assert!(sign.physical.is_some());
    assert!(sign.combat_assets.is_none());
    let mut sign_loadout = bace_simulation::PreparedNpcLoadout {
        combat_assets: None,
        death_motions: Vec::new(),
        physical_motions: Vec::new(),
        locomotion_styles: Vec::new(),
        items: Vec::new(),
        containers: Vec::new(),
        profile: sign.physical.as_ref().unwrap().clone(),
        death_items: Vec::new(),
        death_parents: Vec::new(),
        death_ids: Vec::new(),
        enchantments: Vec::new(),
    };
    assets
        .prepare_npc_loadout_motions(
            region.content.templates.get(&25722).unwrap(),
            &mut sign_loadout,
            &[],
            &region.generation,
        )
        .unwrap();
    assert!(sign_loadout.physical_motions.is_empty());
    assert!(sign_loadout.death_motions.is_empty());
    assert_eq!(sign_loadout.locomotion_styles.len(), 1);
    for (id, _) in &invalid {
        assert!(
            !region
                .content
                .instances
                .iter()
                .any(|instance| instance.template.weenie_id == *id)
        );
        assert!(
            !region
                .content
                .encounters
                .iter()
                .any(|encounter| encounter.template.weenie_id == *id)
        );
    }
    let admission = region
        .prepare_generator_admission(&std::collections::BTreeMap::new(), Default::default())
        .unwrap();
    let invalid_profiles: Vec<_> = admission
        .admission
        .creatures
        .iter()
        .filter_map(|(id, creature)| {
            creature.physical.as_ref().and_then(|profile| {
                bace_combat::physical::validate_physical_profile(profile)
                    .err()
                    .map(|error| (*id, error))
            })
        })
        .collect();
    assert!(
        invalid_profiles.is_empty(),
        "invalid profiles: {invalid_profiles:?}"
    );
    for root in &admission.admission.roots {
        if let Some(creature) = &root.creature
            && creature.physical.is_some()
            && creature.physical_motions.is_empty()
        {
            let profile = root
                .loadout
                .as_ref()
                .map(|loadout| &loadout.profile)
                .or(creature.physical.as_ref())
                .unwrap();
            assert!(!profile.attackable);
            assert!(profile.maneuvers.is_empty());
            assert!(creature.combat_assets.is_none());
        }
    }
    let mut header_shops = 0;
    for instance in region.content.instances.iter().filter(|instance| {
        instance.template.weenie_type == 12 && instance.source.obj_cell_id & 0xffff == 0
    }) {
        let physical = region
            .physical
            .get(&instance.template.weenie_id)
            .unwrap()
            .as_ref()
            .unwrap();
        let center = physical.shape.spheres().first().unwrap().center;
        let source = bace_geometry::Vec3::new(
            instance.source.origin_x,
            instance.source.origin_y,
            instance.source.origin_z,
        );
        let (cell, _) = bace_runtime::region_geometry::normalize_outdoor(
            instance.source.obj_cell_id | 0xffff,
            source + center,
        )
        .unwrap();
        assert!(region.geometry.cell(cell).is_some());
        if let Some(root) = admission
            .admission
            .roots
            .iter()
            .find(|root| root.entity.0 == instance.source.guid)
        {
            assert_eq!(cell, root.location.cell);
        }
        header_shops += 1;
    }
    assert!(header_shops > 0);
    let link_cell = admission
        .admission
        .definitions
        .iter()
        .find(|definition| definition.identity.entity.0 == 0x7e74e04d)
        .unwrap()
        .profiles
        .iter()
        .find(|profile| profile.id == 0x7e74e04b)
        .unwrap()
        .position
        .cell
        .unwrap();
    assert_ne!(link_cell, 0xe74e0000);
    assert!(region.geometry.cell(link_cell).is_some());
    let mut world = bace_world::World::default();
    world.install_geometry(region.geometry.clone()).unwrap();
    for root in &admission.admission.roots {
        let q = root.location.rotation;
        assert!(
            q[0].abs() <= 0.0002
                && q[1].abs() <= 0.0002
                && (0.999..=1.001).contains(&q.iter().map(|v| v * v).sum::<f32>()),
            "root {:#x} has invalid authored rotation {q:?}",
            root.entity.0
        );
        let spawn = bace_physics::GeometrySpawn {
            cell: root.location.cell,
            position: bace_geometry::Vec3::new(
                root.location.origin[0],
                root.location.origin[1],
                root.location.origin[2],
            ),
            shape: root.shape.clone(),
            capabilities: root.creature.as_ref().map_or(
                bace_motion::Capabilities {
                    speed: 0.0,
                    jump_impulse: 0.0,
                },
                |c| c.blueprint.capabilities,
            ),
            heading: 2.0 * q[2].atan2(q[3]),
            maximum_turn_rate: root
                .creature
                .as_ref()
                .map_or(0.0, |c| c.geometry.maximum_turn_rate),
        };
        let mut body = if root.authored_static {
            bace_physics::Body::spawn_authored_static_geometry(&region.geometry, spawn)
        } else if root.creature.is_none() {
            bace_physics::Body::spawn_authored_stationary_geometry(&region.geometry, spawn)
        } else {
            bace_physics::Body::spawn_geometry(&region.geometry, spawn)
        }
        .unwrap_or_else(|error| panic!("root {:#x} geometry: {error:?}", root.entity.0));
        if let Some(creature) = &root.creature {
            body.adopt_animated_style(creature.geometry.locomotion.clone(), true)
                .unwrap_or_else(|error| {
                    panic!("root {:#x} animated style: {error:?}", root.entity.0)
                });
            body.refresh_locomotion(
                &creature.geometry.locomotion.profile,
                creature.geometry.run_rate,
            )
            .unwrap_or_else(|error| panic!("root {:#x} locomotion: {error:?}", root.entity.0));
        }
        world
            .insert(bace_entity::Actor {
                id: root.entity,
                cell: bace_types::CellId(root.location.cell),
                body,
            })
            .unwrap_or_else(|error| panic!("root {:#x} world: {error:?}", root.entity.0));
    }
    assert!(
        admission
            .held
            .iter()
            .all(|(_, error)| !error.contains("NPC physical profile"))
    );
    let unexpected: Vec<_> = admission
        .held
        .iter()
        .filter(|(_, error)| error != "creature equipment GUIDs/loadout not prepared")
        .collect();
    for (id, _) in &unexpected {
        if let Some(instance) = region
            .content
            .instances
            .iter()
            .find(|i| i.source.guid == *id)
        {
            let p = bace_geometry::Vec3::new(
                instance.source.origin_x,
                instance.source.origin_y,
                instance.source.origin_z,
            );
            eprintln!(
                "held source {id:#x}: {} type {} cell {:#x} origin ({}, {}, {}) ground {:?}",
                instance.template.class_name,
                instance.template.weenie_type,
                instance.source.obj_cell_id,
                instance.source.origin_x,
                instance.source.origin_y,
                instance.source.origin_z,
                region.geometry.ground_at(instance.source.obj_cell_id, p)
            );
        }
    }
    assert!(
        unexpected.is_empty(),
        "held geometry or capacity roots: {unexpected:?}"
    );
    assert!(
        admission
            .held
            .iter()
            .all(|(id, _)| ![23623, 71241].contains(id))
    );
    assert!(
        !admission
            .admission
            .creatures
            .iter()
            .any(|(id, _)| *id == 23623)
    );
    assert!(
        admission
            .admission
            .creatures
            .iter()
            .any(|(id, _)| *id == 25722)
    );
    assert!(
        admission
            .admission
            .creatures
            .iter()
            .any(|(id, _)| *id == 71241)
    );
    let mut kernel = bace_simulation::Kernel::new(bace_world::World::default(), 8192).unwrap();
    let random = Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap());
    kernel.configure_native_loot(random.clone(), 1).unwrap();
    kernel.configure_generators(random, 1, true).unwrap();
    kernel.admit_generator_region(admission.admission).unwrap();
    let region8602 = assets
        .prepare(&RegionActivationRequest {
            token: 2,
            activation_epoch: 1,
            landblock: 0x8602,
            generation: region.generation.clone(),
            creature_policy: Some(CreatureAdmissionPolicy {
                corpse_template: 1,
                think_interval: 30,
                corpse_decay_ticks: 900,
            }),
            treasure_assets: None,
            content_hash: hash,
            aetheria_drop_rate: 0.,
        })
        .unwrap();
    let without_equipment_profile: Vec<_> = region8602
        .content
        .instances
        .iter()
        .filter(|instance| !instance.source.is_link_child)
        .filter_map(|instance| {
            let creature = region8602
                .creatures
                .get(&instance.template.weenie_id)?
                .as_ref()
                .ok()?;
            (creature.requires_equipment && creature.physical.is_none()).then_some((
                instance.template.weenie_id,
                instance.template.class_name.clone(),
            ))
        })
        .collect();
    assert!(
        without_equipment_profile.is_empty(),
        "missing equipment profile: {without_equipment_profile:?}"
    );
}
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
        store: None,
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
