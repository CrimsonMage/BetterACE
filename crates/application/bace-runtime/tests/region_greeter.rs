mod treasure_table_support;

#[test]
#[ignore = "requires approved DATs and accepted native world pack"]
fn supplied_7f03_region_asset_closure_prepares() {
    treasure_table_support::install_tables();
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
            landblock: 0x7f03,
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
    assert!(!region.content.templates.is_empty());
    assert!(!region.visibility.is_empty() || region.geometry.cell_ids().next().is_some());
    let root = bace_random::RandomRoot::new([7; 32], 1).unwrap();
    let mut clothed = 0;
    for instance in region
        .content
        .instances
        .iter()
        .filter(|instance| matches!(instance.template.weenie_id, 30997 | 12718 | 30991))
    {
        use bace_gameplay_api::{
            GeneratorDestination, GeneratorIdentity, GeneratorLocation, GeneratorProfile,
            GeneratorSpawnIntent, GeneratorSpawnKey,
        };
        let actor = bace_types::EntityId(instance.source.guid);
        let identity = GeneratorIdentity {
            entity: actor,
            incarnation: 1,
            content_revision: region.generation.revision(),
            random_identity: [1; 16],
        };
        let intent = GeneratorSpawnIntent {
            key: GeneratorSpawnKey {
                generator: identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile: GeneratorProfile {
                id: 0,
                probability: -1.,
                weenie_class_id: instance.template.weenie_id,
                delay: None,
                init_create: 1,
                max_create: 1,
                when_create: 1,
                where_create: 1,
                stack_size: None,
                palette_id: None,
                shade: None,
                position: Default::default(),
            },
            destination: GeneratorDestination::Default(GeneratorLocation {
                cell: instance.source.obj_cell_id,
                origin: [0.; 3],
                rotation: [0., 0., 0., 1.],
            }),
            first_spawn: true,
            due_tick: 0,
            random_identity: [1; 16],
            random_key_version: 1,
        };
        let gear = bace_runtime::generator_equipment::materialize_creature_equipment(
            &instance.template,
            &region.catalog.templates,
            &[],
            &root,
            &intent,
        )
        .unwrap();
        eprintln!(
            "NPC {} guid={:08x} link_child={} gear={}",
            instance.template.weenie_id,
            instance.source.guid,
            instance.source.is_link_child,
            gear.len()
        );
        if instance.template.weenie_id == 30991 {
            // ACE PropertyInt.ItemUseable=16; the pinned source advertises
            // interaction to the retail client before any move-to request.
            assert_eq!(
                instance
                    .template
                    .properties
                    .ints
                    .iter()
                    .find(|p| p.id == 16)
                    .map(|p| p.value),
                Some(32)
            );
        }
        assert!(
            gear.iter().any(|item| item.wielded_location != 0),
            "authored NPC {} has no visible Wield item",
            instance.template.weenie_id
        );
        if instance.template.weenie_id == 30991 {
            let items: Vec<_> = gear
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    (
                        bace_types::EntityId(0x8000_0100 + index as u32),
                        item.clone(),
                    )
                })
                .collect();
            let creature = region.creatures[&30991].as_ref().unwrap();
            let loadout = bace_runtime::generator_equipment::prepare_creature_loadout(
                bace_types::EntityId(0x8000_0001),
                &instance.template,
                &items,
                creature,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(loadout.items.iter().filter(|item| matches!(item.place, bace_inventory::ItemPlace::Contained { equipped, .. } if equipped != 0)).count(), 2);
            let source = instance.template.as_ref();
            let bare = assets
                .prepare_visibility_sources(vec![
                    bace_runtime::visibility_assets::VisibilitySource {
                        entity: bace_types::EntityId(0x8000_0001),
                        incarnation: 1,
                        revision: 1,
                        source,
                        equipment: vec![],
                        missile_combat: false,
                    },
                ])
                .unwrap();
            let greeter = &bare[0].description.game;
            assert_eq!(greeter.item_type, 16);
            assert_eq!(greeter.options.usable, Some(32));
            assert_eq!(greeter.options.use_radius, Some(3.0));
            assert_ne!(greeter.options.header_flags().0 & 0x10, 0);
            let worn = assets
                .prepare_visibility_sources(vec![
                    bace_runtime::visibility_assets::VisibilitySource {
                        entity: bace_types::EntityId(0x8000_0001),
                        incarnation: 1,
                        revision: 1,
                        source,
                        equipment: items
                            .iter()
                            .filter(|(_, item)| item.wielded_location != 0)
                            .map(|(id, item)| (*id, &item.source, item.wielded_location))
                            .collect(),
                        missile_combat: false,
                    },
                ])
                .unwrap();
            assert_ne!(bare[0].description.model, worn[0].description.model);
        }
        clothed += 1;
    }
    assert_eq!(clothed, 3);
}
