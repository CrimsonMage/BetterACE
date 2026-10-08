//! Trusted lifecycle ingress retains rejected preparations under backpressure.
use super::*;
impl Kernel {
    pub fn take_region_admission_outcome(&mut self) -> Option<crate::RegionAdmissionOutcome> {
        self.region_admission_outcomes.pop_front()
    }
    /// A worker may restore an undelivered outcome into the slot it just drained.
    pub fn restore_region_admission_outcome(
        &mut self,
        outcome: crate::RegionAdmissionOutcome,
    ) -> Result<(), crate::RegionAdmissionOutcome> {
        if !self.region_admission_outcomes.is_empty() {
            return Err(outcome);
        }
        self.region_admission_outcomes.push_front(outcome);
        Ok(())
    }
    pub fn has_region_admission_work(&self) -> bool {
        !self.region_admission_outcomes.is_empty()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::AdmitResidentRegion(_)))
    }
    pub(super) fn handle_region_admission(&mut self, request: crate::RegionAdmissionRequest) {
        let result = if request.valid_bounds() {
            if request.prepared.refresh.is_some() {
                self.refresh_resident_region_content(*request.prepared)
            } else {
                self.admit_resident_region_complete(*request.prepared)
            }
        } else {
            Err((crate::GeneratorServiceError::Capacity, request.prepared))
        };
        self.region_admission_outcomes
            .push_back(crate::RegionAdmissionOutcome {
                tick: self.tick,
                correlation: request.correlation,
                result,
            });
    }
}
