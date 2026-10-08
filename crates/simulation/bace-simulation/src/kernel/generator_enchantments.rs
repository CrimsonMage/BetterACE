//! Source TryWieldObject defers permanent item enchantments by 0.1 seconds.
//! Only immutable pending inputs are queued; Magic retains the sole registries.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedGeneratorEnchantment {
    pub target: EntityId,
    pub entry: bace_magic::EnchantmentEntry,
}
struct Pending {
    due_tick: u64,
    entries: Vec<PreparedGeneratorEnchantment>,
}
#[derive(Default)]
pub(super) struct GeneratedEnchantments {
    pending: BTreeMap<EntityId, Pending>,
    count: usize,
    scratch: Vec<EntityId>,
}
impl GeneratedEnchantments {
    pub(super) fn pending_for(&self, actor: EntityId) -> bool {
        self.pending.contains_key(&actor)
    }
    pub(super) fn has_state(&self) -> bool {
        !self.pending.is_empty()
    }
}
impl Kernel {
    pub(super) fn preflight_generated_enchantments(
        &self,
        actor: EntityId,
        entries: &[PreparedGeneratorEnchantment],
    ) -> Result<(), crate::GeneratorServiceError> {
        self.preflight_generated_enchantment_batch(std::iter::once((actor, entries)))
    }
    pub(super) fn preflight_generated_enchantment_batch<'a>(
        &self,
        input: impl Iterator<Item = (EntityId, &'a [PreparedGeneratorEnchantment])>,
    ) -> Result<(), crate::GeneratorServiceError> {
        use crate::GeneratorServiceError as G;
        let mut count = self.generated_enchantments.count;
        let mut actors = BTreeSet::new();
        for (actor, entries) in input {
            if entries.is_empty() {
                continue;
            }
            count = count.checked_add(entries.len()).ok_or(G::Capacity)?;
            if count > self.outcome_capacity.min(4096) {
                return Err(G::Capacity);
            }
            if actor.0 == 0
                || actor.0 == u32::MAX
                || !actors.insert(actor)
                || self.generated_enchantments.pending.contains_key(&actor)
                || self.tick.checked_add(3).is_none()
            {
                return Err(G::Invalid);
            }
            let mut uniqueness = BTreeSet::new();
            for prepared in entries {
                if prepared.entry.caster == 0
                    || prepared.entry.caster == u32::MAX
                    || prepared.target != actor && prepared.target.0 != prepared.entry.caster
                    || prepared.target.0 == 0
                    || prepared.target.0 == u32::MAX
                    || prepared.entry.caster == actor.0
                    || !uniqueness.insert((
                        prepared.target,
                        prepared.entry.caster,
                        prepared.entry.spell,
                    ))
                {
                    return Err(G::Invalid);
                }
                let mut check = bace_magic::EnchantmentRegistry::new(1).map_err(|_| G::Invalid)?;
                check
                    .add(prepared.entry.clone(), self.tick as f64 / 30.0, true)
                    .map_err(|_| G::Invalid)?;
            }
        }
        Ok(())
    }
    /// Admission called the read-only batch preflight before world mutation.
    pub(super) fn enqueue_generated_enchantments(
        &mut self,
        actor: EntityId,
        entries: Vec<PreparedGeneratorEnchantment>,
    ) {
        if entries.is_empty() {
            return;
        }
        self.generated_enchantments.count += entries.len();
        self.generated_enchantments.pending.insert(
            actor,
            Pending {
                due_tick: self.tick + 3,
                entries,
            },
        );
    }
    pub(super) fn cancel_generated_enchantments(&mut self, actor: EntityId) {
        if let Some(pending) = self.generated_enchantments.pending.remove(&actor) {
            self.generated_enchantments.count -= pending.entries.len();
        }
    }
    pub(super) fn step_generated_enchantments(
        &mut self,
        now_tick: u64,
    ) -> Result<(), SimulationError> {
        self.generated_enchantments.scratch.clear();
        self.generated_enchantments.scratch.extend(
            self.generated_enchantments
                .pending
                .iter()
                .filter_map(|(actor, pending)| (pending.due_tick <= now_tick).then_some(*actor)),
        );
        for index in 0..self.generated_enchantments.scratch.len() {
            let actor = self.generated_enchantments.scratch[index];
            if !self.world.contains_identity(actor) && !self.constructed_creatures.contains(actor) {
                self.cancel_generated_enchantments(actor);
                continue;
            }
            let pending = self
                .generated_enchantments
                .pending
                .get_mut(&actor)
                .expect("queued actor");
            let before = pending.entries.len();
            pending.entries.retain(|entry| self.inventory.item(EntityId(entry.entry.caster)).is_some_and(|item| matches!(item.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)));
            self.generated_enchantments.count -= before - pending.entries.len();
            if pending.entries.is_empty() {
                self.cancel_generated_enchantments(actor);
                continue;
            }
            let pending = &self.generated_enchantments.pending[&actor];
            match self
                .magic
                .apply_generated_enchantments(&pending.entries, now_tick as f64 / 30.0)
            {
                Ok(()) => {
                    for prepared in &pending.entries {
                        let revision = self
                            .magic
                            .registry(prepared.target)
                            .expect("applied registry")
                            .revision();
                        self.registry_revisions
                            .entry(prepared.target)
                            .or_insert(revision);
                    }
                    self.cancel_generated_enchantments(actor);
                }
                Err(
                    bace_gameplay_api::CastRejection::Capacity
                    | bace_gameplay_api::CastRejection::Busy,
                ) => {}
                Err(_) => return Err(SimulationError::AuxiliaryRevision),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "generator_enchantments/tests.rs"]
mod tests;
