//! Generated transient destruction is a bounded owner operation. Any already
//! durable descendant requires a durable tombstone workflow and stays retained.
use super::*;
use crate::GeneratorServiceError as G;
use bace_gameplay_api::GeneratorLifecycleEffect as Effect;
impl Kernel {
    pub(super) fn apply_generated_item_lifecycle(&mut self, effect: &Effect) -> Result<bool, G> {
        let (_generator, entity) = match effect {
            Effect::DestroyMember {
                generator, member, ..
            } => {
                if self.withdraw_generated_vendor_member(generator.entity, *member)? {
                    return Ok(true);
                }
                (*generator, member.entity)
            }
            Effect::DetachMember { .. }
            | Effect::SuppressedInitial { .. }
            | Effect::Invalidated { .. } => return Ok(true),
            _ => return Ok(false),
        };
        if self.inventory.item(entity).is_none() {
            return Ok(false);
        }
        let ids = match self.inventory.generated_tree(entity) {
            Ok(ids) => ids,
            Err(_) => {
                self.stage_generated_retirement(effect)?;
                return Err(G::Busy);
            }
        };
        let mut next = self.inventory.clone();
        if next.remove_generated_tree(&ids).is_err() {
            self.stage_generated_retirement(effect)?;
            return Err(G::Busy);
        }
        let mut reserved = Vec::new();
        let now = self.tick as f64 / 30.0;
        for id in &ids {
            if self.magic.registry(*id).is_none() {
                continue;
            }
            if self.magic.registry_reserved(*id)
                || self.magic.reserve_registry(*id, true, now).is_err()
            {
                for previous in reserved {
                    self.magic
                        .reserve_registry(previous, false, now)
                        .map_err(|_| G::Busy)?;
                }
                return Err(G::Busy);
            }
            reserved.push(*id);
        }
        if reserved
            .iter()
            .any(|id| self.magic.can_retire_item_registry(*id, now).is_err())
        {
            for previous in reserved {
                self.magic
                    .reserve_registry(previous, false, now)
                    .map_err(|_| G::Busy)?;
            }
            return Err(G::Busy);
        }
        for id in reserved {
            self.magic
                .retire_item_registry(id, now)
                .map_err(|_| G::Busy)?;
            self.registry_revisions.remove(&id);
        }
        for id in ids {
            if self.constructed_creatures.contains(id) {
                self.cancel_generated_enchantments(id);
                self.constructed_creatures.remove(id);
                next.forget_constructed(id);
            }
            self.world.remove(id);
        }
        self.inventory = next;
        Ok(true)
    }
}
