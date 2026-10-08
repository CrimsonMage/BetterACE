use bace_content::{Position, Property, WeenieV1};
use bace_dat::*;
use bace_runtime::{
    character_assets::{PreparedCharacterAssets, prepare_character_assets},
    character_preparation::*,
};
use std::collections::BTreeMap;
fn fixture() -> (XpTable, SkillTable, CharGen) {
    // Synthetic cumulative tables already exercised by bace-character's
    // independent official C# progression oracle, not invented runtime defaults.
    let xp = XpTable {
        attribute_xp: vec![0, 10, 30, 30, 100],
        vital_xp: vec![0, 2, 8, 20, 100],
        trained_skill_xp: vec![0, 5, 15, 50, 100],
        specialized_skill_xp: vec![0, 1, 7, 40, 100],
        character_level_xp: vec![0, 1000, 3000],
        character_level_skill_credits: vec![0, 1, 2],
    };
    let skill = |trained, total| SkillBase {
        description: "retained description".into(),
        name: "synthetic".into(),
        icon_id: 123,
        trained_cost: trained,
        specialized_cost: total,
        category: 1,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 1,
            x: 1,
            y: 0,
            z: 2,
            attribute1: 2,
            attribute2: 4,
        },
        upper_bound: 1.0,
        lower_bound: 0.0,
        learn_modifier: 0.03,
    };
    let skills = SkillTable {
        bucket_size: 17,
        skills: BTreeMap::from([(6, skill(6, 18)), (7, skill(4, 10))]),
    };
    let heritage = HeritageGroup {
        name: "synthetic heritage".into(),
        icon: 42,
        setup: 43,
        environment_setup: 44,
        attribute_credits: 330,
        skill_credits: 52,
        primary_start_areas: vec![],
        secondary_start_areas: vec![],
        skills: vec![CreationSkill {
            skill: 6,
            normal_cost: 2,
            primary_cost: 3,
        }],
        templates: vec![],
        gender_marker: 1,
        genders: BTreeMap::new(),
    };
    let chargen = CharGen {
        reserved: 77,
        starter_areas: vec![],
        heritage_marker: 1,
        heritage_groups: BTreeMap::from([(1, heritage)]),
    };
    (xp, skills, chargen)
}

