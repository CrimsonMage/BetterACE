//! Fixed-size immutable destination evidence for cold geometry preparation. The
//! actual recall resolves permissions and location again on the world owner.
use bace_interactions::{PortalPosition, RecallError, RecallKind};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecallDestinationSnapshot {
    pub lifestone: Result<PortalPosition, RecallError>,
    pub house: Result<PortalPosition, RecallError>,
    pub marketplace: Result<PortalPosition, RecallError>,
    pub allegiance_hometown: Result<PortalPosition, RecallError>,
    pub allegiance_housing: Result<PortalPosition, RecallError>,
    pub pk_arena: Result<[PortalPosition; 5], RecallError>,
    pub pkl_arena: Result<[PortalPosition; 5], RecallError>,
}
impl RecallDestinationSnapshot {
    /// Arena inspection returns all candidates without advancing a random stream
    /// or reserving an ordinal; selection remains with the actual recall owner.
    pub fn candidates(&self, kind: RecallKind) -> Result<&[PortalPosition], RecallError> {
        let one = match kind {
            RecallKind::Lifestone => &self.lifestone,
            RecallKind::House => &self.house,
            RecallKind::Marketplace => &self.marketplace,
            RecallKind::AllegianceHometown => &self.allegiance_hometown,
            RecallKind::AllegianceHousing => &self.allegiance_housing,
            RecallKind::PkArena => {
                return self.pk_arena.as_ref().map(|p| p.as_slice()).map_err(|e| *e);
            }
            RecallKind::PklArena => {
                return self
                    .pkl_arena
                    .as_ref()
                    .map(|p| p.as_slice())
                    .map_err(|e| *e);
            }
        };
        one.as_ref().map(std::slice::from_ref).map_err(|e| *e)
    }
}
