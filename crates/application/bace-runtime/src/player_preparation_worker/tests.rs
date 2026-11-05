use super::*;

#[test]
#[ignore = "requires approved real DATs; set BACE_DAT_DIRECTORY"]
fn repeated_avatar_closure_reuses_approved_dat_decoding() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("BACE_DAT_DIRECTORY").expect("approved DAT directory"),
    );
    let manifest = RegionAssetManifest {
        portal: directory.join("client_portal.dat"),
        cell: directory.join("client_cell_1.dat"),
        portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4".into(),
        cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
        store: None,
    };
    let mut portal = bace_dat::DatArchive::open(&manifest.portal).unwrap();
    let chargen =
        bace_dat::CharGen::decode(&portal.read(bace_dat::CharGen::RECORD_ID).unwrap()).unwrap();
    let gender = &chargen.heritage_groups[&1].genders[&1];
    let mut source = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "cache-test-avatar".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.data_ids = vec![
        bace_content::Property {
            id: 1,
            value: gender.setup,
        },
        bace_content::Property {
            id: 2,
            value: gender.motion_table,
        },
        bace_content::Property {
            id: 4,
            value: 0x3000_0000,
        },
    ];
    let mut assets = VerifiedRegionAssets::open(&manifest).unwrap();
    let mut cache = AvatarCache::default();
    let start = std::time::Instant::now();
    let first = cache.get_or_prepare(&mut assets, &source).unwrap();
    let cold = start.elapsed();
    let start = std::time::Instant::now();
    let second = cache.get_or_prepare(&mut assets, &source).unwrap();
    let warm = start.elapsed();
    assert!(Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(&first.character, &second.character));
    assert!(Arc::ptr_eq(&first.spells, &second.spells));
    eprintln!("avatar DAT closure: cold {cold:?}, cached {warm:?}");
}
