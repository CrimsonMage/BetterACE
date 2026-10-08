//! Validated restart-only configuration becomes immutable gameplay input.
use bace_interactions::{PlayerDeathPolicy, RecallPolicy, RegionPreload, WorldPolicies};
#[derive(Clone, Debug)]
pub struct PreparedWorldSettings {
    pub zones: WorldPolicies,
    pub recalls: RecallPolicy,
    pub death: PlayerDeathPolicy,
    pub preloads: Vec<RegionPreload>,
    pub max_resident_regions: usize,
}
pub fn prepare_world_settings(
    config: &bace_config::WorldConfig,
) -> Result<PreparedWorldSettings, String> {
    config.validate().map_err(|e| e.to_string())?;
    let zones = WorldPolicies::new(
        &config.zones.no_relog,
        &config.zones.no_death_item_drop,
        &config.zones.no_kill_experience,
    )
    .map_err(|e| format!("zone policy: {e:?}"))?;
    let c = &config.recalls;
    let recalls = RecallPolicy {
        enabled: [
            c.lifestone,
            c.house,
            c.marketplace,
            c.allegiance_hometown,
            c.allegiance_housing,
            c.pk_arena,
            c.pkl_arena,
        ],
        spell_recalls: c.spell_recalls,
        lifestone_binding: c.lifestone_binding,
        allegiance_binding: c.allegiance_binding,
        pk_timer_seconds: c.pk_timer_seconds,
    };
    let d = &config.death;
    let death = PlayerDeathPolicy {
        vitae_penalty: d.vitae_penalty,
        vitae_penalty_max: d.vitae_penalty_max,
        destroy_pyreals: d.destroy_pyreals,
        lifestone_broadcast: d.lifestone_broadcast,
        pk_respite_seconds: d.pk_respite_seconds,
        pk_server: d.pk_server,
        pkl_server: d.pkl_server,
        safe_training_academy: d.safe_training_academy,
    };
    let mut preloads = std::collections::BTreeMap::<u16, RegionPreload>::new();
    if config.preloading.enabled {
        for entry in config.preloading.entries.iter().filter(|e| e.enabled) {
            let targets: Vec<_> = if entry.apartment_group {
                bace_interactions::apartment_landblocks().collect()
            } else {
                entry.landblock.into_iter().collect()
            };
            for landblock in targets {
                let existing = preloads.entry(landblock).or_insert(RegionPreload {
                    landblock,
                    permanent: false,
                    include_adjacent: false,
                });
                existing.permanent |= entry.permanent;
                existing.include_adjacent |= entry.include_adjacent;
            }
        }
    }
    if preloads.len() > config.preloading.max_resident_regions {
        return Err("preloads exceed region residency capacity".into());
    }
    Ok(PreparedWorldSettings {
        zones,
        recalls,
        death,
        preloads: preloads.into_values().collect(),
        max_resident_regions: config.preloading.max_resident_regions,
    })
}

impl PreparedWorldSettings {
    /// Apply once before launching the dedicated simulation owner. Expand and
    /// bound neighboring preloads before any kernel mutation.
    pub fn install(self, kernel: &mut bace_simulation::Kernel) -> Result<(), String> {
        let mut regions = std::collections::BTreeMap::<u16, bool>::new();
        for preload in &self.preloads {
            let x = i32::from(preload.landblock >> 8);
            let y = i32::from(preload.landblock & 255);
            let radius = if preload.include_adjacent { 1 } else { 0 };
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if (0..=255).contains(&(x + dx)) && (0..=255).contains(&(y + dy)) {
                        *regions
                            .entry((((x + dx) as u16) << 8) | (y + dy) as u16)
                            .or_default() |= preload.permanent;
                    }
                }
            }
        }
        if regions.len() > self.max_resident_regions {
            return Err("expanded preloads exceed region capacity".into());
        }
        kernel
            .configure_world_policies(
                self.zones,
                self.recalls,
                self.death,
                self.max_resident_regions,
            )
            .map_err(|e| format!("world policy admission: {e:?}"))?;
        for (landblock, permanent) in regions {
            kernel
                .request_region(landblock, permanent)
                .expect("preflighted fresh preload capacity");
        }
        Ok(())
    }
}
