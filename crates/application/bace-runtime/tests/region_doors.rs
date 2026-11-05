//! Approved content regression for the retail door seen in the 0x7f03 trace.
#[test]
#[ignore = "requires approved DATs and accepted native world pack"]
fn supplied_7f03_has_retail_door_source_and_motion() {
    use bace_runtime::region_activation::{
        RegionActivationRequest, RegionAssetManifest, VerifiedRegionAssets,
    };
    use bace_storage_codec::{PackLimits, load_manifest};
    use std::{path::PathBuf, sync::Arc};

    let directory = PathBuf::from(std::env::var_os("BACE_DAT_DIRECTORY").unwrap());
    let path = PathBuf::from(std::env::var_os("BACE_WORLD_MANIFEST").unwrap());
    let manifest = load_manifest(&path, PackLimits::default()).unwrap();
    let hash = manifest.content_hash(PackLimits::default()).unwrap();
    let generation = Arc::new(
        manifest
            .open(path.parent().unwrap(), PackLimits::default())
            .unwrap(),
    );
    let mut assets = VerifiedRegionAssets::open(&RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
        store: None,
    })
    .unwrap();
    let region = assets
        .prepare(&RegionActivationRequest {
            token: 1,
            activation_epoch: 1,
            landblock: 0x7f03,
            generation,
            creature_policy: None,
            treasure_assets: None,
            content_hash: hash,
            aetheria_drop_rate: 0.0,
        })
        .unwrap();
    let door = region
        .content
        .instances
        .iter()
        .find(|instance| instance.source.guid == 0x77f0_3033)
        .expect("retail target absent from approved 0x7f03 content");
    assert_eq!(door.template.weenie_type, 19);
    assert!(!door.source.is_link_child);
    let motion_id = door
        .template
        .properties
        .data_ids
        .iter()
        .find(|p| p.id == 2)
        .map(|p| p.value)
        .expect("door motion table absent");
    eprintln!(
        "door {:#010x} weenie {} motion {:#010x}",
        door.source.guid, door.template.weenie_id, motion_id
    );
    assert_eq!(
        door.template
            .properties
            .floats
            .iter()
            .find(|p| p.id == 54)
            .map(|p| p.value),
        Some(2.0)
    );
}
