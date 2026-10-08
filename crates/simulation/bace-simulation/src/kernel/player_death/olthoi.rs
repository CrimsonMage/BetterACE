//! Separate source Olthoi death loot never consumes the victim's possessions.
use super::*;
use crate::{OlthoiDeathKind, PreparedPlayerDeath};
use bace_inventory::{InventoryView, ItemChange, ItemPlace};
impl Kernel {
    pub(super) fn olthoi_death_kind(
        &self,
        actor: EntityId,
        killer: Option<EntityId>,
    ) -> Option<OlthoiDeathKind> {
        let olthoi = |id| self.social_presence(id).is_some_and(|p| p.olthoi);
        if killer.is_some_and(olthoi) {
            Some(OlthoiDeathKind::Slag)
        } else if olthoi(actor) {
            Some(
                if killer.is_some_and(|id| {
                    id != actor && self.world.combatant(id).is_some_and(|c| c.profile().player)
                }) {
                    OlthoiDeathKind::Treasure
                } else {
                    OlthoiDeathKind::Empty
                },
            )
        } else {
            None
        }
    }
    pub(super) fn olthoi_death_inventory(
        &self,
        p: &PreparedPlayerDeath,
    ) -> Result<(bace_inventory::InventoryProposal, Vec<EntityId>), E> {
        let loot = p.olthoi.as_ref().ok_or(E::MissingAssets)?;
        let killer = self
            .player_deaths
            .pending
            .get(&p.actor)
            .and_then(|p| p.killer);
        let state = self
            .player_deaths
            .states
            .get(&p.actor)
            .ok_or(E::MissingAssets)?;
        let had_vitae = self
            .magic
            .registry(p.actor)
            .ok_or(E::MissingAssets)?
            .entries()
            .iter()
            .any(|e| e.spell == 666);
        if Some(loot.kind) != self.olthoi_death_kind(p.actor, killer)
            || loot.before_timestamp != state.olthoi_loot_timestamp
            || loot.had_vitae != had_vitae
            || loot.items.len() > 120
            || (had_vitae || loot.kind == OlthoiDeathKind::Empty) && !loot.items.is_empty()
            || loot.kind != OlthoiDeathKind::Slag && loot.after_timestamp != loot.before_timestamp
            || loot.kind == OlthoiDeathKind::Slag
                && (loot.items.len() > 1 || loot.items.iter().any(|i| i.template != 43491))
            || loot.kind == OlthoiDeathKind::Slag
                && (if loot.items.is_empty() {
                    loot.after_timestamp != loot.before_timestamp
                } else {
                    loot.after_timestamp.is_none()
                })
            || loot.after_timestamp.is_some_and(|t| t < 0)
        {
            return Err(E::Stale);
        }
        let mut seen = std::collections::BTreeSet::from([p.corpse.id]);
        let mut accepted_containers = std::collections::BTreeSet::from([p.corpse.id]);
        let container_ids: std::collections::BTreeSet<_> = loot
            .containers
            .iter()
            .map(|container| container.id)
            .collect();
        if container_ids.len() != loot.containers.len()
            || loot
                .containers
                .iter()
                .any(|container| container.revision != 0 || container.root_owner.is_some())
        {
            return Err(E::Invalid);
        }
        let mut changes = Vec::new();
        let mut roots = Vec::new();
        let mut depths: std::collections::BTreeMap<EntityId, usize> =
            std::collections::BTreeMap::new();
        for source in &loot.items {
            let ItemPlace::Contained {
                container,
                equipped: 0,
                ..
            } = source.place
            else {
                return Err(E::Invalid);
            };
            if !seen.insert(source.id)
                || self.world.contains_identity(source.id)
                || self.inventory.item(source.id).is_some()
                || source.revision != 0
                || !accepted_containers.contains(&container)
                || source.is_container != container_ids.contains(&source.id)
            {
                return Err(E::Invalid);
            }
            if container == p.corpse.id {
                roots.push(source.id);
            }
            let depth = if container == p.corpse.id {
                0usize
            } else {
                depths
                    .get(&container)
                    .and_then(|value| value.checked_add(1))
                    .ok_or(E::Invalid)?
            };
            if depth >= 16 {
                return Err(E::Invalid);
            }
            depths.insert(source.id, depth);
            if source.is_container {
                accepted_containers.insert(source.id);
            }
            changes.push(ItemChange {
                before: None,
                after: source.clone(),
            });
        }
        if container_ids
            .iter()
            .any(|id| !accepted_containers.contains(id))
        {
            return Err(E::Invalid);
        }
        // The immutable tree is topological, while ACE Contain insertion puts
        // the latest child at slot zero. Insert final sibling slots in ascending
        // order so proposal slot shifting preserves the authored final order.
        changes.sort_by_key(|change| {
            let (container, slot) = match change.after.place {
                ItemPlace::Contained {
                    container, slot, ..
                } => (container, slot),
                _ => (EntityId(0), 0),
            };
            (
                depths.get(&change.after.id).copied().unwrap_or(usize::MAX),
                container,
                change.after.pack_slot,
                slot,
                change.after.id,
            )
        });
        changes.push(ItemChange {
            before: None,
            after: p.corpse_item.clone(),
        });
        let items: Vec<_> = self.inventory.items().cloned().collect();
        let mut containers: Vec<_> = self.inventory.containers().copied().collect();
        containers.push(p.corpse_container);
        containers.extend(loot.containers.iter().copied());
        let proposal = bace_inventory::propose_item_changes(
            p.actor,
            changes,
            InventoryView {
                items: &items,
                containers: &containers,
            },
        )
        .map_err(|_| E::Invalid)?;
        Ok((proposal, roots))
    }
}