fn object_description() -> ObjectDescription {
    ObjectDescription {
        marker: 0,
        palette_id: None,
        sub_palettes: vec![],
        texture_changes: vec![TextureChange {
            part: 16,
            old_texture: 0x05000001,
            new_texture: 0x05000002,
        }],
        animation_parts: vec![AnimationPart {
            part: 16,
            id: 0x01000001,
        }],
    }
}
fn position() -> Position {
    Position {
        obj_cell_id: 0x12340001,
        position_x: 1.0,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        rotation_x: 0.0,
        rotation_y: 0.0,
        rotation_z: 0.0,
    }
}
fn template(id: u32) -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("class{id}"),
        weenie_type: if id == 1 { 10 } else { 2 },
        last_modified: None,
        properties: Default::default(),
    }
}
struct Inputs {
    base: PreparedCharacterAssets,
    vitals: VitalTable,
    palettes: BTreeMap<u32, DatPaletteSet>,
    clothing: BTreeMap<u32, ClothingTable>,
    human: CreationTemplateInput,
    items: BTreeMap<u32, CreationTemplateInput>,
    starts: Vec<AdmittedStart>,
}
impl Inputs {
    fn new() -> Self {
        let (xp, skills, mut chargen) = fixture();
        let appearance = object_description();
        let gear = |id| CreationGear {
            name: format!("gear{id}"),
            clothing_table: 0x10000000 + id,
            weenie: id,
        };
        let gender = CreationGender {
            name: "Synthetic gender".into(),
            scale: 100,
            setup: 0x02000001,
            sound_table: 0x20000001,
            icon: 0x06000001,
            base_palette: 0x04000001,
            skin_palette_set: 0x0f000001,
            physics_table: 0x34000001,
            motion_table: 0x09000001,
            combat_table: 0x30000001,
            appearance: appearance.clone(),
            hair_colors: vec![0x0f000002],
            hair_styles: vec![HairStyle {
                icon: 0,
                bald: false,
                alternate_setup: 0,
                appearance: appearance.clone(),
            }],
            eye_colors: vec![0x04000004],
            eyes: vec![EyeStrip {
                icon: 0,
                bald_icon: 0,
                appearance: appearance.clone(),
                bald_appearance: appearance.clone(),
            }],
            noses: vec![FaceStrip {
                icon: 0,
                appearance: appearance.clone(),
            }],
            mouths: vec![FaceStrip {
                icon: 0,
                appearance,
            }],
            headgear: vec![],
            shirts: vec![gear(11)],
            pants: vec![gear(12)],
            footwear: vec![gear(13)],
            clothing_colors: vec![1],
        };
        let heritage = chargen.heritage_groups.get_mut(&1).unwrap();
        heritage.genders.insert(1, gender);
        heritage.templates.push(CreationTemplate {
            name: "Adventurer".into(),
            icon: 0,
            title: 7,
            attributes: [10; 6],
            normal_skills: vec![],
            primary_skills: vec![],
        });
        chargen.starter_areas.push(StarterArea {
            name: "Synthetic start".into(),
            locations: vec![CreationPosition {
                cell: 0x12340001,
                origin: [1.0, 2.0, 3.0],
                orientation_wxyz: [1.0, 0.0, 0.0, 0.0],
            }],
        });
        let base = prepare_character_assets(xp, skills, chargen).unwrap();
        let formula = SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 2,
            attribute1: 2,
            attribute2: 0,
        };
        let vitals = VitalTable {
            health: formula,
            stamina: formula,
            mana: SkillFormula {
                attribute1: 6,
                ..formula
            },
        };
        let palettes = [
            (0x0f000001, vec![0x04000001, 0x04000002]),
            (0x0f000002, vec![0x04000003]),
        ]
        .into_iter()
        .map(|(id, palettes)| (id, DatPaletteSet { id, palettes }))
        .collect();
        let mut items = BTreeMap::new();
        let mut clothing = BTreeMap::new();
        for id in 11..=13 {
            let mut value = template(id);
            value.properties.data_ids.push(Property {
                id: 7,
                value: 0x10000000 + id,
            });
            value.properties.ints.push(Property {
                id: 9,
                value: 1 << (id - 11),
            });
            items.insert(
                id,
                CreationTemplateInput {
                    revision: 1,
                    template: value,
                },
            );
            clothing.insert(
                0x10000000 + id,
                ClothingTable {
                    id: 0x10000000 + id,
                    setups: BTreeMap::new(),
                    templates: BTreeMap::from([(
                        1,
                        ClothingTemplate {
                            icon: 0x06000000 + id,
                            palettes: vec![],
                        },
                    )]),
                    template_order: vec![1],
                },
            );
        }
        Self {
            base,
            vitals,
            palettes,
            clothing,
            human: CreationTemplateInput {
                revision: 1,
                template: template(1),
            },
            items,
            starts: vec![AdmittedStart {
                area: 0,
                instantiation: position(),
            }],
        }
    }
    fn supplement(&self) -> CreationSupplementary<'_> {
        CreationSupplementary {
            vitals: &self.vitals,
            palettes: &self.palettes,
            clothing: &self.clothing,
            human: &self.human,
            items: &self.items,
            skill_gear: Some(&[]),
            skill_spells: Some(&[]),
            starts: Some(&self.starts),
        }
    }
}
fn account() -> bace_auth::AccountRecord {
    bace_auth::AccountRecord {
        id: bace_types::AccountId(7),
        name: bace_auth::AccountName::parse("creator").unwrap(),
        password_hash: bace_auth::PasswordService::new(1)
            .unwrap()
            .hash(b"synthetic-password")
            .unwrap(),
        access_level: bace_auth::AccessLevel::Player,
        disabled: false,
    }
}
fn request() -> bace_wire::CharacterCreateRequest {
    bace_wire::CharacterCreateRequest {
        account: "Creator".into(),
        unknown_constant: 1,
        heritage: 1,
        gender: 1,
        appearance: bace_wire::CharacterAppearance {
            eyes: 0,
            nose: 0,
            mouth: 0,
            hair_color: 0,
            eye_color: 0,
            hair_style: 0,
            headgear_style: u32::MAX,
            headgear_color: 1,
            shirt_style: 0,
            shirt_color: 1,
            pants_style: 0,
            pants_color: 1,
            footwear_style: 0,
            footwear_color: 1,
            skin_hue: 1.0,
            hair_hue: 0.0,
            headgear_hue: 0.5,
            shirt_hue: 0.5,
            pants_hue: 0.5,
            footwear_hue: 0.5,
        },
        template_option: 0,
        abilities: bace_wire::CharacterAbilities {
            strength: 100,
            endurance: 100,
            coordination: 50,
            quickness: 50,
            focus: 20,
            self_ability: 10,
        },
        character_slot: 0,
        class_id: 0,
        skill_advancement_classes: vec![0; 55],
        name: "Synthetic Hero".into(),
        start_area: 0,
        requested_admin: true,
        requested_sentinel: true,
        trailing_bytes: 0,
    }
}
#[test]
fn authenticated_intent_and_verified_appearance_prepare_a_domain_proposal_without_client_privilege()
{
    let input = Inputs::new();
    let assets = prepare_creation_assets(&input.base, 1, input.supplement()).unwrap();
    let request = creation_request(&request(), &account(), 0x50000001).unwrap();
    assert_eq!(request.account, 7);
    assert_eq!(request.entity, 0x50000001);
    assert_eq!(assets.genders[0].hair[0].head_object, Some(0x01000001));
    assert_eq!(assets.genders[0].skin_palettes, [0x04000001, 0x04000002]);
    assert_eq!(assets.starts[0].location, position());
    let character = bace_character::prepare_character(
        &request,
        &assets,
        &bace_character::NamePolicy::prepare(&[], &[])
            .unwrap()
            .approve(&request.name)
            .unwrap(),
        &[0x80000001, 0x80000002, 0x80000003],
    )
    .unwrap();
    assert_eq!(character.possessions.len(), 3);
    assert_eq!(character.account, 7);
    assert_eq!(character.state.weenie_type, assets.human.weenie_type);
    let frozen = bace_runtime::character_creation::freeze_creation(character).unwrap();
    assert_eq!(frozen.player.account_id, 7);
    assert_eq!(frozen.player.metadata.hair_texture, 0x05000002);
    assert_eq!(frozen.items.len(), 3);
}
#[test]
fn missing_supplements_palettes_templates_clothing_and_starts_fail_explicitly() {
    let mut input = Inputs::new();
    assert!(matches!(
        prepare_creation_assets(
            &input.base,
            1,
            CreationSupplementary {
                skill_gear: None,
                ..input.supplement()
            }
        ),
        Err(CreationPreparationError::Supplement("starter gear"))
    ));
    input.palettes.remove(&0x0f000002);
    assert!(matches!(
        prepare_creation_assets(&input.base, 1, input.supplement()),
        Err(CreationPreparationError::Palette(0x0f000002))
    ));
    input = Inputs::new();
    input.items.remove(&11);
    assert!(matches!(
        prepare_creation_assets(&input.base, 1, input.supplement()),
        Err(CreationPreparationError::Template(11))
    ));
    input = Inputs::new();
    input.clothing.remove(&0x1000000b);
    assert!(matches!(
        prepare_creation_assets(&input.base, 1, input.supplement()),
        Err(CreationPreparationError::Clothing { template: 11, .. })
    ));
    input = Inputs::new();
    input.starts[0].area = 999;
    assert!(matches!(
        prepare_creation_assets(&input.base, 1, input.supplement()),
        Err(CreationPreparationError::Start(999))
    ));
}
#[test]
fn account_spoofing_truncated_skill_lists_and_negative_template_are_rejected() {
    let account = account();
    let mut wire = request();
    wire.account = "other".into();
    assert!(matches!(
        creation_request(&wire, &account, 0x50000001),
        Err(CreationPreparationError::Identity)
    ));
    wire.account = "creator".into();
    wire.skill_advancement_classes.pop();
    assert!(matches!(
        creation_request(&wire, &account, 0x50000001),
        Err(CreationPreparationError::SkillCount(54))
    ));
    wire.skill_advancement_classes.push(9);
    assert!(matches!(
        creation_request(&wire, &account, 0x50000001),
        Err(CreationPreparationError::Skill { slot: 54, value: 9 })
    ));
    wire.skill_advancement_classes[54] = 0;
    wire.template_option = -1;
    assert!(matches!(
        creation_request(&wire, &account, 0x50000001),
        Err(CreationPreparationError::TemplateIndex)
    ));
}
