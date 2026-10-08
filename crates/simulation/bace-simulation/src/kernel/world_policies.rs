use super::*;
use bace_interactions::{PlayerDeathPolicy, RecallPolicy, WorldPolicies};
impl Kernel {
    pub fn peek_region_lifecycle_event(&self) -> Option<crate::RegionLifecycleEvent> {
        self.region_residency.peek_event()
    }
    pub fn admit_resident_region(
        &mut self,
        region: crate::PreparedGeneratorRegion,
        dungeon: bool,
        keep_alive: u32,
    ) -> Result<(), crate::GeneratorServiceError> {
        let landblock = region.landblock;
        let epoch = region.epoch;
        let state = self
            .region_residency
            .state(landblock)
            .ok_or(crate::GeneratorServiceError::Stale)?;
        if state.epoch != epoch
            || state.phase != crate::RegionPhase::Preparing
            || self.tick.checked_add(150).is_none()
            || keep_alive > 4096
        {
            return Err(crate::GeneratorServiceError::Stale);
        }
        let roots = region.roots.iter().map(|r| r.entity).collect();
        self.admit_generator_region(region)?;
        self.region_unloads.roots.insert(landblock, roots);
        self.region_residency
            .admit(landblock, epoch, dungeon, self.tick)
            .expect("preflighted resident admission");
        self.region_residency
            .keep_alive(landblock, epoch, keep_alive)
            .expect("preflighted residency identity");
        Ok(())
    }

    /// Startup-only; installed before players, generators or region requests.
    pub fn configure_world_policies(
        &mut self,
        zones: WorldPolicies,
        recalls: RecallPolicy,
        death: PlayerDeathPolicy,
        max_regions: usize,
    ) -> Result<(), crate::ResidencyError> {
        if self.tick != 0
            || self.world.states().next().is_some()
            || self.region_residency.has_state()
        {
            return Err(crate::ResidencyError::Busy);
        }
        death
            .validate()
            .map_err(|_| crate::ResidencyError::Invalid)?;
        if recalls.pk_timer_seconds > 86400 {
            return Err(crate::ResidencyError::Invalid);
        }
        let regions = crate::RegionResidency::new(max_regions)?;
        self.world_policies = zones;
        self.recall_policy = recalls;
        self.death_policy = death;
        self.region_residency = regions;
        Ok(())
    }
    pub fn world_policies(&self) -> &WorldPolicies {
        &self.world_policies
    }
    /// No live character may remain when shutdown freezes regional residency.
    /// Pending cold admissions retain their epoch and drain after admission.
    pub fn quiesce_regions(&mut self) -> Result<(), crate::ResidencyError> {
        if !self.characters.is_empty() {
            return Err(crate::ResidencyError::Busy);
        }
        self.region_residency.begin_drain();
        Ok(())
    }
    pub fn request_region(
        &mut self,
        landblock: u16,
        permanent: bool,
    ) -> Result<u64, crate::ResidencyError> {
        self.region_residency
            .request(landblock, permanent, self.tick)
    }
    pub fn acknowledge_region_admission(
        &mut self,
        landblock: u16,
        epoch: u64,
        dungeon: bool,
    ) -> Result<(), crate::ResidencyError> {
        self.region_residency
            .admit(landblock, epoch, dungeon, self.tick)
    }
    pub fn region_residency(&self) -> &crate::RegionResidency {
        &self.region_residency
    }
    pub fn retry_region_preparation(
        &mut self,
        landblock: u16,
        epoch: u64,
    ) -> Result<(), crate::ResidencyError> {
        self.region_residency.retry_preparation(landblock, epoch)
    }
    pub fn take_region_lifecycle_event(&mut self) -> Option<crate::RegionLifecycleEvent> {
        self.region_residency.take_event()
    }
    pub fn set_region_keep_alive_count(
        &mut self,
        landblock: u16,
        epoch: u64,
        count: u32,
    ) -> Result<(), crate::ResidencyError> {
        self.region_residency.keep_alive(landblock, epoch, count)
    }
    pub(in crate::kernel) fn step_region_residency(&mut self) -> Result<(), SimulationError> {
        self.region_activity_scratch.clear();
        self.region_activity_scratch
            .extend(self.world.states().filter_map(|(id, cell, _)| {
                self.characters
                    .get(id)
                    .is_some()
                    .then_some((cell.0 >> 16) as u16)
            }));
        self.region_activity_scratch.sort_unstable();
        self.region_activity_scratch.dedup();
        for &landblock in &self.region_activity_scratch {
            if self.region_residency.state(landblock).is_some() {
                match self.region_residency.activity(landblock, self.tick) {
                    Ok(()) => {}
                    Err(crate::ResidencyError::Capacity) => break,
                    Err(_) => return Err(SimulationError::AuxiliaryRevision),
                }
            }
        }
        self.region_residency
            .advance(self.tick)
            .map_err(|_| SimulationError::AuxiliaryRevision)?;
        for (landblock, state) in self.region_residency.states() {
            self.world.set_region_dormant(
                landblock,
                matches!(
                    state.phase,
                    crate::RegionPhase::Dormant | crate::RegionPhase::Draining
                ),
            )?;
        }
        Ok(())
    }
}
