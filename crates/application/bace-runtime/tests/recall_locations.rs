use bace_content::{Position, Property, TemplateClassIdentityV1, TemplateClassIndexV1, WeenieV1};
use bace_runtime::portal_preparation::prepare_recall_locations;
use std::sync::atomic::AtomicBool;
fn names() -> Vec<String> {
    std::iter::once("portalmarketplace".into())
        .chain((1..=5).map(|i| format!("portalpkarenanew{i}")))
        .chain((1..=5).map(|i| format!("portalpklarenanew{i}")))
        .collect()
}
fn item(id: u32, name: String, destination: Option<Position>) -> WeenieV1 {
    let mut source = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: name,
        weenie_type: 7,
        last_modified: None,
        properties: Default::default(),
    };
    if let Some(value) = destination {
        source.properties.positions.push(Property { id: 2, value });
    }
    source
}
fn destination() -> Position {
    Position {
        obj_cell_id: 0xa9b40001,
        position_x: 1.25,
        position_y: -2.5,
        position_z: 3.75,
        rotation_w: 0.8,
        rotation_x: 0.,
        rotation_y: 0.,
        rotation_z: 0.6,
    }
}
fn pack(sources: &[WeenieV1]) -> (tempfile::TempDir, bace_storage_codec::PackGeneration) {
    let dir = tempfile::tempdir().unwrap();
    let built =
        bace_content_tools::build_world_pack(sources, &[], dir.path(), &AtomicBool::new(false))
            .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let pack = manifest.open(dir.path(), Default::default()).unwrap();
    (dir, pack)
}
fn index(sources: &[WeenieV1]) -> TemplateClassIndexV1 {
    TemplateClassIndexV1 {
        schema_version: 1,
        entries: sources
            .iter()
            .map(|s| TemplateClassIdentityV1 {
                template: s.weenie_id,
                class_name: s.class_name.clone(),
            })
            .collect(),
    }
}
#[test]
fn accepted_classes_match_original_csharp_fallback_and_override_bits() {
    let rows: Vec<Vec<u32>> = include_str!("fixtures/recall_locations.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split(',').map(|v| v.parse().unwrap()).collect())
        .collect();
    assert_eq!(rows.len(), 33);
    for mode in 0..3 {
        let mut sources = vec![item(100, "unrelated".into(), None)];
        if mode != 0 {
            sources.extend(
                names()
                    .into_iter()
                    .enumerate()
                    .map(|(i, name)| item(101 + i as u32, name, (mode == 2).then(destination))),
            );
        }
        let (_dir, generation) = pack(&sources);
        let prepared = prepare_recall_locations(&generation, &index(&sources)).unwrap();
        for (i, actual) in std::iter::once(prepared.marketplace)
            .chain(prepared.pk_arena)
            .chain(prepared.pkl_arena)
            .enumerate()
        {
            let expected = &rows[mode * 11 + i];
            assert_eq!(expected[..2], [mode as u32, i as u32]);
            let actual_bits: Vec<_> = std::iter::once(actual.cell)
                .chain(actual.origin.map(f32::to_bits))
                .chain(actual.rotation.map(f32::to_bits))
                .collect();
            assert_eq!(
                actual_bits,
                expected[2..],
                "source mode={mode} destination={i}"
            );
        }
    }
}
#[test]
fn broken_accepted_class_index_cannot_silently_select_a_fallback() {
    let sources = vec![item(100, "unrelated".into(), None)];
    let (_dir, generation) = pack(&sources);
    let mut classes = index(&sources);
    classes.entries[0].class_name = "portalmarketplace".into();
    assert!(
        prepare_recall_locations(&generation, &classes)
            .unwrap_err()
            .contains("identity mismatch")
    );
    classes.entries[0].template = 999;
    assert!(
        prepare_recall_locations(&generation, &classes)
            .unwrap_err()
            .contains("missing accepted template")
    );
    classes.entries.push(classes.entries[0].clone());
    assert!(prepare_recall_locations(&generation, &classes).is_err());
}
