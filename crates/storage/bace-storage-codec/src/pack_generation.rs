use crate::{MappedPack, PackError, PackKey, PackLimits, PackLookup};
use std::sync::Arc;

/// Immutable overlay. Deltas are ordered oldest to newest. Atomic installation
/// and durable publication order belong to the application, not this codec.
pub struct PackGeneration {
    revision: u64,
    base: Arc<MappedPack>,
    deltas: Vec<Arc<MappedPack>>,
}

impl PackGeneration {
    pub fn new(
        revision: u64,
        base: Arc<MappedPack>,
        deltas: Vec<Arc<MappedPack>>,
        limits: PackLimits,
    ) -> Result<Self, PackError> {
        if deltas.len() >= limits.max_segments {
            return Err(PackError::Limit("generation segments"));
        }
        Ok(Self {
            revision,
            base,
            deltas,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn lookup(&self, key: PackKey) -> Result<PackLookup, PackError> {
        for segment in self.deltas.iter().rev().chain(std::iter::once(&self.base)) {
            match segment.lookup(key)? {
                PackLookup::Missing => {}
                found => return Ok(found),
            }
        }
        Ok(PackLookup::Missing)
    }
}
