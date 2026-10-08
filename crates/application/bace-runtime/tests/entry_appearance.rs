use bace_content::{Property, SparseProperties, WeenieV1};
use bace_dat::{
    CharGen, ClothingPalette, ClothingPart, ClothingTable, ClothingTemplate, DatPaletteSet,
    ModelSetup,
};
use bace_runtime::player_entry::{
    EntryAppearanceAssets, PlayerAppearanceOptions, prepare_item_model, prepare_player_model,
};
use std::collections::BTreeMap;
mod treasure_table_support;
fn source() -> WeenieV1 {
    WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "fixture".into(),
        weenie_type: 1,
        last_modified: None,
        properties: SparseProperties::default(),
    }
}
fn id(kind: &str, name: &str) -> u32 {
    bace_loot::ace_tables::enum_value(kind, name).unwrap() as u32
}
fn int(w: &mut WeenieV1, name: &str, value: i32) {
    let id = id("PropertyInt", name);
    w.properties.ints.retain(|p| p.id != id);
    w.properties.ints.push(Property { id, value });
}
fn did(w: &mut WeenieV1, name: &str, value: u32) {
    let id = id("PropertyDataId", name);
    w.properties.data_ids.retain(|p| p.id != id);
    w.properties.data_ids.push(Property { id, value });
}
fn float(w: &mut WeenieV1, name: &str, value: f64) {
    let id = id("PropertyFloat", name);
    w.properties.floats.retain(|p| p.id != id);
    w.properties.floats.push(Property { id, value });
}
fn boolean(w: &mut WeenieV1, name: &str, value: bool) {
    w.properties.bools.push(Property {
        id: id("PropertyBool", name),
        value,
    });
}
fn gear(clo: u32, loc: i32, kind: i32, priority: i32) -> WeenieV1 {
    let mut w = source();
    did(&mut w, "Setup", 0x02000001);
    did(&mut w, "ClothingBase", clo);
    int(&mut w, "CurrentWieldedLocation", loc);
    int(&mut w, "ItemType", kind);
    int(&mut w, "ClothingPriority", priority);
    int(&mut w, "PaletteTemplate", 999);
    float(&mut w, "Shade", 0.6);
    w
}
fn table(id: u32, model: u32, index: u32) -> ClothingTable {
    ClothingTable {
        id,
        setups: BTreeMap::from([(
            0x02000001,
            vec![ClothingPart {
                index,
                model,
                textures: vec![(0x05000011, model + 0x04000000)],
            }],
        )]),
        templates: BTreeMap::from([
            (
                7,
                ClothingTemplate {
                    icon: 0x06000077,
                    palettes: vec![ClothingPalette {
                        palette_set: 0x0f000001,
                        ranges: vec![(64, 16)],
                    }],
                },
            ),
            (
                1,
                ClothingTemplate {
                    icon: 0x06000011,
                    palettes: vec![],
                },
            ),
        ]),
        template_order: vec![7, 1],
    }
}
fn setup(id: u32, parts: Vec<u32>) -> ModelSetup {
    ModelSetup {
        id,
        parts,
        flags: 0,
        parents: vec![],
        scales: vec![],
        placements: BTreeMap::new(),
        default_animation: 0,
        default_motion: 0,
    }
}
#[test]
fn original_csharp_player_and_item_appearance() {
    treasure_table_support::install_tables();
    let chargen = body_style_chargen();
    let parts = (1..=17).map(|v| 0x01000000 + v).collect::<Vec<_>>();
    let setups = BTreeMap::from([
        (0x02000001, setup(0x02000001, parts.clone())),
        (0x02001aa3, setup(0x02001aa3, parts)),
        (0x02000002, setup(0x02000002, vec![0x01000099, 0x01000098])),
    ]);
    let clothing = BTreeMap::from([
        (0x10000001, table(0x10000001, 0x01000081, 9)),
        (0x10000002, table(0x10000002, 0x01000082, 16)),
        (0x10000003, table(0x10000003, 0x01000083, 9)),
    ]);
    let palettes = BTreeMap::from([(
        0x0f000001,
        DatPaletteSet {
            id: 0x0f000001,
            palettes: vec![0x04000012, 0x04000034, 0x04000056],
        },
    )]);
    let assets = EntryAppearanceAssets {
        chargen: &chargen,
        setups: &setups,
        clothing: &clothing,
        palettes: &palettes,
    };
    for line in include_str!("fixtures/entry_appearance.txt")
        .lines()
        .filter(|v| !v.starts_with('#'))
    {
        let fields = line.split('|').collect::<Vec<_>>();
        let case = fields[0].parse::<u32>().unwrap();
        let mut w = source();
        let mut equipment = vec![];
        let options = PlayerAppearanceOptions {
            show_helm: case != 3,
            show_cloak: case != 7,
            default_hair_texture: 0x05000101,
            hair_texture: 0x05000102,
        };
        let output = if case < 16 {
            int(&mut w, "HeritageGroup", if case == 14 { 6 } else { 1 });
            if case == 14 {
                int(&mut w, "Hairstyle", 0);
            }
            int(&mut w, "Gender", 1);
            did(
                &mut w,
                "Setup",
                if case == 9 { 0x02001aa3 } else { 0x02000001 },
            );
            for (name, value) in [
                ("HeadObject", 0x01000070),
                ("PaletteBase", 0x04000001),
                ("HairPalette", 0x04000002),
                ("SkinPalette", 0x04000003),
                ("EyesPalette", 0x04000004),
                ("DefaultEyesTexture", 0x05000001),
                ("EyesTexture", 0x05000002),
                ("DefaultNoseTexture", 0x05000003),
                ("NoseTexture", 0x05000004),
                ("DefaultMouthTexture", 0x05000005),
                ("MouthTexture", 0x05000006),
            ] {
                did(&mut w, name, value);
            }
            if case == 1 {
                w.properties
                    .animation_parts
                    .push(bace_content::AnimationPart {
                        index: 2,
                        animation_id: 0x010000aa,
                    });
                w.properties.palettes.push(bace_content::Palette {
                    sub_palette_id: 0x04000055,
                    offset: 2,
                    length: 3,
                });
            }
            if case >= 2 && case != 14 {
                equipment.push(gear(0x10000001, 0x200, 2, 4));
                equipment.push(gear(0x10000002, 1, 4, 1));
            }
            if (4..=8).contains(&case) {
                equipment.push(gear(0x10000003, 0x08000000, 4, 8));
                if case == 5 || case == 6 {
                    boolean(&mut equipment[0], "TopLayerPriority", case == 5);
                }
            }
            if case == 8 {
                int(&mut equipment[0], "PaletteTemplate", 1);
            }
            if case == 10 {
                equipment[0].properties.data_ids.retain(|p| p.id != 7);
                did(&mut equipment[0], "Setup", 0x02000002);
            }
            if (11..=13).contains(&case) {
                float(
                    &mut equipment[0],
                    "Shade",
                    match case {
                        11 => 1.,
                        12 => -1.,
                        _ => 0.,
                    },
                );
            }
            if case == 15 {
                equipment.clear();
                did(&mut w, "ClothingBase", 0x10000001);
                float(&mut w, "Shade", 0.6);
            }
            prepare_player_model(&w, &equipment.iter().collect::<Vec<_>>(), options, &assets)
                .unwrap()
        } else {
            w = gear(0x10000001, 0x200, 2, 4);
            did(&mut w, "PaletteBase", 0x04000001);
            if case == 17 {
                boolean(&mut w, "IgnoreCloIcons", true);
            }
            if case == 18 {
                w.properties.floats.clear();
                w.properties
                    .ints
                    .retain(|p| p.id != id("PropertyInt", "PaletteTemplate"));
            }
            if case == 19 {
                did(&mut w, "Setup", 0x02000002);
            }
            prepare_item_model(&w, &assets).unwrap()
        };
        let actual = format!(
            "{}|{}|{}|{}|{}|{}",
            case,
            output.icon_override.unwrap_or(0),
            output.model.palette_id.unwrap_or(0),
            output
                .model
                .parts
                .iter()
                .map(|v| format!("{},{}", v.part_index, v.animation_id))
                .collect::<Vec<_>>()
                .join(";"),
            output
                .model
                .textures
                .iter()
                .map(|v| format!("{},{},{}", v.part_index, v.old_texture, v.new_texture))
                .collect::<Vec<_>>()
                .join(";"),
            output
                .model
                .palettes
                .iter()
                .map(|v| format!("{},{},{}", v.palette_id, v.offset, v.length))
                .collect::<Vec<_>>()
                .join(";")
        );
        // No palette ID is serialized when the subpalette count is zero.
        let mut expected = fields.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        if expected[5].is_empty() {
            expected[2] = "0".into();
        }
        assert_eq!(actual, expected.join("|"), "original C# case {case}");
    }
}
#[test]
fn missing_assets_and_byte_count_overflow_reject_entry() {
    treasure_table_support::install_tables();
    let chargen = CharGen {
        reserved: 0,
        starter_areas: vec![],
        heritage_marker: 0,
        heritage_groups: BTreeMap::new(),
    };
    let setups = BTreeMap::new();
    let clothing = BTreeMap::new();
    let palettes = BTreeMap::new();
    let assets = EntryAppearanceAssets {
        chargen: &chargen,
        setups: &setups,
        clothing: &clothing,
        palettes: &palettes,
    };
    let mut w = gear(0x10000001, 1, 4, 1);
    assert!(prepare_item_model(&w, &assets).is_err());
    w.properties.data_ids.clear();
    w.properties.palettes = vec![bace_content::Palette::default(); 256];
    did(&mut w, "Setup", 0x02000001);
    assert!(
        prepare_player_model(
            &w,
            &[],
            PlayerAppearanceOptions {
                show_helm: true,
                show_cloak: true,
                default_hair_texture: 0,
                hair_texture: 0
            },
            &assets
        )
        .is_err()
    );
}

