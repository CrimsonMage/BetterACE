//! Bounded trusted region transfer; rejection retains every prepared owner.
pub struct RegionAdmissionRequest {
    pub correlation: u64,
    pub prepared: Box<crate::PreparedResidentRegion>,
}
impl RegionAdmissionRequest {
    pub fn valid_bounds(&self) -> bool {
        let input = &self.prepared;
        self.correlation != 0
            && input.visibility.len() <= 1024
            && input
                .visibility
                .iter()
                .try_fold(0usize, |n, row| n.checked_add(row.visible_cells.len()))
                .is_some_and(|n| n <= 1_048_576)
            && input.keep_alive <= 4096
            && input.refresh.as_ref().is_none_or(|r| {
                r.expected_epoch != 0
                    && r.expected_revision != 0
                    && r.expected_manifest != [0; 32]
                    && r.remove_roots.len() <= 4096
            })
            && input.region.roots.len() <= 4096
            && input.region.containers.len() <= 4096
            && input.region.definitions.len() <= 4096
            && input.region.templates.len() <= 4096
            && input.region.creatures.len() <= 4096
            && input.items.items.len() <= 4096
            && input.items.containers.len() <= 4096
            && input.items.roots.len() <= 4096
            && input.items.registries.len() <= 4096
            && input
                .items
                .registries
                .iter()
                .try_fold(0usize, |n, (_, r)| n.checked_add(r.entries().len()))
                .is_some_and(|n| n <= 65536)
    }
}
pub struct RegionAdmissionOutcome {
    /// Simulation-owner tick at which this admission was accepted or rejected.
    pub tick: u64,
    pub correlation: u64,
    pub result: Result<
        (),
        (
            crate::GeneratorServiceError,
            Box<crate::PreparedResidentRegion>,
        ),
    >,
}
