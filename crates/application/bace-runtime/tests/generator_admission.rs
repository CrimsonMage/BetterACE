use bace_geometry::Vec3;
use bace_runtime::{
    region_activation::{RegionAssetManifest, VerifiedRegionAssets},
    region_geometry::normalize_outdoor,
};
#[test]
fn outdoor_coordinates_normalize_across_block_edges_without_wrapping_world() {
    assert_eq!(
        normalize_outdoor(0x12340001, Vec3::new(24., 48., 7.)).unwrap(),
        (0x1234000b, Vec3::new(24., 48., 7.))
    );
    assert_eq!(
        normalize_outdoor(0x12340001, Vec3::new(193., -1., 7.)).unwrap(),
        (0x13330008, Vec3::new(1., 191., 7.))
    );
    assert!(normalize_outdoor(0x00000001, Vec3::new(-1., 0., 0.)).is_err());
    assert!(normalize_outdoor(0xfe000001, Vec3::new(192., 0., 0.)).is_err());
    assert!(normalize_outdoor(0x12340000, Vec3::ZERO).is_err());
}
#[test]
fn unapproved_dat_never_produces_region_assets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unapproved.dat");
    std::fs::write(&path, b"untrusted").unwrap();
    assert!(
        VerifiedRegionAssets::open(&RegionAssetManifest {
            portal: path.clone(),
            cell: path,
            portal_sha256: "0".repeat(64),
            cell_sha256: "0".repeat(64)
        })
        .is_err()
    );
}
#[test]
#[ignore = "requires the complete accepted BACE_WORLD_MANIFEST"]
fn academy_researcher_full_pack_minus_one_capacity_matches_ace_slot_rule() {
    use bace_loot::{TreasureError, TreasureRandom, generate_creature_equipment};
    use bace_storage_codec::{PackKey, PackLookup};
    use std::{collections::BTreeMap, sync::Arc};
    #[derive(Clone)]
    struct First;
    impl TreasureRandom for First {
        fn unit(&mut self) -> Result<f64, TreasureError> {
            Ok(0.0)
        }
        fn inclusive(&mut self, low: i32, _: i32) -> Result<i32, TreasureError> {
            Ok(low)
        }
    }
    let path = std::path::PathBuf::from(
        std::env::var_os("BACE_WORLD_MANIFEST").expect("accepted full world manifest"),
    );
    let manifest = bace_storage_codec::load_manifest(&path, Default::default()).unwrap();
    let pack = manifest
        .open(path.parent().unwrap(), Default::default())
        .unwrap();
    let read = |id: u32| {
        let PackLookup::Record(record) = pack
            .lookup(PackKey {
                namespace: 1,
                id: u64::from(id),
            })
            .unwrap()
        else {
            panic!("missing complete-world WCID {id}");
        };
        bace_content_tools::decode(record.bytes()).unwrap()
    };
    let mut source = read(30997);
    assert_eq!(source.class_name, "academyresearcher");
    for id in [6, 7] {
        assert_eq!(
            source
                .properties
                .ints
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .value,
            -1
        );
    }
    // Keep one unchanged pinned Wield row to isolate ACE's signed capacity
    // comparison from other constructor content and random choices.
    source
        .properties
        .create_list
        .retain(|r| r.weenie_class_id == 118);
    assert_eq!(source.properties.create_list.len(), 1);
    let templates = BTreeMap::from([(118, Arc::new(read(118)))]);
    let prepared =
        generate_creature_equipment(&source, &templates, None, &[], false, &mut First).unwrap();
    assert!(prepared.is_empty());
}
#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn verified_complete_region_supports_encounters_and_real_building_filter() {
    let dir =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: dir.join("client_portal.dat"),
        cell: dir.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let region = assets.prepare_geometry(0xa260).unwrap();
    let mut accepted = 0;
    let mut buildings = 0;
    for x in 0..8 {
        for y in 0..8 {
            let result = bace_runtime::region_geometry::encounter_position(&region, 0xa260, x, y);
            let cell = 0xa2600001 + (x as u32) * 8 + y as u32;
            if region.has_building(cell).unwrap() {
                buildings += 1;
                assert!(result.is_err());
            } else {
                let (cell, p) = result.unwrap();
                let (z, normal) = region.ground_at(cell, p).unwrap();
                assert_eq!(p.z, z);
                assert!(normal.z > 0.);
                accepted += 1;
            }
        }
    }
    assert!(accepted > 0);
    assert!(buildings > 0);
    assert_eq!(accepted + buildings, 64);
    eprintln!("authentic encounter candidates={accepted}, building cells={buildings}");
}