fn body_style_chargen() -> CharGen {
    let mut appearance = bace_dat::ObjectDescription {
        marker: 0,
        palette_id: None,
        sub_palettes: vec![],
        texture_changes: vec![],
        animation_parts: vec![],
    };
    appearance.texture_changes.push(bace_dat::TextureChange {
        part: 16,
        old_texture: 0x050000dd,
        new_texture: 0x050000ee,
    });
    appearance.animation_parts.push(bace_dat::AnimationPart {
        part: 16,
        id: 0x010000dd,
    });
    let gender = bace_dat::CreationGender {
        name: String::new(),
        scale: 0,
        setup: 0,
        sound_table: 0,
        icon: 0,
        base_palette: 0,
        skin_palette_set: 0,
        physics_table: 0,
        motion_table: 0,
        combat_table: 0,
        appearance: appearance.clone(),
        hair_colors: vec![],
        hair_styles: vec![bace_dat::HairStyle {
            icon: 0,
            bald: false,
            alternate_setup: 0,
            appearance,
        }],
        eye_colors: vec![],
        eyes: vec![],
        noses: vec![],
        mouths: vec![],
        headgear: vec![],
        shirts: vec![],
        pants: vec![],
        footwear: vec![],
        clothing_colors: vec![],
    };
    let heritage = bace_dat::HeritageGroup {
        name: String::new(),
        icon: 0,
        setup: 0,
        environment_setup: 0,
        attribute_credits: 0,
        skill_credits: 0,
        primary_start_areas: vec![],
        secondary_start_areas: vec![],
        skills: vec![],
        templates: vec![],
        gender_marker: 0,
        genders: BTreeMap::from([(1, gender)]),
    };
    CharGen {
        reserved: 0,
        starter_areas: vec![],
        heritage_marker: 0,
        heritage_groups: BTreeMap::from([(6, heritage)]),
    }
}

