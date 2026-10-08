use crate::preview_scene::{PreviewRequest, load};
use bace_content::{
    ClothingPalette, ClothingPaletteEffect, ClothingPart, ClothingPatchV1, ClothingRange,
    ClothingSetup, ClothingTexture, PaletteSource,
};
#[test]
fn native_clothing_document_preserves_order_and_checks_external_changes() {
    let p = ClothingPatchV1 {
        schema_version: 1,
        id: 0x1000fffe,
        setups: vec![ClothingSetup {
            id: 0x02000001,
            parts: vec![ClothingPart {
                index: 9,
                model: 0x01000001,
                textures: vec![ClothingTexture {
                    old: 0x05000001,
                    new: 0x05000002,
                }],
            }],
        }],
        palettes: vec![ClothingPalette {
            template: 94,
            icon: 0,
            effects: vec![ClothingPaletteEffect {
                source: PaletteSource::Palette(0x04000001),
                ranges: vec![
                    ClothingRange {
                        offset: 0,
                        colors: 8,
                    },
                    ClothingRange {
                        offset: 32,
                        colors: 16,
                    },
                ],
            }],
        }],
    };
    let text = crate::clothing_document::native_text(&p).unwrap();
    assert_eq!(crate::clothing_document::native_parse(&text).unwrap(), p);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("clothing.toml");
    crate::document::write(&file, &text, None).unwrap();
    use sha2::{Digest, Sha256};
    let expected: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    std::fs::write(&file, "externally changed").unwrap();
    assert!(crate::document::write(&file, &text, Some(expected)).is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "externally changed");
}
#[test]
#[ignore = "Requires user-supplied DAT and local CustomClothingBase reference checkout"]
fn supplied_custom_clothing_examples_resolve_all_overrides() {
    let dat =
        std::path::PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").expect("DAT directory"))
            .join("client_portal.dat");
    let reference = std::path::PathBuf::from(
        std::env::var_os("BACE_CUSTOM_CLOTHING_REFERENCE")
            .expect("CustomClothingBase reference checkout"),
    );
    for (filename, template) in [
        ("10000188.json", 94),
        ("10000322.json", 8),
        ("10000900.json", 0),
    ] {
        let patch = bace_import::import_clothing_json(
            &std::fs::read_to_string(reference.join("Examples").join(filename)).unwrap(),
        )
        .unwrap();
        let scene = load(PreviewRequest {
            path: dat.clone(),
            setup: 0x02000001,
            clothing: patch.id,
            template,
            shade: 0.5,
            clothing_patch: Some(patch.clone()),
            ..Default::default()
        })
        .unwrap_or_else(|e| panic!("{filename}: {e}"));
        let resolved = scene.clothing.as_ref().unwrap();
        for setup in &patch.setups {
            assert_eq!(
                resolved.setups.iter().find(|s| s.id == setup.id),
                Some(setup)
            );
        }
        for palette in &patch.palettes {
            assert_eq!(
                resolved
                    .palettes
                    .iter()
                    .find(|p| p.template == palette.template),
                Some(palette)
            );
        }
        assert!(!scene.triangles.is_empty());
        assert!(scene.report.iter().any(|r| r.contains("Native override")));
        let image = crate::preview_render::render(&scene, Default::default()).unwrap();
        assert!(
            image
                .pixels
                .iter()
                .filter(|p| **p != eframe::egui::Color32::from_rgb(13, 19, 27))
                .count()
                > 1000
        );
        eprintln!(
            "{filename} / DID {:08X}: {} setups, {} templates, {} triangles",
            patch.id,
            resolved.setups.len(),
            resolved.palettes.len(),
            scene.triangles.len()
        );
    }
}

#[test]
fn copying_selected_entries_retains_identity_order_and_unrelated_variants() {
    use crate::clothing_document::ClothingDocument;
    let mut document = ClothingDocument::new(ClothingPatchV1 {
        schema_version: 1,
        id: 0x1000ffff,
        setups: vec![
            ClothingSetup {
                id: 0x02000001,
                parts: vec![],
            },
            ClothingSetup {
                id: 0x02000002,
                parts: vec![],
            },
        ],
        palettes: vec![
            ClothingPalette {
                template: 94,
                icon: 0,
                effects: vec![],
            },
            ClothingPalette {
                template: 8,
                icon: 0,
                effects: vec![],
            },
        ],
    });
    let before = document.patch.clone();
    let setup = ClothingSetup {
        id: 0x02000001,
        parts: vec![ClothingPart {
            index: 30,
            model: 0x01000001,
            textures: vec![
                ClothingTexture {
                    old: 0x05000001,
                    new: 0x05000002,
                },
                ClothingTexture {
                    old: 0x05000003,
                    new: 0x05000004,
                },
            ],
        }],
    };
    let palette = ClothingPalette {
        template: 94,
        icon: 0x06000001,
        effects: vec![ClothingPaletteEffect {
            source: PaletteSource::Palette(0x04000001),
            ranges: vec![
                ClothingRange {
                    offset: 0,
                    colors: 8,
                },
                ClothingRange {
                    offset: 16,
                    colors: 24,
                },
            ],
        }],
    };
    let undo = document
        .copy_entries(Some(setup.clone()), Some(palette.clone()))
        .unwrap();
    assert_eq!(undo, before);
    assert_eq!(document.patch.id, 0x1000ffff);
    assert_eq!(document.patch.setups, vec![setup, before.setups[1].clone()]);
    assert_eq!(
        document.patch.palettes,
        vec![palette, before.palettes[1].clone()]
    );
    let applied = document.patch.clone();
    assert!(
        document
            .copy_entries(
                Some(ClothingSetup {
                    id: 0,
                    parts: vec![]
                }),
                None
            )
            .is_err()
    );
    assert_eq!(document.patch, applied);
}
