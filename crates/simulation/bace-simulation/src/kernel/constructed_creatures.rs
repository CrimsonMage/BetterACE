//! A constructed Creature need not have entered World. Mutable items remain in
//! Inventory, delayed item effects in the existing queue, and registries in Magic.
mod forest;
use super::*;
use crate::{GeneratorItemAdmission, GeneratorServiceError as G, PreparedContainedCreature};
use bace_gameplay_api::{
    GeneratorDestination, GeneratorLocation, GeneratorSpawnIntent, GeneratorSpawnKey,
    GeneratorSpawnMember, GeneratorSpawnReceipt, GeneratorSpawnResult,
};
use bace_inventory::ItemPlace;
use std::collections::{BTreeMap, BTreeSet};
/// A cold reconstruction of an already durable Creature/Cow and its exact item
/// identities. The source aggregate is retained by the runtime save owner; this
/// projection never asks the generator to draw a second inventory forest.
#[derive(Clone)]
pub struct PreparedRestoredConstructedCreature {
    pub actor: EntityId,
    pub landblock: u16,
    pub intent: GeneratorSpawnIntent,
    pub template: crate::GeneratedNpcTemplate,
    pub children: Vec<EntityId>,
}
impl std::fmt::Debug for PreparedRestoredConstructedCreature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedRestoredConstructedCreature")
            .field("actor", &self.actor)
            .field("landblock", &self.landblock)
            .field("intent", &self.intent)
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}
#[derive(Default)]
pub(super) struct ConstructedCreatures {
    entries: BTreeMap<EntityId, ConstructedCreature>,
    items: usize,
}
struct ConstructedCreature {
    landblock: u16,
    intent: GeneratorSpawnIntent,
    template: crate::GeneratedNpcTemplate,
    children: Vec<EntityId>,
    durable: bool,
    acquired: bool,
}
impl ConstructedCreatures {
    pub(super) fn contains(&self, actor: EntityId) -> bool {
        self.entries.contains_key(&actor)
    }
    pub(super) fn has_state(&self) -> bool {
        !self.entries.is_empty()
    }
    pub(super) fn transient_in_region(&self, landblock: u16) -> bool {
        self.entries
            .values()
            .any(|entry| entry.landblock == landblock && !entry.acquired && !entry.durable)
    }
    pub(super) fn remove(&mut self, actor: EntityId) {
        if let Some(entry) = self.entries.remove(&actor) {
            self.items -= entry.children.len() + 1;
        }
    }
}
impl Kernel {
    pub(super) fn preflight_constructed_acquisition(
        &self,
        operation: u64,
        transient: &BTreeSet<EntityId>,
    ) -> Result<Vec<EntityId>, bace_gameplay_api::InventoryRejection> {
        use bace_gameplay_api::InventoryRejection as E;
        let ticket = self
            .inventory
            .pending_ticket(operation)
            .ok_or(E::InvalidState)?;
        let roots = self
            .inventory
            .constructed_acquisition_roots(operation)
            .ok_or(E::InvalidState)?;
        let changes: BTreeSet<_> = ticket.proposal.changes.iter().map(|c| c.after.id).collect();
        let mut covered = BTreeSet::new();
        for root in roots {
            let entry = self
                .constructed_creatures
                .entries
                .get(root)
                .ok_or(E::InvalidState)?;
            if entry.durable || entry.acquired || !transient.contains(root) {
                return Err(E::DurabilityPending);
            }
            let subtree: BTreeSet<_> = entry
                .children
                .iter()
                .copied()
                .chain(std::iter::once(*root))
                .collect();
            let actual: BTreeSet<_> = self.inventory.tree_members(*root)?.into_iter().collect();
            if subtree.len() != entry.children.len() + 1
                || subtree != actual
                || !subtree.is_subset(&changes)
                || !subtree.is_subset(transient)
            {
                return Err(E::InvalidState);
            }
            covered.extend(subtree);
        }
        if roots.is_empty()
            || transient
                .iter()
                .any(|id| self.inventory.constructed_ancestor(*id) && !covered.contains(id))
        {
            return Err(E::InvalidState);
        }
        Ok(roots.to_vec())
    }
    pub(super) fn adopt_constructed_acquisition(&mut self, roots: &[EntityId]) {
        for root in roots {
            let entry = self
                .constructed_creatures
                .entries
                .get_mut(root)
                .expect("preflighted constructed acquisition");
            entry.durable = true;
            entry.acquired = true;
        }
    }
    pub fn has_constructed_creature(&self, actor: EntityId) -> bool {
        self.constructed_creatures.contains(actor)
    }
    /// A saved region may detach its constructed owner only when the same exact
    /// durable item tree is included in that region's save-before-eviction hold.
    pub(super) fn preflight_restored_constructed_unload(
        &self,
        landblock: u16,
        saved: &BTreeSet<EntityId>,
    ) -> Result<(), G> {
        for (&actor, entry) in self
            .constructed_creatures
            .entries
            .iter()
            .filter(|(_, entry)| entry.landblock == landblock && !entry.acquired)
        {
            if !entry.durable
                || !saved.contains(&actor)
                || entry.children.iter().any(|id| !saved.contains(id))
                || self.generated_enchantments.pending_for(actor)
            {
                return Err(G::Busy);
            }
        }
        Ok(())
    }
    /// Called after the exact durable region receipt and inventory eviction.
    /// No creature may remain with a detached inventory owner.
    pub(super) fn retire_restored_constructed_region(&mut self, landblock: u16) {
        let actors: Vec<_> = self
            .constructed_creatures
            .entries
            .iter()
            .filter_map(|(&actor, entry)| {
                (entry.landblock == landblock && !entry.acquired).then_some(actor)
            })
            .collect();
        for actor in actors {
            self.constructed_creatures.remove(actor);
            self.inventory.forget_constructed(actor);
        }
    }
    /// Called before any region mutation. All item/physical identities and the
    /// exact descendant partition must agree with the cold decoded forest.
    pub(super) fn validate_restored_constructed_creatures(
        &self,
        landblock: u16,
        entries: &[PreparedRestoredConstructedCreature],
        items: &[bace_inventory::InventoryItem],
    ) -> Result<(), G> {
        if entries.len() > 128 || self.constructed_creatures.entries.len() + entries.len() > 128 {
            return Err(G::Capacity);
        }
        let indexed: BTreeMap<_, _> = items.iter().map(|item| (item.id, item)).collect();
        let all_actors: BTreeSet<_> = entries.iter().map(|entry| entry.actor).collect();
        let mut actors = BTreeSet::new();
        let mut count = 0usize;
        for entry in entries {
            if entry.actor.0 == 0
                || entry.landblock != landblock
                || !actors.insert(entry.actor)
                || self.constructed_creatures.contains(entry.actor)
                || self.world.contains_identity(entry.actor)
                || entry.intent.key.generator.entity.0 == 0
                || entry.intent.key.generator.incarnation == 0
                || entry.intent.key.generator.content_revision == 0
                || entry.intent.key.occurrence == 0
            {
                return Err(G::Invalid);
            }
            let root = indexed.get(&entry.actor).ok_or(G::Missing)?;
            if !root.is_container
                || root.stack != 1
                || !matches!(root.place, ItemPlace::Contained { equipped: 0, .. })
            {
                return Err(G::Invalid);
            }
            let profile = entry.template.physical.as_ref().ok_or(G::Missing)?;
            bace_combat::physical::validate_physical_profile(profile).map_err(|_| G::Invalid)?;
            if profile.player || entry.children.len() > 1023 {
                return Err(G::Invalid);
            }
            let mut actual = BTreeSet::new();
            let mut equipped = BTreeMap::new();
            for item in items {
                if item.id == entry.actor {
                    continue;
                }
                let mut current = item.id;
                let mut nested = false;
                for _ in 0..64 {
                    let Some(next) = indexed.get(&current) else {
                        break;
                    };
                    match next.place {
                        ItemPlace::Contained { container, .. } if container == entry.actor => {
                            actual.insert(item.id);
                            if !nested
                                && let ItemPlace::Contained {
                                    equipped: location, ..
                                } = item.place
                                && location != 0
                            {
                                equipped.insert(item.id, (item.revision, location));
                            }
                            break;
                        }
                        ItemPlace::Contained { container, .. } => {
                            nested |= all_actors.contains(&container);
                            current = container;
                        }
                        _ => break,
                    }
                }
            }
            let expected: BTreeSet<_> = entry.children.iter().copied().collect();
            if expected.len() != entry.children.len() || expected != actual {
                return Err(G::Invalid);
            }
            let stamps: BTreeMap<_, _> = profile
                .equipment
                .iter()
                .map(|stamp| (EntityId(stamp.entity), (stamp.revision, stamp.location)))
                .collect();
            if stamps.len() != profile.equipment.len() || stamps != equipped {
                return Err(G::Invalid);
            }
            count = count.checked_add(expected.len() + 1).ok_or(G::Capacity)?;
        }
        if self.constructed_creatures.items + count > 4096 {
            return Err(G::Capacity);
        }
        Ok(())
    }
    /// Runs only after the same region's inventory graph has been admitted.
    pub(super) fn adopt_restored_constructed_creatures(
        &mut self,
        entries: Vec<PreparedRestoredConstructedCreature>,
    ) {
        for entry in entries {
            self.inventory.mark_constructed(entry.actor);
            self.constructed_creatures.items += entry.children.len() + 1;
            self.constructed_creatures.entries.insert(
                entry.actor,
                ConstructedCreature {
                    landblock: entry.landblock,
                    intent: entry.intent,
                    template: entry.template,
                    children: entry.children,
                    durable: true,
                    acquired: false,
                },
            );
        }
    }
    pub fn admit_contained_creature(
        &mut self,
        key: GeneratorSpawnKey,
        prepared: PreparedContainedCreature,
    ) -> Result<GeneratorItemAdmission, G> {
        if !prepared.valid_bounds() {
            return Err(G::Invalid);
        }
        let actor = prepared.root.id;
        let mut items = vec![prepared.root];
        items.extend(prepared.loadout.items.iter().cloned());
        let containers = prepared.loadout.containers.clone();
        self.admit_contained_forest(
            key,
            crate::PreparedContainedForest {
                roots: vec![actor],
                items,
                containers,
                creatures: vec![crate::PreparedConstructedCreature {
                    entity: actor,
                    weenie_type: prepared.weenie_type,
                    loadout: prepared.loadout,
                }],
            },
        )
    }
    /// Trusted transient promotion. Player trade/drop must first gain a durable
    /// subtype companion; this method never bypasses an inventory reservation.
    pub fn promote_contained_creature(
        &mut self,
        actor: EntityId,
        expected_revision: u64,
        location: GeneratorLocation,
    ) -> Result<(), G> {
        let root = self.inventory.item(actor).ok_or(G::Missing)?;
        if root.revision != expected_revision {
            return Err(G::Stale);
        }
        self.inventory
            .preflight_constructed_promotion(actor)
            .map_err(|_| G::Busy)?;
        if self.generators.events.len() == self.generators.capacity {
            return Err(G::Capacity);
        }
        let saved = self
            .constructed_creatures
            .entries
            .get(&actor)
            .ok_or(G::Missing)?;
        // A cold-restored creature has durable item rows. Placement must first
        // commit their exact world/contained transition under a receipt; this
        // transient promotion entry point cannot erase those rows safely.
        if saved.durable {
            return Err(G::Busy);
        }
        let actual = self.inventory.generated_tree(actor).map_err(|_| G::Busy)?;
        if actual.len() != saved.children.len() + 1
            || saved.children.iter().any(|id| !actual.contains(id))
            || actual.iter().any(|id| self.magic.registry_reserved(*id))
        {
            return Err(G::Stale);
        }
        let mut template = saved.template.clone();
        let profile = std::sync::Arc::make_mut(template.physical.as_mut().ok_or(G::Missing)?);
        for stamp in &mut profile.equipment {
            stamp.revision = self
                .inventory
                .item(EntityId(stamp.entity))
                .ok_or(G::Missing)?
                .revision;
        }
        for weapon in [
            &mut profile.main,
            &mut profile.offhand,
            &mut profile.launcher,
            &mut profile.ammunition,
            &mut profile.gloves,
            &mut profile.boots,
        ]
        .into_iter()
        .flatten()
        {
            weapon.revision = self
                .inventory
                .item(EntityId(weapon.entity))
                .ok_or(G::Missing)?
                .revision;
        }
        if !self
            .combat
            .proposed_equipment_current(profile, actor, &self.inventory)
        {
            return Err(G::Stale);
        }
        let mut intent = saved.intent.clone();
        intent.destination = GeneratorDestination::Specific(location);
        let template_id = root.template;
        // Placement failure leaves construction, inventory, queues and registries
        // unchanged. The existing NPC stage rolls back any partial physical admission.
        let accepted = self.stage_generator_npc(&intent, actor, template, 0)?;
        let birth = match self.prepare_generator_birth(actor) {
            Ok(birth) => birth,
            Err(error) => {
                self.rollback_staged_generator_npc(actor)?;
                return Err(error);
            }
        };
        self.inventory.adopt_constructed_promotion(actor);
        self.constructed_creatures.remove(actor);
        self.generators
            .events
            .push_back(crate::GeneratorWorldEvent::Spawned {
                birth,
                key: intent.key,
                entity: actor,
                template: template_id,
                location: accepted,
            });
        Ok(())
    }
}
