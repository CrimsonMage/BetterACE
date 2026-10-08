use bace_content::{LandblockInstanceLinkRowV1, LandblockInstanceRowV1, WeenieV1, WorldRecordV1};
use bace_runtime::world_content::{RegionRequest, WorldContentWorker};
use bace_storage_codec::{PackLimits, load_manifest};
use std::sync::{Arc, atomic::AtomicBool};

fn instance(id: u32, template: u32) -> WorldRecordV1 {
    WorldRecordV1::LandblockInstance(LandblockInstanceRowV1 {
        guid: id,
        landblock: 0xa260,
        weenie_class_id: template,
        obj_cell_id: 0xa260_0001,
        origin_x: 10.0,
        origin_y: 20.0,
        origin_z: 30.0,
        angles_w: 1.0,
        angles_x: 0.0,
        angles_y: 0.0,
        angles_z: 0.0,
        is_link_child: false,
        last_modified: "2005-02-09 10:00:00".into(),
    })
}

#[test]
fn region_preparation_is_bounded_atomic_and_keeps_instance_links() {
    let directory = tempfile::tempdir().unwrap();
    let template = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "npc".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let records = vec![
        instance(0x7a260001, 1),
        instance(0x7a260002, 1),
        WorldRecordV1::LandblockInstanceLink(LandblockInstanceLinkRowV1 {
            id: 1,
            parent_guid: 0x7a260001,
            child_guid: 0x7a260002,
            last_modified: "2005-02-09 10:00:00".into(),
        }),
    ];
    let build = bace_content_tools::build_world_pack(
        std::slice::from_ref(&template),
        &records,
        directory.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = load_manifest(&build.manifest, PackLimits::default()).unwrap();
    let worker = WorldContentWorker::start(
        Arc::new(
            manifest
                .open(directory.path(), PackLimits::default())
                .unwrap(),
        ),
        1,
    )
    .unwrap();
    worker
        .try_submit(RegionRequest {
            token: 17,
            landblock: 0xa260,
        })
        .unwrap();
    let complete = worker.shutdown().unwrap().remove(0);
    assert_eq!(complete.request.token, 17);
    let region = complete.result.unwrap();
    assert_eq!(region.content_generation, 1);
    assert_eq!(region.instances.len(), 2);
    assert!(Arc::ptr_eq(
        &region.instances[0].template,
        &region.instances[1].template
    ));
    assert_eq!(region.instances[0].links[0].child_guid, 0x7a260002);
    let missing = tempfile::tempdir().unwrap();
    let build = bace_content_tools::build_world_pack(
        &[template],
        &[instance(0x7a260001, 1), instance(0x7a260002, 2)],
        missing.path(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let manifest = load_manifest(&build.manifest, PackLimits::default()).unwrap();
    let worker = WorldContentWorker::start(
        Arc::new(
            manifest
                .open(missing.path(), PackLimits::default())
                .unwrap(),
        ),
        1,
    )
    .unwrap();
    worker
        .try_submit(RegionRequest {
            token: 18,
            landblock: 0xa260,
        })
        .unwrap();
    assert!(
        worker
            .shutdown()
            .unwrap()
            .remove(0)
            .result
            .err()
            .unwrap()
            .contains("missing required template 2")
    );
}

#[test]
#[ignore = "requires local complete world pack in BACE_WORLD_MANIFEST"]
fn supplied_world_holtburg_loads_without_decoding_the_world() {
    let path = std::path::PathBuf::from(
        std::env::var_os("BACE_WORLD_MANIFEST").expect("BACE_WORLD_MANIFEST"),
    );
    let manifest = load_manifest(&path, PackLimits::default()).unwrap();
    let generation = manifest
        .open(path.parent().unwrap(), PackLimits::default())
        .unwrap();
    let worker = WorldContentWorker::start(Arc::new(generation), 1).unwrap();
    worker
        .try_submit(RegionRequest {
            token: 1,
            landblock: 0xa260,
        })
        .unwrap();
    let region = worker.shutdown().unwrap().remove(0).result.unwrap();
    assert!(!region.instances.is_empty());
    assert!(region.instances.len() < 4096);
    let ids: std::collections::BTreeSet<_> =
        region.instances.iter().map(|i| i.source.guid).collect();
    assert_eq!(ids.len(), region.instances.len());
    eprintln!(
        "Holtburg prepared: {} instances, {} encounters; geometry admission still required",
        region.instances.len(),
        region.encounters.len()
    );
}
