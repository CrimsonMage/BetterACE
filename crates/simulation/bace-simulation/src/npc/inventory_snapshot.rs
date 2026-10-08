//! In-memory freeze receipt; storage uses explicit item DTOs and source fences.
use super::*;
#[derive(Clone, Debug, PartialEq)]
pub struct NpcSourceInventorySnapshot {
    pub source: EntityId,
    pub ticket: u64,
    pub origin: Option<crate::GeneratedNpcOrigin>,
    pub items: Vec<crate::RegionUnloadItem>,
    pub root_item: Option<crate::RegionUnloadItem>,
    pub source_registry_revision: Option<u64>,
    pub source_enchantments: Vec<bace_magic::EnchantmentEntry>,
    pub death_items: Vec<EntityId>,
}
impl Npcs {
    pub(crate) fn source_hold(&self, source: EntityId) -> Option<u64> {
        self.sources
            .get(&source)
            .and_then(|s| s.journal_hold.map(|(ticket, _)| ticket))
            .or_else(|| {
                self.handins.iter().find_map(|(ticket, (handin, adopted))| {
                    (!adopted && handin.request.source == source).then_some(*ticket)
                })
            })
    }
}