#[test]
#[ignore = "requires approved actual DATs; set BACE_DAT_DIRECTORY"]
fn native_creature_uses_real_dat_motions_health_and_bodyparts() {
    use bace_content::{
        Attribute, BodyPart, Property, SecondaryAttribute, Skill, SparseProperties, WeenieV1,
    };
    let dir =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"));
    let manifest = RegionAssetManifest {
        portal: dir.join("client_portal.dat"),
        cell: dir.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
    };
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let chargen = bace_dat::CharGen::decode(&portal.read(0x0e000002).unwrap()).unwrap();
    let setup = chargen.heritage_groups[&1].genders[&1].setup;
    let properties = SparseProperties {
        data_ids: vec![
            Property {
                id: 1,
                value: setup,
            },
            Property {
                id: 2,
                value: 0x09000001,
            },
            Property {
                id: 4,
                value: 0x30000000,
            },
        ],
        attributes: (1..=6)
            .map(|id| Property {
                id,
                value: Attribute {
                    init_level: if id == 2 { 101 } else { 100 },
                    level_from_cp: 0,
                    cp_spent: 0,
                },
            })
            .collect(),
        secondary_attributes: vec![Property {
            id: 1,
            value: SecondaryAttribute {
                init_level: 12,
                level_from_cp: 3,
                cp_spent: 0,
                current_level: 1,
            },
        }],
        skills: vec![Property {
            id: 24,
            value: Skill {
                sac: 2,
                init_level: 10,
                ..Default::default()
            },
        }],
        body_parts: vec![Property {
            id: 0,
            value: BodyPart {
                d_type: 4,
                d_val: 12,
                d_var: 0.2,
                armor_vs_slash: 17,
                armor_vs_pierce: 19,
                hlf: 1.,
                mlf: 1.,
                llf: 1.,
                hrf: 1.,
                mrf: 1.,
                lrf: 1.,
                hlb: 1.,
                mlb: 1.,
                llb: 1.,
                hrb: 1.,
                mrb: 1.,
                lrb: 1.,
                ..Default::default()
            },
        }],
        ..Default::default()
    };
    let template = WeenieV1 {
        schema_version: 1,
        weenie_id: 200,
        class_name: "synthetic-creature".into(),
        weenie_type: 10,
        last_modified: None,
        properties,
    };
    let physical = assets.prepare_template(&template).unwrap();
    let creature = assets
        .prepare_creature(
            &template,
            &physical,
            bace_runtime::generator_preparation::CreatureAdmissionPolicy {
                corpse_template: 201,
                think_interval: 5,
                corpse_decay_ticks: 450,
            },
        )
        .unwrap();
    assert_eq!(creature.blueprint.combat.maximum_health, 66);
    let profile = creature.physical.unwrap();
    assert_eq!(profile.body_attacks[0].damage, 12.);
    assert_eq!(profile.armor[0].armor[0].raw, 17.);
    assert!(creature.blueprint.death_animation_ticks > 0);
    assert!(profile.maneuvers.iter().any(|m| !m.hooks.is_empty()));
    assert_eq!(creature.blueprint.respawn_ticks, 0);
    // Vendor inherits Creature in the original factory; passive admission uses
    // the same real motions/vitals without inventing an aggressive NPC profile.
    let mut vendor = template;
    vendor.weenie_type = 12;
    // GDLE WeenieObject::InitPhysicsObj accepts an absent combat table;
    // MeleeAttackEventData::Setup falls back to authored unarmed motion IDs.
    // Real Academy Researcher WCID 30997 has the same absent DID 4 shape.
    vendor
        .properties
        .data_ids
        .retain(|property| property.id != 4);
    vendor.properties.bools.push(Property {
        id: 19,
        value: false,
    });
    let vendor = assets
        .prepare_creature(
            &vendor,
            &physical,
            bace_runtime::generator_preparation::CreatureAdmissionPolicy {
                corpse_template: 201,
                think_interval: 5,
                corpse_decay_ticks: 450,
            },
        )
        .unwrap();
    assert_eq!(vendor.blueprint.combat.maximum_health, 66);
    let vendor_profile = vendor.physical.unwrap();
    assert_eq!(vendor_profile.body_attacks[0].damage, 12.);
    assert!(
        vendor_profile
            .maneuvers
            .iter()
            .any(|motion| !motion.hooks.is_empty())
    );
    assert!(!vendor.physical_motions.is_empty());
}
