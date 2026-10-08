use super::*;
impl Magic {
    pub(crate) fn validate_region_registries(
        &self,
        registries: &[(EntityId, EnchantmentRegistry)],
        now: f64,
    ) -> Result<(), CastRejection> {
        if registries.len() > 4096usize.saturating_sub(self.registries.len()) {
            return Err(CastRejection::Capacity);
        }
        if !now.is_finite() || now < self.current_time || now + 5.0 <= now {
            return Err(CastRejection::InvalidState);
        }
        let mut seen = BTreeSet::new();
        for (id, _) in registries {
            if id.0 == 0 || id.0 == u32::MAX || self.reserves_identity(*id) || !seen.insert(*id) {
                return Err(CastRejection::InvalidState);
            }
        }
        Ok(())
    }
}
