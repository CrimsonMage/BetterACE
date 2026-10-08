//! Item spells use accepted inventory containment and the real owning actor for
//! geometry/PK checks. An item registry never creates a fake creature/body.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ItemSpellTarget {
    pub owner: EntityId,
    pub revision: u64,
    pub equipped: u32,
    pub stack_limit: u32,
    pub wield_allowed: bool,
}
impl Magic {
    pub(crate) fn validate_item_spell_targets(
        &self,
        targets: &[(EntityId, u32, u32, bool)],
    ) -> Result<(), CastRejection> {
        if targets.len() > 4096 {
            return Err(CastRejection::Capacity);
        }
        let mut seen = BTreeSet::new();
        for &(id, kind, resistance, _) in targets {
            if id.0 == 0
                || kind == 0
                || resistance > i32::MAX as u32
                || self.item_types.contains_key(&id)
                || !seen.insert(id)
            {
                return Err(CastRejection::InvalidState);
            }
        }
        Ok(())
    }
    pub(crate) fn register_item_type(
        &mut self,
        item: EntityId,
        item_type: u32,
    ) -> Result<(), CastRejection> {
        if item.0 == 0 || item_type == 0 || !self.registries.contains_key(&item) {
            return Err(CastRejection::InvalidState);
        }
        self.item_types.insert(item, item_type);
        Ok(())
    }
    pub(crate) fn register_item_spell_quality(
        &mut self,
        item: EntityId,
        resist_magic: u32,
        non_projectile_immune: bool,
    ) -> Result<(), CastRejection> {
        if resist_magic > i32::MAX as u32 || !self.item_types.contains_key(&item) {
            return Err(CastRejection::InvalidState);
        }
        self.item_spell_qualities
            .insert(item, (resist_magic, non_projectile_immune));
        Ok(())
    }
    pub(crate) fn register_spell_target_mask(
        &mut self,
        spell: u32,
        mask: u32,
    ) -> Result<(), CastRejection> {
        if !self.spells.contains_key(&spell) {
            return Err(CastRejection::UnknownSpell);
        }
        self.spell_target_masks.insert(spell, mask);
        Ok(())
    }
    pub(crate) fn refresh_item_targets(
        &mut self,
        inventory: &crate::inventory::Inventory,
        incoming: Option<EntityId>,
    ) {
        self.item_targets.retain(|id, _| {
            incoming == Some(*id)
                || self
                    .attempts
                    .values()
                    .chain(self.instant_continuations.values())
                    .any(|a| a.target == Some(*id))
        });
        if let Some(id) = incoming
            && inventory.item(id).is_some()
            && self.registries.contains_key(&id)
        {
            self.item_targets.entry(id).or_insert(None);
        }
        for (id, projection) in &mut self.item_targets {
            *projection = None;
            let Some(item) = inventory.item(*id) else {
                continue;
            };
            if inventory.reserved(*id) {
                continue;
            }
            let mut place = item.place;
            for _ in 0..64 {
                let bace_inventory::ItemPlace::Contained { container, .. } = place else {
                    break;
                };
                if let Some(owner) = inventory
                    .containers()
                    .find(|c| c.id == container)
                    .and_then(|c| c.root_owner)
                {
                    let equipped = match item.place {
                        bace_inventory::ItemPlace::Contained {
                            container,
                            equipped,
                            ..
                        } if container == owner => equipped,
                        _ => 0,
                    };
                    *projection = Some(ItemSpellTarget {
                        owner,
                        revision: item.revision,
                        equipped,
                        stack_limit: item.maximum_stack,
                        wield_allowed: item.wield_requirements_met,
                    });
                    break;
                }
                let Some(parent) = inventory.item(container) else {
                    break;
                };
                place = parent.place;
            }
        }
    }
    pub(super) fn item_target_for(
        &self,
        actor: EntityId,
        target: Option<EntityId>,
        spell: &PreparedSpell,
        world: &World,
    ) -> Result<Option<ItemSpellTarget>, CastRejection> {
        let Some(target) = target else {
            return Ok(None);
        };
        if world.actor_state(target).is_ok() {
            return Ok(None);
        }
        if spell.school != bace_magic::MagicSchool::Item
            || !matches!(
                spell.effect,
                SpellEffect::Enchantment(_) | SpellEffect::Dispel(_)
            )
        {
            return Err(CastRejection::InvalidTarget);
        }
        let item = self
            .item_targets
            .get(&target)
            .copied()
            .flatten()
            .ok_or(CastRejection::OwnershipMismatch)?;
        let mask = *self
            .spell_target_masks
            .get(&spell.id)
            .ok_or(CastRejection::MissingAssets)?;
        let kind = *self
            .item_types
            .get(&target)
            .ok_or(CastRejection::MissingAssets)?;
        if !self.item_spell_qualities.contains_key(&target) {
            return Err(CastRejection::MissingAssets);
        }
        if mask & kind == 0
            || item.stack_limit > 1
            || !item.wield_allowed
            || item.owner != actor && item.equipped == 0
        {
            return Err(CastRejection::InvalidTarget);
        }
        if !self.registries.contains_key(&target) || world.actor_state(item.owner).is_err() {
            return Err(CastRejection::MissingAssets);
        }
        Ok(Some(item))
    }
    pub(super) fn observed_target(
        &self,
        attempt: &Attempt,
        world: &World,
    ) -> Result<Option<EntityId>, CastRejection> {
        if let Some(expected) = attempt.item_target {
            let current = self.item_target_for(
                attempt.origin.actor(),
                attempt.target,
                &attempt.prepared.spell,
                world,
            )?;
            if current != Some(expected) {
                return Err(CastRejection::OwnershipMismatch);
            }
            Ok(Some(expected.owner))
        } else {
            Ok(attempt.target)
        }
    }
}
