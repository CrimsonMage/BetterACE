//! Authored avatar content with actual approved DAT assets. This tests server-side
//! preparation/admission, not a stock-client qualification claim.
mod equipment_transaction;
use bace_content::{Attribute, BodyPart, Property, SecondaryAttribute, Skill, WeenieV1};
use bace_runtime::{
    player_assets::*,
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
};
use bace_types::{AccountId, EntityId};
#[test]
#[ignore = "requires approved real DATs; set BACE_DAT_DIRECTORY"]
fn real_avatar_dat_and_saved_state_admit_without_synthetic_scene() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut archive = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let chargen =
        bace_dat::CharGen::decode(&archive.read(bace_dat::CharGen::RECORD_ID).unwrap()).unwrap();
    let gender = &chargen.heritage_groups[&1].genders[&1];
    let mut source = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "authored-server-test-avatar".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.data_ids = vec![
        Property {
            id: 1,
            value: gender.setup,
        },
        Property {
            id: 2,
            value: gender.motion_table,
        },
        Property {
            id: 4,
            value: 0x30000000,
        },
    ];
    source.properties.ints = vec![
        Property { id: 6, value: 102 },
        Property { id: 7, value: 7 },
        Property { id: 24, value: 52 },
        Property { id: 25, value: 1 },
        Property { id: 188, value: 1 },
    ];
    source.properties.int64s = vec![Property { id: 1, value: 0 }, Property { id: 2, value: 0 }];
    source.properties.attributes = (1..=6)
        .map(|id| Property {
            id,
            value: Attribute {
                init_level: 100,
                level_from_cp: 0,
                cp_spent: 0,
            },
        })
        .collect();
    source.properties.secondary_attributes = [1, 3, 5]
        .into_iter()
        .map(|id| Property {
            id,
            value: SecondaryAttribute {
                init_level: 0,
                level_from_cp: 0,
                cp_spent: 0,
                current_level: 10,
            },
        })
        .collect();
    source.properties.body_parts = vec![Property {
        id: 0,
        value: BodyPart {
            d_type: 4,
            d_val: 3,
            d_var: 0.2,
            llf: 1.,
            llb: 1.,
            lrf: 1.,
            lrb: 1.,
            mlf: 1.,
            mlb: 1.,
            mrf: 1.,
            mrb: 1.,
            hlf: 1.,
            hlb: 1.,
            hrf: 1.,
            hrb: 1.,
            ..Default::default()
        },
    }];
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    assert!(
        !assets.prepare_material_names().unwrap().is_empty(),
        "verified DAT material table is required for crafting output"
    );
    let idle = bace_motion::SourceMotionState {
        style: 0x8000003d,
        substate: 0x41000003,
        speed: 1.0,
    };
    let combat_recall = assets
        .prepare_recall_motion(
            &source,
            bace_motion::SourceMotionState {
                style: 0x8000003e,
                ..idle
            },
            bace_interactions::RecallKind::Marketplace,
        )
        .unwrap();
    let style = combat_recall
        .style
        .as_ref()
        .expect("combat recall needs authored stance");
    assert_eq!(style.motion, 0x8000003d);
    assert_eq!(
        style.source_transition().unwrap().after,
        combat_recall.chain.source_transition().unwrap().before
    );
    assert_eq!(combat_recall.delay_ticks, 420);
    let clap = assets.prepare_crafting_motion(&source, idle).unwrap();
    assert_eq!(clap.motion, 0x1300007e);
    assert_eq!(clap.source_transition().unwrap().before, idle);
    assert!(clap.nominal_duration_seconds() > 0.0);
    assert!(!clap.clips().is_empty());
    assert!(
        assets
            .prepare_crafting_motion(
                &source,
                bace_motion::SourceMotionState {
                    style: 0x8000003e,
                    ..idle
                }
            )
            .is_err()
    );
    for kind in [
        bace_interactions::RecallKind::Lifestone,
        bace_interactions::RecallKind::House,
        bace_interactions::RecallKind::Marketplace,
        bace_interactions::RecallKind::AllegianceHometown,
        bace_interactions::RecallKind::PkArena,
    ] {
        let recall = assets.prepare_recall_motion(&source, idle, kind).unwrap();
        assert_eq!(recall.kind, kind);
        assert_eq!(recall.chain.motion, kind.motion());
        assert_eq!(recall.chain.source_transition().unwrap().before, idle);
        assert!(recall.source_animation_seconds > 0.);
        assert!(!recall.chain.clips().is_empty());
        if kind == bace_interactions::RecallKind::Marketplace {
            assert_eq!(recall.delay_ticks, 420);
            assert!(recall.source_animation_seconds > 14.);
        }
    }
    let dat = assets.prepare_avatar_dat(&source).unwrap();
    source.properties.skills = dat
        .character
        .skill_table()
        .skills
        .keys()
        .map(|id| Property {
            id: *id as i32,
            value: Skill {
                sac: 2,
                ..Default::default()
            },
        })
        .collect();
    let geometry = assets.prepare_geometry(0xa260).unwrap();
    let (cell, p) = (0..8)
        .flat_map(|x| (0..8).map(move |y| (x, y)))
        .find_map(|(x, y)| {
            let (cell, mut p) =
                bace_runtime::region_geometry::encounter_position(&geometry, 0xa260, x, y).ok()?;
            // An authored spawn above the real ground avoids assuming that a
            // sphere rooted exactly on a sloped plane is a valid placement.
            p.z += 1.0;
            geometry
                .validate_placement(cell, p, &dat.shape, &[], 0x50000001)
                .ok()?;
            Some((cell, p))
        })
        .expect("outdoor sample with valid full avatar collision placement");
    source.properties.positions = vec![Property {
        id: 1,
        value: bace_content::Position {
            obj_cell_id: cell,
            position_x: p.x,
            position_y: p.y,
            position_z: p.z,
            rotation_w: 1.,
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: 0.,
        },
    }];
    for (id, value) in [(125, 45), (281, 1)] {
        if let Some(property) = source.properties.ints.iter_mut().find(|p| p.id == id) {
            property.value = value;
        } else {
            source
                .properties
                .ints
                .push(bace_content::Property { id, value });
        }
    }
    source.properties.ints.sort_by_key(|p| p.id);
    let player = bace_storage_codec::PlayerSaveV6::migrate_v2(
        bace_storage_codec::PlayerSaveV2::migrate_v1(bace_storage_codec::PlayerSaveV1 {
            entity: bace_storage_codec::EntitySaveV1 {
                object_id: 0x50000001,
                template_revision: 1,
                mutation_revision: 1,
                state: source,
            },
            account_id: 1,
            name: "Alice".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    let binding = bace_gameplay_api::CharacterBinding {
        session: bace_gameplay_api::SessionId(1),
        account: AccountId(1),
        actor: EntityId(0x50000001),
    };
    let loaded = bace_runtime::game_login::LoadedPlayer {
        is_plussed: false,
        key: bace_session::SessionKey {
            id: 1,
            generation: 1,
        },
        binding,
        lease: bace_persistence::CharacterLease {
            character_id: binding.actor.0,
            epoch: 1,
            state: bace_persistence::OwnershipState::Loading,
        },
        persisted_version: 1,
        player,
        cached_experience: 0,
        inventory: vec![],
    };
    let policy = || PlayerColdPolicy {
        account_created_unix: None,
        staff: bace_gameplay_api::staff::StaffRegistration {
            binding,
            privileges: Default::default(),
        },
        portal_access: bace_interactions::PortalAccess {
            level: 1,
            pk_status: 2,
            pk_recent: false,
            olthoi: false,
            vitae: false,
            account_15_days: true,
            entitlement: 0,
            quest_allowed: true,
            teleporting: false,
            recently_teleported: false,
            ignore_restrictions: false,
            enforce_maximum_level: true,
        },
        components_required: true,
        safe_components: false,
    };
    let mut prepared = prepare_loaded_avatar(
        &loaded,
        PlayerColdAssets {
            avatar: &dat,
            geometry: &geometry,
            spell_rows: &Default::default(),
            component_templates: &[],
            projectile_shapes: &Default::default(),
        },
        policy(),
        0,
    )
    .unwrap();
    let rebuilt = prepare_equipment_physical(EquipmentPhysicalInput {
        actor: binding.actor,
        before_revision: prepared.state.character.progression.revision(),
        source: &loaded.player.player.entity.state,
        state: &prepared.state,
        items: &[],
        dat: &dat,
        projectile_shapes: &Default::default(),
    })
    .unwrap();
    assert_eq!(
        rebuilt.source.profile.equipment,
        prepared.physical.equipment
    );
    assert_eq!(rebuilt.source.profile.style, prepared.physical.style);
    assert_eq!(rebuilt.skills, prepared.skills);
    assert!(!rebuilt.motions.is_empty());
    let gear_id = EntityId(0x80000077);
    let mut gear = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 700,
        class_name: "authored-equipment-rebuild".into(),
        weenie_type: 2,
        last_modified: None,
        properties: Default::default(),
    };
    gear.properties.ints = vec![
        Property { id: 1, value: 2 },
        Property {
            id: 9,
            value: 0x200,
        },
        Property { id: 28, value: 200 },
        Property { id: 379, value: 40 },
    ];
    let gear = bace_storage_codec::ItemSaveV4::migrate_v2(bace_storage_codec::ItemSaveV2 {
        entity: bace_storage_codec::EntitySaveV1 {
            object_id: gear_id.0,
            template_revision: 1,
            mutation_revision: 2,
            state: gear,
        },
        placement: bace_storage_codec::ItemPlacementV2::Contained {
            container: binding.actor.0,
            slot: 0,
            pack_slot: false,
            equipped: 0x200,
        },
    })
    .unwrap();
    prepared
        .state
        .item_enchantments
        .push((gear_id, bace_magic::EnchantmentRegistry::new(4096).unwrap()));
    let equipped = prepare_equipment_physical(EquipmentPhysicalInput {
        actor: binding.actor,
        before_revision: prepared.state.character.progression.revision(),
        source: &loaded.player.player.entity.state,
        state: &prepared.state,
        items: std::slice::from_ref(&gear),
        dat: &dat,
        projectile_shapes: &Default::default(),
    })
    .unwrap();
    assert_eq!(equipped.source.profile.equipment.len(), 1);
    assert_eq!(equipped.source.profile.equipment[0].entity, gear_id.0);
    assert_eq!(equipped.vital_inputs.equipped_health, vec![(gear_id, 40)]);
    assert!(
        prepare_equipment_physical(EquipmentPhysicalInput {
            actor: binding.actor,
            before_revision: prepared.state.character.progression.revision() + 1,
            source: &loaded.player.player.entity.state,
            state: &prepared.state,
            items: std::slice::from_ref(&gear),
            dat: &dat,
            projectile_shapes: &Default::default()
        })
        .is_err()
    );
    prepared.state.item_enchantments.pop();
    assert!(prepared.actor.body.collision_shape().is_some());
    assert_eq!(prepared.chat_eligibility.player_age_seconds, 45);
    assert_eq!(prepared.state.chat_age, Some(45));
    assert_eq!(prepared.presence.society, 7);
    assert_eq!(prepared.combatant.health(), 10);
    assert_eq!(
        prepared.skills.inputs.len(),
        loaded.player.player.entity.state.properties.skills.len()
    );
    let loaded = std::sync::Arc::new(loaded);
    let directory = tempfile::tempdir().unwrap();
    let built = bace_content_tools::build_world_pack(
        std::slice::from_ref(&loaded.player.player.entity.state),
        &[],
        directory.path(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let pack = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = std::sync::Arc::new(pack.open(directory.path(), Default::default()).unwrap());
    let mut worker =
        bace_runtime::player_preparation_worker::PlayerPreparationWorker::start(manifest, 1)
            .unwrap();
    assert!(
        worker
            .try_submit(
                bace_runtime::player_preparation_worker::PlayerPreparationRequest {
                    generation,
                    correlation: 901,
                    loaded: loaded.clone(),
                    geometry: geometry.clone(),
                    spell_rows: Default::default(),
                    component_templates: std::sync::Arc::from([]),
                    projectile_shapes: Default::default(),
                    policy: policy(),
                    now_unix_millis: 0,
                }
            )
            .is_ok()
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let cold = loop {
        if let Some(done) = worker.try_recv().unwrap() {
            assert_eq!(done.correlation, 901);
            assert_eq!(done.key, loaded.key);
            assert_eq!(done.binding, binding);
            break done.result.unwrap();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "cold avatar worker deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert_eq!(cold.admission.binding, prepared.binding);
    assert_eq!(
        cold.admission.actor.body.accepted().position(),
        prepared.actor.body.accepted().position()
    );
    assert!(!cold.appearance.setups.is_empty());
    assert_eq!(
        cold.character_assets.char_gen().heritage_groups.len(),
        dat.character.char_gen().heritage_groups.len()
    );
    assert!(cold.enchantments.is_empty());
    assert_eq!(worker.pending(), 0);
    worker.try_shutdown().ok().unwrap().join().unwrap();
    let mut world = bace_world::World::default();
    world.install_geometry(geometry).unwrap();
    let mut kernel = bace_simulation::Kernel::with_gameplay_limits(world, 32, 8, 32).unwrap();
    kernel
        .configure_social_random(std::sync::Arc::new(
            bace_random::RandomRoot::new([23; 32], 1).unwrap(),
        ))
        .unwrap();
    kernel
        .admit_player(cold.admission)
        .map_err(|(error, _)| error)
        .unwrap();
    assert!(kernel.avatar_locomotion(binding.actor).is_some());
    assert_eq!(kernel.social_player_age(binding.actor), Some(45));
    assert!(kernel.world().scene(bace_types::CellId(cell)).is_err());
    let setup_id = loaded
        .player
        .player
        .entity
        .state
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 1)
        .unwrap()
        .value;
    let setup = &cold.appearance.setups[&setup_id];
    let receipt = bace_persistence::OnlineLoginReceipt {
        lease: bace_persistence::CharacterLease {
            state: bace_persistence::OwnershipState::Online,
            ..loaded.lease
        },
        total_logins: 65536,
    };
    let sequences = bace_replication::Sequences::with_instance(64, 0).unwrap();
    let mut last = String::new();
    let mut entry = None;
    for _ in 0..300 {
        let snapshot = kernel.read_player_snapshot(binding).unwrap();
        match bace_runtime::player_entry::prepare_player_entry_state(
            &loaded.player,
            &snapshot,
            receipt,
            &sequences,
            setup,
        ) {
            Ok(value) => {
                entry = Some(value);
                break;
            }
            Err(error) => last = format!("{error}: {:?}", snapshot.entry_physics()),
        }
        kernel.step().unwrap();
    }
    assert!(
        entry.is_some(),
        "real DAT avatar never reached accepted entry state: {last}"
    );

    equipment_transaction::exercise(
        &mut kernel,
        binding,
        &loaded.player.player.entity.state,
        &dat,
        gear,
    );
}
