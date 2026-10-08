use crate::preview_scene::{PreviewRequest, load};

#[test]
fn compressed_textures_handle_transparency_and_short_blocks() {
    // DXT1: black/red endpoints with selector 3 means transparent when c0 <= c1.
    let mut t = bace_dat::DatTexture {
        id: 0x06000001,
        width: 4,
        height: 4,
        format: 827611204,
        bytes: vec![0, 0, 0, 248, 255, 255, 255, 255],
        palette: None,
    };
    assert!(
        crate::preview_pixels::pixels(&t, &[])
            .unwrap()
            .iter()
            .all(|p| p[3] == 0)
    );
    t.bytes[0..4].copy_from_slice(&[0, 248, 0, 0]);
    t.bytes[4..].fill(0);
    assert!(
        crate::preview_pixels::pixels(&t, &[])
            .unwrap()
            .iter()
            .all(|p| *p == [255, 0, 0, 255])
    );
    t.bytes.pop();
    assert!(crate::preview_pixels::pixels(&t, &[]).is_err());
    // DXT5: alpha selector 7 uses the final endpoint 255 in the six-alpha mode.
    t.format = 894720068;
    t.bytes = vec![
        0, 255, 255, 255, 255, 255, 255, 255, 0, 248, 0, 0, 0, 0, 0, 0,
    ];
    assert!(
        crate::preview_pixels::pixels(&t, &[])
            .unwrap()
            .iter()
            .all(|p| *p == [255, 0, 0, 255])
    );
    t.width = 3;
    t.height = 2;
    assert_eq!(crate::preview_pixels::pixels(&t, &[]).unwrap().len(), 6);
}

#[test]
fn indexed_textures_reject_missing_colors_and_wrong_payload_sizes() {
    let mut t = bace_dat::DatTexture {
        id: 0x06000001,
        width: 1,
        height: 1,
        format: 101,
        bytes: vec![0, 1],
        palette: Some(0x04000001),
    };
    assert!(crate::preview_pixels::pixels(&t, &[0; 256]).is_err());
    let mut palette = vec![0; 257];
    palette[256] = 0xff123456;
    assert_eq!(
        crate::preview_pixels::pixels(&t, &palette).unwrap(),
        vec![[0x12, 0x34, 0x56, 255]]
    );
    t.bytes.push(0);
    assert!(crate::preview_pixels::pixels(&t, &palette).is_err());
}

#[test]
fn authoring_formula_rounds_halves_and_rejects_missing_inputs() {
    let weenie=bace_content_tools::parse("schema_version=1\nweenie_id=1\nclass_name='test'\nweenie_type=1\n[[properties.attributes]]\nid=2\n[properties.attributes.value]\ninit_level=101\nlevel_from_cp=0\ncp_spent=0\n").unwrap();
    let formula = bace_dat::SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z: 2,
        attribute1: 2,
        attribute2: 0,
    };
    assert_eq!(
        crate::authoring_helpers::attribute_bonus(&weenie, formula).unwrap(),
        51
    );
    assert!(
        crate::authoring_helpers::attribute_bonus(
            &weenie,
            bace_dat::SkillFormula { z: 0, ..formula }
        )
        .is_err()
    );
    assert!(
        crate::authoring_helpers::attribute_bonus(
            &weenie,
            bace_dat::SkillFormula {
                attribute2: 1,
                ..formula
            }
        )
        .is_err()
    );
    assert_eq!(crate::authoring_helpers::spell_probability(2.0), 0.02);
    assert_eq!(crate::authoring_helpers::spell_probability(2.25), 0.25);
    assert_eq!(crate::authoring_helpers::spell_probability(3.0), 1.0);
}

#[test]
#[ignore = "Requires user-supplied client DAT; no proprietary assets committed"]
fn supplied_model_preview() {
    let directory = std::env::var_os("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY");
    {
        let setup = 0x02000001;
        let scene = load(PreviewRequest {
            path: std::path::PathBuf::from(&directory).join("client_portal.dat"),
            setup,
            ..Default::default()
        })
        .expect("real Setup resolves");
        assert!(scene.triangles.len() > 100);
        assert!(!scene.textures.is_empty());
        let image = crate::preview_render::render(&scene, Default::default()).expect("render");
        assert!(
            image
                .pixels
                .iter()
                .filter(|p| **p != eframe::egui::Color32::from_rgb(13, 19, 27))
                .count()
                > 1000
        );
        eprintln!(
            "Setup {setup:08X}: {} triangles / {} textures",
            scene.triangles.len(),
            scene.textures.len()
        );
    }
}

