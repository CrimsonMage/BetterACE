use bace_config::WorldConfig;
use bace_runtime::world_settings::prepare_world_settings;
use bace_simulation::{Kernel, RegionLifecycleEvent};
#[test]
fn source_preloads_request_hebian_to_and_preserve_disabled_holtburg() {
    let mut kernel = Kernel::new(bace_world::World::default(), 64).unwrap();
    prepare_world_settings(&WorldConfig::default())
        .unwrap()
        .install(&mut kernel)
        .unwrap();
    assert!(kernel.region_residency().state(0xE74E).unwrap().permanent);
    assert!(kernel.region_residency().state(0xA9B4).is_none());
    assert_eq!(
        kernel.take_region_lifecycle_event(),
        Some(RegionLifecycleEvent::Prepare {
            landblock: 0xE74E,
            epoch: 1
        })
    );
    assert_eq!(kernel.take_region_lifecycle_event(), None);
}
#[test]
fn neighbor_preloads_are_bounded_before_owner_mutation() {
    let mut config = WorldConfig::default();
    for entry in &mut config.preloading.entries {
        entry.enabled = false;
    }
    let entry = &mut config.preloading.entries[0];
    entry.enabled = true;
    entry.landblock = Some(0x0101);
    entry.include_adjacent = true;
    entry.permanent = true;
    config.preloading.max_resident_regions = 4;
    let prepared = prepare_world_settings(&config).unwrap();
    let mut kernel = Kernel::new(bace_world::World::default(), 64).unwrap();
    assert!(prepared.install(&mut kernel).is_err());
    assert!(!kernel.region_residency().has_state());
    config.preloading.max_resident_regions = 9;
    prepare_world_settings(&config)
        .unwrap()
        .install(&mut kernel)
        .unwrap();
    assert_eq!(kernel.region_residency().states().count(), 9);
    assert!(
        kernel
            .region_residency()
            .states()
            .all(|(_, state)| state.permanent)
    );
}
