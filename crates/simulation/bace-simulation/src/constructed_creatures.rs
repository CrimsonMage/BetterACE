//! Immutable construction input; inventory and Magic keep the mutable graph and registries.
use bace_inventory::InventoryItem;
#[derive(Clone)]
pub struct PreparedContainedCreature {
    pub root: InventoryItem,
    pub loadout: Box<crate::PreparedNpcLoadout>,
    /// Exact factory subtype. Pets, Vendor and GamePiece require their own lifecycle.
    pub weenie_type: u32,
}
impl PreparedContainedCreature {
    pub fn valid_bounds(&self) -> bool {
        matches!(self.weenie_type, 10 | 15)
            && self.root.id.0 != 0
            && self.root.stack == 1
            && self.root.is_container
            && self.loadout.items.len() < 1024
            && self.loadout.containers.len() <= 1024
            && self.loadout.enchantments.len() <= 4096
            && self.loadout.death_items.len() <= 256
            && self.loadout.death_ids.len() == self.loadout.death_items.len()
            && self.loadout.death_parents.len() == self.loadout.death_items.len()
    }
}

/// Complete incoming forest for a single Contain occurrence. The immutable
/// loadouts describe creature-owned equipment; they are never another live graph.
#[derive(Clone)]
pub struct PreparedContainedForest {
    pub roots: Vec<bace_types::EntityId>,
    pub items: Vec<InventoryItem>,
    pub containers: Vec<bace_inventory::InventoryContainer>,
    pub creatures: Vec<PreparedConstructedCreature>,
}
#[derive(Clone)]
pub struct PreparedConstructedCreature {
    pub entity: bace_types::EntityId,
    pub weenie_type: u32,
    pub loadout: Box<crate::PreparedNpcLoadout>,
}
impl PreparedContainedForest {
    pub fn valid_bounds(&self) -> bool {
        !self.roots.is_empty()
            && self.roots.len() <= 1024
            && !self.items.is_empty()
            && self.items.len() <= 1024
            && self.containers.len() <= 1024
            && self.creatures.len() <= 128
            && self.creatures.iter().all(|c| {
                matches!(c.weenie_type, 10 | 15)
                    && c.loadout.items.len() < 1024
                    && c.loadout.containers.len() <= 1024
                    && c.loadout.death_items.len() <= 256
                    && c.loadout.death_ids.len() == c.loadout.death_items.len()
                    && c.loadout.death_parents.len() == c.loadout.death_items.len()
            })
            && self
                .creatures
                .iter()
                .map(|c| c.loadout.enchantments.len())
                .sum::<usize>()
                <= 4096
            && self
                .creatures
                .iter()
                .map(|c| c.loadout.items.len())
                .sum::<usize>()
                <= 4096
    }
}