#[test]
#[ignore = "Requires user-supplied Portal DAT; no proprietary assets committed"]
fn supplied_animation_pose_preview() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("BACE_DAT_DIRECTORY"),
    );
    let path = directory.join("client_portal.dat");
    let mut archive = bace_dat::DatArchive::open(&path).unwrap();
    let setup = bace_dat::ModelSetup::decode(&archive.read(0x02000001).unwrap()).unwrap();
    let ids = archive
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 3)
        .take(2048)
        .collect::<Vec<_>>();
    let animation = std::iter::once(setup.default_animation)
        .chain(ids)
        .find(|id| {
            archive
                .read(*id)
                .ok()
                .and_then(|bytes| bace_dat::Animation::decode(&bytes).ok())
                .is_some_and(|animation| {
                    animation.num_parts as usize == setup.parts.len()
                        && !animation.frames.is_empty()
                })
        })
        .expect("compatible animation in supplied DAT");
    let scene = crate::preview_scene::load(crate::preview_scene::PreviewRequest {
        path,
        setup: setup.id,
        animation_id: animation,
        animation_frame: 0,
        animation_patch: Some(bace_content::AnimationSwapPatchV1 {
            schema_version: 1,
            animation_id: animation,
            edits: vec![bace_content::AnimationSwapEditV1 {
                operation: bace_content::AnimationSwapOperationV1::Insert,
                frame: 0,
                hook_index: 0,
                direction: Some(1),
                part_index: Some(0),
                object_id: Some(setup.parts[0]),
            }],
        }),
        ..Default::default()
    })
    .unwrap();
    assert!(!scene.triangles.is_empty());
    assert!(scene.report.iter().any(|line| line.contains("part 0 →")));
}

#[test]
#[ignore = "Requires user-supplied DAT"]
fn supplied_clothing_palette_preview() {
    let path =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"))
            .join("client_portal.dat");
    let mut dat = bace_dat::DatArchive::open(&path).unwrap();
    let ids: Vec<_> = dat
        .records()
        .keys()
        .copied()
        .filter(|id| id >> 24 == 16)
        .collect();
    let mut checked = 0;
    for id in ids {
        let table = bace_dat::ClothingTable::decode(&dat.read(id).unwrap()).unwrap();
        if !table.setups.contains_key(&0x02000001) || table.templates.is_empty() {
            continue;
        }
        let template = *table.templates.keys().next().unwrap();
        let scene = load(PreviewRequest {
            path: path.clone(),
            setup: 0x02000001,
            clothing: id,
            template,
            shade: 0.5,
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("Clothing {id:08X}: {e}"));
        assert!(!scene.triangles.is_empty());
        eprintln!(
            "Clothing {id:08X} template {template}: {} triangles",
            scene.triangles.len()
        );
        checked += 1;
        if checked == 5 {
            break;
        }
    }
    assert_eq!(checked, 5);
}

#[test]
fn preview_appearance_applies_all_fields_atomically_with_undo() {
    use crate::{document::Document, preview_appearance::AppearanceSelection};
    let mut document = Document::from_text(
        "schema_version=1\nweenie_id=1\nclass_name='appearance'\nweenie_type=1\n[[properties.ints]]\nid=1\nvalue=42\n[[properties.ints]]\nid=3\nvalue=2\n[[properties.data_ids]]\nid=1\nvalue=33554433\n",
        None,
        None,
    ).unwrap();
    let before = document.value.clone();
    let selection = AppearanceSelection {
        setup: 0x02000002,
        clothing: 0x10000903,
        palette: 0x04000001,
        template: 94,
        shade: 0.625,
    };
    selection.apply(&mut document).unwrap();
    let weenie = bace_content_tools::parse(&document.validated().unwrap()).unwrap();
    assert_eq!(
        weenie
            .properties
            .ints
            .iter()
            .find(|p| p.id == 1)
            .unwrap()
            .value,
        42
    );
    assert_eq!(
        weenie.properties.ints.iter().filter(|p| p.id == 3).count(),
        1
    );
    assert_eq!(
        weenie
            .properties
            .ints
            .iter()
            .find(|p| p.id == 3)
            .unwrap()
            .value,
        94
    );
    assert_eq!(
        weenie
            .properties
            .floats
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        0.625
    );
    assert_eq!(
        weenie
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 7)
            .unwrap()
            .value,
        0x10000903
    );
    assert_eq!(
        weenie
            .properties
            .data_ids
            .iter()
            .filter(|p| p.id == 1)
            .count(),
        1
    );
    let applied = document.value.clone();
    document.undo();
    assert_eq!(document.value, before);
    document.redo();
    assert_eq!(document.value, applied);
    for invalid in [
        AppearanceSelection {
            palette: 0x0f000001,
            ..selection
        },
        AppearanceSelection {
            template: u32::MAX,
            ..selection
        },
        AppearanceSelection {
            shade: f64::NAN,
            ..selection
        },
        AppearanceSelection {
            setup: 0x02000000,
            ..selection
        },
    ] {
        assert!(invalid.apply(&mut document).is_err());
        assert_eq!(document.value, applied);
    }
}