#[test]
fn npc_gear_uses_source_palettes_without_player_hair_override() {
    treasure_table_support::install_tables();
    let chargen = body_style_chargen();
    let setups = BTreeMap::from([(
        0x02000001,
        setup(0x02000001, (1..=17).map(|v| 0x01000000 + v).collect()),
    )]);
    let clothing = BTreeMap::from([(0x10000001, table(0x10000001, 0x01000081, 9))]);
    let palettes = BTreeMap::from([(
        0x0f000001,
        DatPaletteSet {
            id: 0x0f000001,
            palettes: vec![0x04000012, 0x04000034, 0x04000056],
        },
    )]);
    let assets = EntryAppearanceAssets {
        chargen: &chargen,
        setups: &setups,
        clothing: &clothing,
        palettes: &palettes,
    };
    let mut creature = source();
    creature.weenie_type = 10;
    did(&mut creature, "Setup", 0x02000001);
    did(&mut creature, "SkinPalette", 0x04000022);
    let armor = gear(0x10000001, 0x200, 2, 1);
    let npc =
        bace_runtime::player_entry::prepare_creature_model(&creature, &[&armor], &assets).unwrap();
    assert!(
        npc.model
            .textures
            .iter()
            .all(|t| t.old_texture != 0 && t.new_texture != 0)
    );
    assert!(
        npc.model
            .parts
            .iter()
            .any(|part| part.part_index == 9 && part.animation_id == 0x01000081)
    );
    assert!(
        npc.model
            .palettes
            .iter()
            .any(|p| p.palette_id == 0x04000022)
    );
    let player = prepare_player_model(
        &creature,
        &[&armor],
        PlayerAppearanceOptions {
            show_helm: true,
            show_cloak: true,
            default_hair_texture: 0x05000021,
            hair_texture: 0x05000022,
        },
        &assets,
    )
    .unwrap();
    let mut expected = player.model;
    expected.textures.retain(|t| t.old_texture != 0x05000021);
    assert_eq!(npc.model, expected);
}
