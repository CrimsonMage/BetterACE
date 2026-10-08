use bace_character::*;
use bace_content::{Position, Property, SparseProperties, WeenieV1};
use bace_gameplay_api::{CreationAllocation, CreationAttributes, SkillAdvancement};
fn template(id: u32, kind: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("class{id}"),
        weenie_type: kind,
        last_modified: None,
        properties: SparseProperties::default(),
    }
}
fn fixture() -> (CharacterCreateRequest, PreparedCreationAssets) {
    let texture = FaceTextures {
        old: 0x05000001,
        new: 0x05000002,
    };
    let gender = PreparedGender {
        gender: 1,
        scale: 100,
        setup: 0x02000001,
        motion: 0x09000001,
        sound: 0x20000001,
        physics: 0x34000001,
        combat: 0x30000001,
        palette_base: 0x04000001,
        hair: vec![PreparedHair {
            bald: false,
            alternate_setup: 0,
            head_object: Some(0x01000001),
            texture: Some(texture),
            multiple_parts: false,
        }],
        hair_palette_sets: vec![vec![0x04000002, 0x04000003]],
        skin_palettes: vec![0x04000004, 0x04000005],
        eye_palettes: vec![0x04000006],
        eyes: vec![PreparedEyes {
            normal: texture,
            bald: texture,
        }],
        noses: vec![texture],
        mouths: vec![texture],
        clothing: [vec![], vec![11], vec![12], vec![13]],
        clothing_colors: vec![1],
    };
    let selection = ClothingSelection {
        style: 0,
        color: 1,
        hue: 0.5,
    };
    let request = CharacterCreateRequest {
        account: 1,
        entity: 100,
        name: "Arwic Hero".into(),
        heritage: 1,
        gender: 1,
        template_index: 0,
        start_area: 3,
        allocation: CreationAllocation {
            attributes: CreationAttributes {
                strength: 100,
                endurance: 100,
                coordination: 50,
                quickness: 50,
                focus: 20,
                self_attribute: 10,
            },
            skills: [SkillAdvancement::Inactive; 55],
        },
        appearance: AppearanceSelection {
            hair_style: 0,
            hair_color: 0,
            hair_hue: 1.0,
            skin_hue: 0.0,
            eyes: 0,
            eye_color: 0,
            nose: 0,
            mouth: 0,
            clothing: [
                ClothingSelection {
                    style: u32::MAX,
                    ..selection
                },
                selection,
                selection,
                selection,
            ],
        },
    };
    let position = Position {
        obj_cell_id: 0xA9B40019,
        position_x: 84.0,
        position_y: 7.1,
        position_z: 94.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    };
    let items = (11..=13)
        .map(|id| {
            let mut state = template(id, 2);
            state.properties.ints.push(Property {
                id: 9,
                value: 1 << (id - 11),
            });
            StarterItem {
                template: state,
                revision: 1,
                clothing_icons: vec![(1, 0x06000001)],
            }
        })
        .collect();
    let assets = PreparedCreationAssets {
        heritage: 1,
        heritage_name: "Aluvian".into(),
        rules: CreationRules::new(330, 52, [None; 55]).unwrap(),
        genders: vec![gender],
        templates: vec![CharacterTemplate {
            name: "Custom".into(),
            title: 1,
        }],
        starts: vec![CharacterStart {
            area: 3,
            location: position.clone(),
            instantiation: position,
        }],
        human: template(1, 10),
        human_revision: 7,
        items,
        skill_gear: vec![],
        skill_spells: vec![],
        vital_formulas: [
            VitalFormula {
                enabled: true,
                divisor: 2,
                attribute1: 2,
                attribute2: 0,
            },
            VitalFormula {
                enabled: true,
                divisor: 1,
                attribute1: 2,
                attribute2: 0,
            },
            VitalFormula {
                enabled: true,
                divisor: 1,
                attribute1: 6,
                attribute2: 0,
            },
        ],
    };
    (request, assets)
}
#[test]
fn creation_proposes_complete_humanoid_snapshot_and_owned_starting_clothing() {
    let (request, assets) = fixture();
    let result = prepare_character(
        &request,
        &assets,
        &NamePolicy::prepare(&[], &[])
            .unwrap()
            .approve(&request.name)
            .unwrap(),
        &[200, 201, 202],
    )
    .unwrap();
    assert_eq!(result.entity, 100);
    assert_eq!(result.template_revision, 7);
    assert_eq!(result.metadata.hair_texture, 0x05000002);
    assert_eq!(result.metadata.options1, 1_355_064_650);
    assert_eq!(result.metadata.options2, 9_733_888 | 0x02000000);
    assert_eq!(
        result
            .state
            .properties
            .secondary_attributes
            .iter()
            .map(|p| p.value.current_level)
            .collect::<Vec<_>>(),
        [50, 100, 10]
    );
    assert_eq!(
        result
            .state
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 15)
            .unwrap()
            .value,
        0x04000003
    );
    assert_eq!(
        result
            .state
            .properties
            .attributes
            .iter()
            .find(|p| p.id == 3)
            .unwrap()
            .value
            .init_level,
        50
    );
    assert_eq!(result.possessions.len(), 3);
    for item in &result.possessions {
        assert!(item.equipped > 0);
        assert_eq!(
            item.state
                .properties
                .instance_ids
                .iter()
                .find(|p| p.id == 3)
                .unwrap()
                .value,
            100
        );
    }
    assert!(assets.human.properties.attributes.is_empty());
}
#[test]
fn missing_assets_and_invalid_creation_do_not_partially_create() {
    let (mut request, mut assets) = fixture();
    let approved = NamePolicy::prepare(&[], &[])
        .unwrap()
        .approve(&request.name)
        .unwrap();
    assert_eq!(
        prepare_character(
            &request,
            &assets,
            &NamePolicy::prepare(&[], &[])
                .unwrap()
                .approve("Other")
                .unwrap(),
            &[200, 201, 202]
        ),
        Err(FactoryError::NameNotApproved)
    );
    assert_eq!(
        prepare_character(&request, &assets, &approved, &[200, 200, 202]),
        Err(FactoryError::DuplicateId)
    );
    assert_eq!(
        prepare_character(&request, &assets, &approved, &[200]),
        Err(FactoryError::InsufficientIds)
    );
    request.appearance.skin_hue = f64::NAN;
    assert_eq!(
        prepare_character(&request, &assets, &approved, &[200, 201, 202]),
        Err(FactoryError::InvalidHue)
    );
    request.appearance.skin_hue = 0.5;
    assets.items.pop();
    assert_eq!(
        prepare_character(&request, &assets, &approved, &[200, 201, 202]),
        Err(FactoryError::MissingAsset)
    );
    assert!(assets.human.properties.attributes.is_empty());
}
#[test]
fn independent_official_palette_vital_and_heritage_vectors() {
    for line in include_str!("fixtures/factory.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let (mut request, mut assets) = fixture();
        match p[0] {
            "palette" => {
                assets.genders[0].skin_palettes = (0..p[1].parse::<u32>().unwrap())
                    .map(|i| 0x04000000 + i)
                    .collect();
                request.appearance.skin_hue = p[2].parse().unwrap();
            }
            "vital" => {
                assets.rules = CreationRules::new(600, 52, [None; 55]).unwrap();
                request.allocation.attributes.endurance = p[1].parse().unwrap();
                request.allocation.attributes.self_attribute = p[2].parse().unwrap();
                assets.vital_formulas[0] = VitalFormula {
                    enabled: true,
                    divisor: p[3].parse().unwrap(),
                    attribute1: 2,
                    attribute2: 6,
                };
            }
            "heritage" => {
                request.heritage = p[1].parse().unwrap();
                assets.heritage = request.heritage;
            }
            _ => panic!("unexpected oracle row {line}"),
        }
        let character = prepare_character(
            &request,
            &assets,
            &NamePolicy::prepare(&[], &[])
                .unwrap()
                .approve(&request.name)
                .unwrap(),
            &[200, 201, 202],
        )
        .unwrap();
        let props = &character.state.properties;
        match p[0] {
            "palette" => assert_eq!(
                props.data_ids.iter().find(|v| v.id == 17).unwrap().value,
                p[3].parse::<u32>().unwrap(),
                "{line}"
            ),
            "vital" => assert_eq!(
                props
                    .secondary_attributes
                    .iter()
                    .find(|v| v.id == 1)
                    .unwrap()
                    .value
                    .current_level,
                p[4].parse::<u32>().unwrap(),
                "{line}"
            ),
            "heritage" => {
                for (id, index) in [
                    (354, 2),
                    (355, 3),
                    (326, 4),
                    (298, 5),
                    (310, 6),
                    (233, 7),
                    (296, 8),
                    (299, 9),
                    (230, 10),
                ] {
                    assert_eq!(
                        props
                            .ints
                            .iter()
                            .find(|v| v.id == id)
                            .map_or(0, |v| v.value),
                        p[index].parse::<i32>().unwrap(),
                        "{line} prop {id}"
                    );
                }
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn humanoid_creation_applies_player_constructor_defaults_without_reusing_template_birth_date() {
    let (request, mut assets) = fixture();
    assets.human.properties.bools.push(Property {
        id: 19,
        value: false,
    });
    assets
        .human
        .properties
        .create_list
        .push(bace_content::CreateListEntry {
            weenie_class_id: 999,
            destination_type: 1,
            stack_size: 1,
            ..Default::default()
        });
    assets.human.properties.strings.push(Property {
        id: 43,
        value: "01 January 1900".into(),
    });
    let approved = NamePolicy::prepare(&[], &[])
        .unwrap()
        .approve(&request.name)
        .unwrap();
    let prepared = prepare_character(&request, &assets, &approved, &[200, 201, 202]).unwrap();
    assert!(
        prepared
            .state
            .properties
            .bools
            .iter()
            .find(|p| p.id == 19)
            .unwrap()
            .value
    );
    assert_eq!(
        prepared
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .value,
        0
    );
    assert_eq!(
        prepared
            .state
            .properties
            .int64s
            .iter()
            .find(|p| p.id == 2)
            .unwrap()
            .value,
        0
    );
    assert!(prepared.state.properties.create_list.is_empty());
    assert!(!prepared.state.properties.strings.iter().any(|p| p.id == 43));
    // ACE only supplies zero for absent values; trusted initialized values survive.
    assets.human.properties.int64s = vec![
        Property { id: 1, value: 100 },
        Property { id: 2, value: 50 },
    ];
    let prepared = prepare_character(&request, &assets, &approved, &[200, 201, 202]).unwrap();
    assert_eq!(
        prepared.state.properties.int64s,
        assets.human.properties.int64s
    );
    assert_eq!(assets.human.properties.create_list.len(), 1);
}
#[test]
fn starting_clothing_uses_original_coverage_overlap_not_wield_locations() {
    for row in include_str!("fixtures/creation_clothing.csv")
        .lines()
        .filter(|row| !row.starts_with('#'))
    {
        let values: Vec<i32> = row.split(',').map(|v| v.parse().unwrap()).collect();
        let (request, mut assets) = fixture();
        for (index, (coverage, wield)) in [(values[0], 4), (values[1], values[2]), (0, 256)]
            .into_iter()
            .enumerate()
        {
            let p = &mut assets.items[index].template.properties.ints;
            p.iter_mut().find(|p| p.id == 9).unwrap().value = wield;
            if coverage >= 0 {
                p.push(Property {
                    id: 4,
                    value: coverage,
                });
                p.sort_by_key(|p| p.id);
            }
        }
        let policy = NamePolicy::prepare(&[], &[]).unwrap();
        let name = policy.approve(&request.name).unwrap();
        let result = prepare_character(&request, &assets, &name, &[101, 102, 103]);
        assert_eq!(result.is_ok(), values[3] == 1, "{row}");
    }
}
