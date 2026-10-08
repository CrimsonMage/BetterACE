use bace_content::{Property, WeenieV1};
use bace_storage_codec::{PackKey, PackLookup};
#[test]
fn startup_name_index_selects_creatures_and_keeps_template_identity() {
    let source = |id, kind, name: &str| {
        let mut w = WeenieV1 {
            schema_version: 1,
            weenie_id: id,
            class_name: format!("name_{id}"),
            weenie_type: kind,
            last_modified: None,
            properties: Default::default(),
        };
        w.properties.strings.push(Property {
            id: 1,
            value: name.to_owned(),
        });
        w
    };
    let temp = tempfile::tempdir().unwrap();
    let built = bace_content_tools::build_world_pack(
        &[
            source(3, 10, "Drudge"),
            source(1, 10, "Drudge"),
            source(2, 1, "Player"),
        ],
        &[],
        temp.path(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    let manifest = bace_storage_codec::load_manifest(&built.manifest, Default::default()).unwrap();
    let generation = manifest.open(temp.path(), Default::default()).unwrap();
    let PackLookup::Record(record) = generation
        .lookup(PackKey {
            namespace: 48,
            id: 1,
        })
        .unwrap()
    else {
        panic!("missing name index");
    };
    let index = bace_content_tools::decode_creature_names(record.bytes()).unwrap();
    assert_eq!(
        index
            .entries
            .iter()
            .map(|r| (r.template, r.name.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "Drudge"), (3, "Drudge")]
    );
    // Same-name templates keep independent identities for live replacement/removal.
    assert_eq!(index.entries.len(), 2);
}

#[test]
fn offline_acceptance_rejects_partial_stale_and_extra_derived_identities() {
    use bace_content::{
        CreatureNameIndexV1, CreatureNameV1, TemplateClassIdentityV1, TemplateClassIndexV1,
    };
    use bace_storage_codec::{MappedPack, PackRecord};
    let template = bace_content_tools::compile("schema_version=1\nweenie_id=7\nclass_name=\"seven\"\nweenie_type=10\n[[properties.strings]]\nid=1\nvalue=\"Drudge\"\n").unwrap();
    let names = CreatureNameIndexV1 {
        schema_version: 1,
        entries: vec![CreatureNameV1 {
            template: 7,
            name: "Drudge".into(),
        }],
    };
    let classes = TemplateClassIndexV1 {
        schema_version: 1,
        entries: vec![TemplateClassIdentityV1 {
            template: 7,
            class_name: "seven".into(),
        }],
    };
    for fault in 0..6 {
        let mut names = names.clone();
        let mut classes = classes.clone();
        match fault {
            2 => classes.entries[0].class_name = "old".into(),
            3 => names.entries[0].name = "old".into(),
            4 => names.entries.push(CreatureNameV1 {
                template: 8,
                name: "Extra".into(),
            }),
            _ => {}
        }
        let mut records = vec![PackRecord {
            key: PackKey {
                namespace: 1,
                id: 7,
            },
            schema: 1,
            value: Some(template.clone()),
        }];
        if fault != 1 {
            records.push(PackRecord {
                key: PackKey {
                    namespace: 48,
                    id: 1,
                },
                schema: if fault == 5 { 2 } else { 1 },
                value: Some(bace_content_tools::compile_creature_names(&names).unwrap()),
            });
        }
        records.push(PackRecord {
            key: PackKey {
                namespace: 49,
                id: 1,
            },
            schema: 1,
            value: Some(bace_content_tools::compile_template_classes(&classes).unwrap()),
        });
        let directory = tempfile::tempdir().unwrap();
        let descriptor = bace_storage_codec::compile_pack(
            directory.path(),
            records.into_iter().map(Ok),
            Default::default(),
        )
        .unwrap();
        let pack = MappedPack::open(directory.path(), &descriptor, Default::default()).unwrap();
        assert_eq!(
            bace_content_tools::validate_world_pack_indexes(&pack).is_ok(),
            fault == 0,
            "fault {fault}"
        );
    }
}
