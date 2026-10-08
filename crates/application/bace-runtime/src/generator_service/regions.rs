//! Narrow accepted-region/source-cache port. RegionService is the production owner.
use super::*;
use crate::region_unload_saves::RegionItemSource;
pub trait GeneratorRegions {
    fn prepared_region(&self, landblock: u16) -> Option<&Arc<PreparedRegionActivation>>;
    fn record_sources(
        &mut self,
        landblock: u16,
        batch: PreparedRegionSources,
    ) -> Result<(), Box<PreparedRegionSources>>;
    fn transient_source(&self, id: EntityId) -> Option<&RegionItemSource>;
    fn forget_transient_sources(&mut self, ids: &[EntityId]);
    fn forget_transient_tree(&mut self, root: EntityId);
}
impl GeneratorRegions for RegionService {
    fn prepared_region(&self, landblock: u16) -> Option<&Arc<PreparedRegionActivation>> {
        RegionService::prepared_region(self, landblock)
    }
    fn record_sources(
        &mut self,
        landblock: u16,
        batch: PreparedRegionSources,
    ) -> Result<(), Box<PreparedRegionSources>> {
        RegionService::record_sources(self, landblock, batch)
    }
    fn transient_source(&self, id: EntityId) -> Option<&RegionItemSource> {
        RegionService::transient_source(self, id)
    }
    fn forget_transient_sources(&mut self, ids: &[EntityId]) {
        RegionService::forget_transient_sources(self, ids)
    }
    fn forget_transient_tree(&mut self, root: EntityId) {
        RegionService::forget_transient_tree(self, root)
    }
}
