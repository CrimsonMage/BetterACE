//! Original Spawn foreach ordering, with each root's complete constructed graph.
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone)]
pub enum PreparedMixedGeneratorRoot {
    Item {
        root: EntityId,
        items: Vec<bace_inventory::InventoryItem>,
        containers: Vec<bace_inventory::InventoryContainer>,
        shape: Arc<bace_physics::CollisionShape>,
    },
    Creature {
        root: EntityId,
        template: u32,
        loadout: Box<crate::PreparedNpcLoadout>,
    },
}
impl PreparedMixedGeneratorRoot {
    pub fn entity(&self) -> EntityId {
        match self {
            Self::Item { root, .. } | Self::Creature { root, .. } => *root,
        }
    }
    pub fn entity_count(&self) -> usize {
        match self {
            Self::Item { items, .. } => items.len(),
            Self::Creature { loadout, .. } => 1 + loadout.items.len(),
        }
    }
    pub fn container_count(&self) -> usize {
        match self {
            Self::Item { containers, .. } => containers.len(),
            Self::Creature { loadout, .. } => loadout.containers.len(),
        }
    }
    pub fn enchantment_count(&self) -> usize {
        match self {
            Self::Item { .. } => 0,
            Self::Creature { loadout, .. } => loadout.enchantments.len(),
        }
    }
    pub fn valid_bounds(&self) -> bool {
        match self {
            Self::Item {
                items, containers, ..
            } => !items.is_empty() && items.len() <= 1024 && containers.len() <= 1024,
            Self::Creature { loadout, .. } => {
                loadout.items.len() < 1024
                    && loadout.containers.len() <= 1024
                    && loadout.death_items.len() <= loadout.items.len().min(256)
                    && loadout.death_parents.len() == loadout.death_items.len()
                    && loadout.death_ids.len() == loadout.death_items.len()
                    && loadout.enchantments.len() <= 4096
            }
        }
    }
}
