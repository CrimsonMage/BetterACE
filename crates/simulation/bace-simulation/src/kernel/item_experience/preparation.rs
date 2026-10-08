//! Pinned Player_Xp.GrantItemXP and Player_Spells.OnItemLevelUp ordering.
use super::*;
use bace_magic::EnchantmentRegistry;
impl Kernel {
    pub fn prepare_item_experience(
        &self,
        actor: EntityId,
        amount: u64,
    ) -> Result<ItemExperienceReward, E> {
        if amount > i64::MAX as u64 {
            return Err(E::Overflow);
        }
        let mut equipped = Vec::new();
        for item in self.inventory.equipped_items(actor) {
            let prepared = self
                .item_experience
                .items
                .get(&item.id)
                .filter(|p| p.actor == actor)
                .ok_or(E::InvalidState)?;
            equipped.push((item, prepared));
        }
        equipped.sort_by_key(|(_, p)| p.equipment_order);
        if equipped
            .windows(2)
            .any(|w| w[0].1.equipment_order == w[1].1.equipment_order)
        {
            return Err(E::InvalidState);
        }
        let mut reward = ItemExperienceReward {
            actor,
            amount,
            changes: Vec::new(),
            registries: Vec::new(),
            events: Vec::new(),
        };
        let mut levels = std::collections::BTreeMap::new();
        for (item, p) in &equipped {
            levels.insert(
                item.id,
                p.experience
                    .map(|xp| xp.level())
                    .transpose()
                    .map_err(|_| E::InvalidState)?
                    .unwrap_or(0),
            );
        }
        for (item, prepared) in &equipped {
            let Some(mut before) = prepared.experience else {
                continue;
            };
            before.revision = item.revision;
            let change = before.propose_xp(amount).map_err(|_| E::Overflow)?;
            if change.added == 0 {
                continue;
            }
            levels.insert(item.id, change.after_level);
            reward.changes.push((item.id, change));
            reward.events.push(ItemExperienceEvent {
                actor,
                item: item.id,
                total: change.after.total,
                level_up: (change.after_level > change.before_level)
                    .then(|| (prepared.name.clone(), change.after_level)),
            });
            if change.after_level == change.before_level {
                continue;
            }
            let Some(set) = &prepared.set else {
                continue;
            };
            let members: Vec<_> = equipped
                .iter()
                .filter(|(_, p)| p.set.as_ref().is_some_and(|s| s.id == set.id))
                .collect();
            if members.iter().any(|(_, p)| {
                p.set
                    .as_ref()
                    .is_some_and(|s| s.tiers.keys().ne(set.tiers.keys()))
            }) {
                return Err(E::InvalidState);
            }
            let level = if members.first().is_some_and(|(_, p)| p.set_uses_item_levels) {
                members.iter().try_fold(0u32, |n, (item, _)| {
                    n.checked_add(levels[&item.id]).ok_or(E::Overflow)
                })?
            } else {
                members.len() as u32
            };
            let previous = if members.first().is_some_and(|(_, p)| p.set_uses_item_levels) {
                level
                    .checked_sub(change.after_level - change.before_level)
                    .ok_or(E::InvalidState)?
            } else {
                level
            };
            let previous_spells = tier(set, previous);
            let spells = tier(set, level);
            for old in previous_spells
                .iter()
                .filter(|old| !spells.iter().any(|new| new.entry.spell == old.entry.spell))
            {
                // ACE removes the currently registered spell/set pair on the
                // player, even if another item was the original caster.
                let candidate = registry_candidate(&self.magic, &mut reward.registries, actor)?;
                let selected: Vec<_> = candidate
                    .after
                    .iter()
                    .filter(|e| e.spell == old.entry.spell && e.spec.set_id == Some(set.id))
                    .map(|e| (e.spell, e.spec.layer))
                    .collect();
                if selected.is_empty() {
                    continue;
                }
                let mut registry = restore(candidate)?;
                registry.remove(&selected).map_err(|_| E::InvalidState)?;
                candidate.after_revision = registry.revision();
                candidate.after = registry.into_entries();
                candidate
                    .events
                    .push(crate::MagicEvent::EnchantmentsRemoved {
                        actor,
                        entries: selected,
                    });
            }
            for new in spells.iter().filter(|new| {
                !previous_spells
                    .iter()
                    .any(|old| old.entry.spell == new.entry.spell)
            }) {
                let candidate =
                    registry_candidate(&self.magic, &mut reward.registries, new.target)?;
                let mut registry = restore(candidate)?;
                let applied = registry
                    .add(new.entry.clone(), self.tick as f64 / 30., true)
                    .map_err(|_| E::InvalidState)?;
                candidate.events.push(crate::MagicEvent::Enchantment {
                    actor: new.target,
                    entry: applied.entry,
                });
                candidate.after_revision = registry.revision();
                candidate.after = registry.into_entries();
            }
        }
        Ok(reward)
    }
}
fn tier(set: &PreparedItemSet, level: u32) -> &[crate::PreparedGeneratorEnchantment] {
    set.tiers
        .range(..=level)
        .next_back()
        .map_or(&[], |(_, entries)| entries)
}
fn restore(patch: &ItemExperienceRegistryChange) -> Result<EnchantmentRegistry, E> {
    EnchantmentRegistry::restore(patch.capacity, patch.after_revision, patch.after.clone())
        .map_err(|_| E::InvalidState)
}
fn registry_candidate<'a>(
    magic: &crate::magic::Magic,
    patches: &'a mut Vec<ItemExperienceRegistryChange>,
    actor: EntityId,
) -> Result<&'a mut ItemExperienceRegistryChange, E> {
    let index = if let Some(index) = patches.iter().position(|p| p.actor == actor) {
        index
    } else {
        let registry = magic.registry(actor).ok_or(E::InvalidState)?;
        patches.push(ItemExperienceRegistryChange {
            actor,
            capacity: registry.capacity(),
            before_revision: registry.revision(),
            before: registry.entries().to_vec(),
            after_revision: registry.revision(),
            after: registry.entries().to_vec(),
            events: Vec::new(),
        });
        patches.len() - 1
    };
    Ok(&mut patches[index])
}
