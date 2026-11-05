use super::*;
use bace_dat::{AnimationPart, EyeStrip, FaceStrip, HairStyle, HeritageGroup, TextureChange};

fn did(source: &WeenieV1, name: &str) -> Option<u32> {
    let id = key("PropertyDataId", name)?;
    source
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.value)
}

fn description(old: u32, new: u32) -> ObjectDescription {
    ObjectDescription {
        marker: 0,
        palette_id: None,
        sub_palettes: vec![],
        texture_changes: vec![TextureChange {
            part: 16,
            old_texture: old,
            new_texture: new,
        }],
        animation_parts: vec![],
    }
}
fn fixture() -> (WeenieV1, CharGen, BTreeMap<u32, DatPaletteSet>) {
    let mut source = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "human_npc".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.strings.push(Property {
        id: key("PropertyString", "HeritageGroup").unwrap(),
        value: "Gharu'ndim".into(),
    });
    source.properties.strings.push(Property {
        id: key("PropertyString", "Sex").unwrap(),
        value: "Male".into(),
    });
    let style = |id| HairStyle {
        icon: 0,
        bald: false,
        alternate_setup: 0,
        appearance: ObjectDescription {
            animation_parts: vec![AnimationPart { part: 16, id }],
            ..description(0x05000001, 0x05000002)
        },
    };
    let sex = CreationGender {
        name: "Male".into(),
        scale: 100,
        setup: 0x02000001,
        sound_table: 0,
        icon: 0,
        base_palette: 0x04000010,
        skin_palette_set: 0x0f000010,
        physics_table: 0,
        motion_table: 0,
        combat_table: 0,
        appearance: description(1, 2),
        hair_colors: vec![0x0f000011, 0x0f000012],
        hair_styles: (0..12).map(|i| style(0x01000001 + i)).collect(),
        eye_colors: vec![0x04000030, 0x04000031],
        eyes: vec![EyeStrip {
            icon: 0,
            bald_icon: 0,
            appearance: description(0x05000010, 0x05000011),
            bald_appearance: description(0x05000012, 0x05000013),
        }],
        noses: vec![FaceStrip {
            icon: 0,
            appearance: description(0x05000020, 0x05000021),
        }],
        mouths: vec![FaceStrip {
            icon: 0,
            appearance: description(0x05000030, 0x05000031),
        }],
        headgear: vec![],
        shirts: vec![],
        pants: vec![],
        footwear: vec![],
        clothing_colors: vec![],
    };
    let group = HeritageGroup {
        name: "Gharundim".into(),
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
        genders: BTreeMap::from([(1, sex)]),
    };
    let chargen = CharGen {
        reserved: 0,
        starter_areas: vec![],
        heritage_marker: 0,
        heritage_groups: BTreeMap::from([(2, group)]),
    };
    let palettes = [
        (0x0f000010, vec![0x04000040, 0x04000041]),
        (0x0f000011, vec![0x04000050, 0x04000051]),
        (0x0f000012, vec![0x04000052, 0x04000053]),
    ]
    .into_iter()
    .map(|(id, palettes)| (id, DatPaletteSet { id, palettes }))
    .collect();
    (source, chargen, palettes)
}

#[test]
fn pinned_ace_face_selection_uses_dat_bounds_and_preserves_authored_fields() {
    let (mut source, chargen, palettes) = fixture();
    let first = generate(&source, 0x8000_0001, 1, &chargen, &palettes)
        .unwrap()
        .unwrap();
    assert_eq!(int(&first, "HeritageGroup"), Some(2));
    assert_eq!(int(&first, "Gender"), Some(1));
    assert_eq!(did(&first, "PaletteBase"), Some(0x04000010));
    assert_eq!(did(&first, "DefaultEyesTexture"), Some(0x05000010));
    assert_eq!(did(&first, "EyesTexture"), Some(0x05000011));
    assert_eq!(did(&first, "DefaultNoseTexture"), Some(0x05000020));
    assert_eq!(did(&first, "MouthTexture"), Some(0x05000031));
    // Golden selection for this instance seed. The values exercise ACE's
    // inclusive 0..8 retail hair bound and PaletteSet shade lookup.
    assert_eq!(did(&first, "HeadObject"), Some(0x01000005));
    assert_eq!(did(&first, "HairPalette"), Some(0x04000052));
    assert_eq!(did(&first, "EyesPalette"), Some(0x04000030));
    assert_eq!(did(&first, "SkinPalette"), Some(0x04000041));
    assert_eq!(
        generate(&source, 0x8000_0001, 1, &chargen, &palettes)
            .unwrap()
            .unwrap(),
        first
    );

    source.properties.data_ids.push(Property {
        id: key("PropertyDataId", "HeadObject").unwrap(),
        value: 0x0100ffff,
    });
    source.properties.data_ids.push(Property {
        id: key("PropertyDataId", "PaletteBase").unwrap(),
        value: 0x0400ffff,
    });
    let authored = generate(&source, 0x8000_0001, 1, &chargen, &palettes)
        .unwrap()
        .unwrap();
    assert_eq!(did(&authored, "HeadObject"), Some(0x0100ffff));
    assert_eq!(did(&authored, "PaletteBase"), Some(0x04000010));
    source.properties.strings.clear();
    assert!(
        generate(&source, 0x8000_0001, 1, &chargen, &palettes)
            .unwrap()
            .is_none()
    );
}

#[test]
#[ignore = "requires approved retail DATs; set BACE_DAT_DIRECTORY"]
fn real_human_npc_face_uses_approved_portal_tables() {
    let directory = std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let mut portal = bace_dat::DatArchive::open_verified(
        directory.join("client_portal.dat"),
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4",
    )
    .unwrap();
    let chargen = CharGen::decode(&portal.read(CharGen::RECORD_ID).unwrap()).unwrap();
    let (source, _, _) = fixture();
    let mut palettes = BTreeMap::new();
    for id in palette_sets(&source, &chargen) {
        let value = DatPaletteSet::decode(&portal.read(id).unwrap()).unwrap();
        assert_eq!(value.id, id);
        palettes.insert(id, value);
    }
    let face = generate(&source, 0x8000_0001, 1, &chargen, &palettes)
        .unwrap()
        .unwrap();
    for property in [
        "HeadObject",
        "EyesTexture",
        "NoseTexture",
        "MouthTexture",
        "SkinPalette",
        "HairPalette",
    ] {
        assert!(did(&face, property).is_some_and(|id| id != 0), "{property}");
    }
}
