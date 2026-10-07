use bace_dat::{CharGen, CreationGender, ObjectDescription, SkillTable, XpTable};
use serde_json::Value;

fn fixture() -> Value {
    let value: Value = serde_json::from_str(include_str!("../fixtures/dat.json")).unwrap();
    assert_eq!(value["commit"], bace_compat::ACE_COMMIT);
    assert_eq!(value["repository"], bace_compat::ACE_REPOSITORY);
    value
}
fn hex(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
fn words(value: &Value) -> Vec<u32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u32)
        .collect()
}
fn numbers(actual: &[u32], expected: &Value, keys: &[&str]) {
    assert_eq!(actual.len(), keys.len());
    for (actual, key) in actual.iter().zip(keys) {
        assert_eq!(u64::from(*actual), expected[key].as_u64().unwrap(), "{key}");
    }
}
#[test]
fn official_xp_counts_are_inclusive_and_level_xp_preserves_64_bits() {
    for vector in fixture()["vectors"]["xp"].as_array().unwrap() {
        let bytes = hex(&vector["bytes"]);
        let table = XpTable::decode(&bytes).unwrap();
        let expected = &vector["decoded"];
        assert_eq!(table.attribute_xp, words(&expected["AttributeXpList"]));
        assert_eq!(table.vital_xp, words(&expected["VitalXpList"]));
        assert_eq!(
            table.trained_skill_xp,
            words(&expected["TrainedSkillXpList"])
        );
        assert_eq!(
            table.specialized_skill_xp,
            words(&expected["SpecializedSkillXpList"])
        );
        assert_eq!(
            table.character_level_skill_credits,
            words(&expected["CharacterLevelSkillCreditList"])
        );
        assert_eq!(
            table.character_level_xp,
            expected["CharacterLevelXPList"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap())
                .collect::<Vec<_>>()
        );
        for end in 0..bytes.len() {
            assert!(XpTable::decode(&bytes[..end]).is_err());
        }
    }
}
#[test]
fn official_skills_preserve_utf8_alignment_signed_costs_and_all_formula_words() {
    let fixture = fixture();
    let vector = &fixture["vectors"]["skills"];
    let bytes = hex(&vector["bytes"]);
    let table = SkillTable::decode(&bytes).unwrap();
    let expected = vector["decoded"]["SkillBaseHash"].as_object().unwrap();
    assert_eq!(table.skills.len(), expected.len());
    for (id, actual) in &table.skills {
        let expected = &expected[&id.to_string()];
        assert_eq!(
            actual.description,
            expected["Description"].as_str().unwrap()
        );
        assert_eq!(actual.name, expected["Name"].as_str().unwrap());
        assert_eq!(
            i64::from(actual.trained_cost),
            expected["TrainedCost"].as_i64().unwrap()
        );
        assert_eq!(
            i64::from(actual.specialized_cost),
            expected["SpecializedCost"].as_i64().unwrap()
        );
        numbers(
            &[
                actual.icon_id,
                actual.category,
                actual.chargen_use,
                actual.min_level,
            ],
            expected,
            &["IconId", "Category", "ChargenUse", "MinLevel"],
        );
        numbers(
            &[
                actual.formula.w,
                actual.formula.x,
                actual.formula.y,
                actual.formula.z,
                actual.formula.attribute1,
                actual.formula.attribute2,
            ],
            &expected["Formula"],
            &["W", "X", "Y", "Z", "Attr1", "Attr2"],
        );
        assert_eq!(actual.upper_bound, expected["UpperBound"].as_f64().unwrap());
        assert_eq!(actual.lower_bound, expected["LowerBound"].as_f64().unwrap());
        assert_eq!(
            actual.learn_modifier,
            expected["LearnMod"].as_f64().unwrap()
        );
    }
    for end in 0..bytes.len() {
        assert!(SkillTable::decode(&bytes[..end]).is_err());
    }
}
fn appearance(actual: &ObjectDescription, expected: &Value) {
    assert_eq!(
        u64::from(actual.palette_id.unwrap_or(0)),
        expected["PaletteID"].as_u64().unwrap()
    );
    assert_eq!(
        actual.sub_palettes.len(),
        expected["SubPalettes"].as_array().unwrap().len()
    );
    for (a, e) in actual
        .sub_palettes
        .iter()
        .zip(expected["SubPalettes"].as_array().unwrap())
    {
        numbers(
            &[a.id, a.offset, a.colors],
            e,
            &["SubID", "Offset", "NumColors"],
        );
    }
    assert_eq!(
        actual.texture_changes.len(),
        expected["TextureChanges"].as_array().unwrap().len()
    );
    for (a, e) in actual
        .texture_changes
        .iter()
        .zip(expected["TextureChanges"].as_array().unwrap())
    {
        numbers(
            &[a.part.into(), a.old_texture, a.new_texture],
            e,
            &["PartIndex", "OldTexture", "NewTexture"],
        );
    }
    assert_eq!(
        actual.animation_parts.len(),
        expected["AnimPartChanges"].as_array().unwrap().len()
    );
    for (a, e) in actual
        .animation_parts
        .iter()
        .zip(expected["AnimPartChanges"].as_array().unwrap())
    {
        numbers(&[a.part.into(), a.id], e, &["PartIndex", "PartID"]);
    }
}
fn gender(actual: &CreationGender, expected: &Value) {
    assert_eq!(actual.name, expected["Name"].as_str().unwrap());
    numbers(
        &[
            actual.scale,
            actual.setup,
            actual.sound_table,
            actual.icon,
            actual.base_palette,
            actual.skin_palette_set,
            actual.physics_table,
            actual.motion_table,
            actual.combat_table,
        ],
        expected,
        &[
            "Scale",
            "SetupID",
            "SoundTable",
            "IconImage",
            "BasePalette",
            "SkinPalSet",
            "PhysicsTable",
            "MotionTable",
            "CombatTable",
        ],
    );
    appearance(&actual.appearance, &expected["BaseObjDesc"]);
    assert_eq!(actual.hair_colors, words(&expected["HairColorList"]));
    assert_eq!(actual.eye_colors, words(&expected["EyeColorList"]));
    assert_eq!(
        actual.clothing_colors,
        words(&expected["ClothingColorsList"])
    );
    assert_eq!(
        actual.hair_styles.len(),
        expected["HairStyleList"].as_array().unwrap().len()
    );
    for (a, e) in actual
        .hair_styles
        .iter()
        .zip(expected["HairStyleList"].as_array().unwrap())
    {
        numbers(
            &[a.icon, a.alternate_setup],
            e,
            &["IconImage", "AlternateSetup"],
        );
        assert_eq!(a.bald, e["Bald"].as_bool().unwrap());
        appearance(&a.appearance, &e["ObjDesc"]);
    }
    assert_eq!(
        actual.eyes.len(),
        expected["EyeStripList"].as_array().unwrap().len()
    );
    for (a, e) in actual
        .eyes
        .iter()
        .zip(expected["EyeStripList"].as_array().unwrap())
    {
        numbers(&[a.icon, a.bald_icon], e, &["IconImage", "IconImageBald"]);
        appearance(&a.appearance, &e["ObjDesc"]);
        appearance(&a.bald_appearance, &e["ObjDescBald"]);
    }
    for (list, key) in [
        (&actual.noses, "NoseStripList"),
        (&actual.mouths, "MouthStripList"),
    ] {
        let expected = expected[key].as_array().unwrap();
        assert_eq!(list.len(), expected.len());
        for (a, e) in list.iter().zip(expected) {
            numbers(&[a.icon], e, &["IconImage"]);
            appearance(&a.appearance, &e["ObjDesc"]);
        }
    }
    for (list, key) in [
        (&actual.headgear, "HeadgearList"),
        (&actual.shirts, "ShirtList"),
        (&actual.pants, "PantsList"),
        (&actual.footwear, "FootwearList"),
    ] {
        let expected = expected[key].as_array().unwrap();
        assert_eq!(list.len(), expected.len());
        for (a, e) in list.iter().zip(expected) {
            assert_eq!(a.name, e["Name"].as_str().unwrap());
            numbers(
                &[a.clothing_table, a.weenie],
                e,
                &["ClothingTable", "WeenieDefault"],
            );
        }
    }
}
#[test]
fn official_chargen_preserves_heritage_rules_templates_and_nested_appearance() {
    for vector in fixture()["vectors"]["chargen"].as_array().unwrap() {
        let bytes = hex(&vector["bytes"]);
        let actual = CharGen::decode(&bytes).unwrap();
        let expected = &vector["decoded"];
        assert_eq!(
            actual.starter_areas.len(),
            expected["StarterAreas"].as_array().unwrap().len()
        );
        for (a, e) in actual
            .starter_areas
            .iter()
            .zip(expected["StarterAreas"].as_array().unwrap())
        {
            assert_eq!(a.name, e["Name"].as_str().unwrap());
            assert_eq!(a.locations.len(), e["Locations"].as_array().unwrap().len());
            for (a, e) in a.locations.iter().zip(e["Locations"].as_array().unwrap()) {
                numbers(&[a.cell], e, &["ObjCellID"]);
                for (value, key) in a.origin.iter().zip(["X", "Y", "Z"]) {
                    assert_eq!(
                        f64::from(*value),
                        e["Frame"]["Origin"][key].as_f64().unwrap()
                    );
                }
                for (value, key) in a.orientation_wxyz.iter().zip(["W", "X", "Y", "Z"]) {
                    assert_eq!(
                        f64::from(*value),
                        e["Frame"]["Orientation"][key].as_f64().unwrap()
                    );
                }
            }
        }
        let groups = expected["HeritageGroups"].as_object().unwrap();
        assert_eq!(actual.heritage_groups.len(), groups.len());
        for (id, a) in &actual.heritage_groups {
            let e = &groups[&id.to_string()];
            assert_eq!(a.name, e["Name"].as_str().unwrap());
            numbers(
                &[
                    a.icon,
                    a.setup,
                    a.environment_setup,
                    a.attribute_credits,
                    a.skill_credits,
                ],
                e,
                &[
                    "IconImage",
                    "SetupID",
                    "EnvironmentSetupID",
                    "AttributeCredits",
                    "SkillCredits",
                ],
            );
            assert_eq!(
                a.primary_start_areas,
                e["PrimaryStartAreas"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_i64().unwrap() as i32)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                a.secondary_start_areas,
                e["SecondaryStartAreas"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_i64().unwrap() as i32)
                    .collect::<Vec<_>>()
            );
            assert_eq!(a.skills.len(), e["Skills"].as_array().unwrap().len());
            for (a, e) in a.skills.iter().zip(e["Skills"].as_array().unwrap()) {
                numbers(&[a.skill], e, &["SkillNum"]);
                assert_eq!(i64::from(a.normal_cost), e["NormalCost"].as_i64().unwrap());
                assert_eq!(
                    i64::from(a.primary_cost),
                    e["PrimaryCost"].as_i64().unwrap()
                );
            }
            assert_eq!(a.templates.len(), e["Templates"].as_array().unwrap().len());
            for (a, e) in a.templates.iter().zip(e["Templates"].as_array().unwrap()) {
                assert_eq!(a.name, e["Name"].as_str().unwrap());
                numbers(&[a.icon, a.title], e, &["IconImage", "Title"]);
                numbers(
                    &a.attributes,
                    e,
                    &[
                        "Strength",
                        "Endurance",
                        "Coordination",
                        "Quickness",
                        "Focus",
                        "Self",
                    ],
                );
                assert_eq!(a.normal_skills, words(&e["NormalSkillsList"]));
                assert_eq!(a.primary_skills, words(&e["PrimarySkillsList"]));
            }
            let genders = e["Genders"].as_object().unwrap();
            assert_eq!(a.genders.len(), genders.len());
            for (id, a) in &a.genders {
                gender(a, &genders[&id.to_string()]);
            }
        }
        for end in 0..bytes.len() {
            assert!(CharGen::decode(&bytes[..end]).is_err());
        }
    }
}

#[test]
fn synthetic_official_records_reject_nonfinite_and_unbounded_mutations() {
    let fixture = fixture();
    let mut skill = hex(&fixture["vectors"]["skills"]["bytes"]);
    let length = skill.len();
    skill[length - 8..].copy_from_slice(&f64::NAN.to_bits().to_le_bytes());
    assert!(SkillTable::decode(&skill).is_err());
    let mut skill = hex(&fixture["vectors"]["skills"]["bytes"]);
    skill[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(SkillTable::decode(&skill).is_err());
    let mut chargen = hex(&fixture["vectors"]["chargen"][1]["bytes"]);
    // At offset 8 is the SmartArray count for starting areas.
    chargen[8..12].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    assert!(CharGen::decode(&chargen).is_err());
    let mut chargen = hex(&fixture["vectors"]["chargen"][1]["bytes"]);
    // The first area's BinaryReader string begins after that one-byte count.
    chargen[9..14].copy_from_slice(&[0xff, 0xff, 0xff, 0xff, 0xff]);
    assert!(CharGen::decode(&chargen).is_err());
}
