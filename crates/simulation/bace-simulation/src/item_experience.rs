//! Prepared item progression and exact durable reward patches. Gameplay math is
//! owned by bace-character; the simulation owns accepted item identity/revision.
use bace_character::{ItemExperience, ItemExperienceChange};
use bace_magic::EnchantmentEntry;
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedItemSet {
    pub id: u32,
    /// Actual DAT tiers, preserving the authored spell order. An empty map is a
    /// verified missing DAT set, which ACE treats as no set spells.
    pub tiers: BTreeMap<u32, Vec<crate::PreparedGeneratorEnchantment>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedItemExperience {
    pub item: EntityId,
    pub actor: EntityId,
    pub name: String,
    /// None explicitly records HasItemLevel=false; absent preparation is an error.
    pub experience: Option<ItemExperience>,
    pub set: Option<PreparedItemSet>,
    pub set_uses_item_levels: bool,
    /// Accepted equipment insertion order, supplied by the equipment owner.
    pub equipment_order: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ItemExperienceRegistryChange {
    pub actor: EntityId,
    pub capacity: usize,
    pub before_revision: u64,
    pub before: Vec<EnchantmentEntry>,
    pub after_revision: u64,
    pub after: Vec<EnchantmentEntry>,
    pub events: Vec<crate::MagicEvent>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ItemExperienceReward {
    pub actor: EntityId,
    pub amount: u64,
    pub changes: Vec<(EntityId, ItemExperienceChange)>,
    pub registries: Vec<ItemExperienceRegistryChange>,
    pub events: Vec<ItemExperienceEvent>,
}
pub use bace_gameplay_api::item_experience::ItemExperienceEvent;
pub(crate) struct ItemExperienceState {
    pub(crate) items: BTreeMap<EntityId, PreparedItemExperience>,
    pub(crate) events: VecDeque<ItemExperienceEvent>,
    pub(crate) capacity: usize,
}
impl ItemExperienceState {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            items: BTreeMap::new(),
            events: VecDeque::new(),
            capacity: capacity.min(4096),
        }
    }
}

impl PreparedItemExperience {
    /// Pure preparation preflight for an incoming accepted inventory graph.
    /// The caller additionally verifies root ancestry and unique equipment order.
    pub fn validate(
        &self,
        item: &bace_inventory::InventoryItem,
    ) -> Result<(), bace_gameplay_api::InventoryRejection> {
        use bace_gameplay_api::InventoryRejection as E;
        let prepared = self;
        if self.item != item.id
            || self.actor.0 == 0
            || self.item == self.actor
            || self.name.len() > 1024
            || self.equipment_order == 0
        {
            return Err(E::InvalidState);
        }
        if let Some(xp) = prepared.experience
            && (xp.revision != item.revision || xp.level().is_err())
        {
            return Err(E::InvalidState);
        }
        if let Some(set) = &prepared.set {
            if set.id == 0 || set.tiers.len() > 256 || set.tiers.values().any(|v| v.len() > 256) {
                return Err(E::InvalidState);
            }
            for entries in set.tiers.values() {
                for entry in entries {
                    if entry.entry.caster != prepared.item.0
                        || entry.entry.spec.set_id != Some(set.id)
                        || !entry.entry.is_set_spell
                        || ![prepared.actor, prepared.item].contains(&entry.target)
                    {
                        return Err(E::InvalidState);
                    }
                    let mut registry =
                        bace_magic::EnchantmentRegistry::new(1).map_err(|_| E::InvalidState)?;
                    registry
                        .add(entry.entry.clone(), 0., true)
                        .map_err(|_| E::InvalidState)?;
                }
            }
        }
        Ok(())
    }
}
