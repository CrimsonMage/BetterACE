//! Equipped generator effects use the sole registry owner, with atomic output
//! admission and the same ACE stacking implementation as all other enchantments.
use super::*;
impl Magic {
    pub(crate) fn apply_generated_enchantments(
        &mut self,
        entries: &[crate::PreparedGeneratorEnchantment],
        now: f64,
    ) -> Result<(), CastRejection> {
        if entries.is_empty() {
            return Ok(());
        }
        if entries.len() > 4096 || !now.is_finite() || now < self.current_time {
            return Err(CastRejection::InvalidState);
        }
        if entries.len() > self.capacity - self.events.len() {
            return Err(CastRejection::Capacity);
        }
        // Held inventory/registry operations are common retry causes. Refuse
        // before allocating candidate maps while the same reservation persists.
        if entries.iter().any(|entry| {
            self.registry_clocks
                .get(&entry.target)
                .is_some_and(|clock| {
                    clock.reserved || clock.error.is_some() || clock.active && clock.next_due <= now
                })
        }) {
            return Err(CastRejection::Busy);
        }
        let targets: BTreeSet<_> = entries.iter().map(|entry| entry.target).collect();
        let new_count = targets
            .iter()
            .filter(|target| !self.registries.contains_key(target))
            .count();
        if self.registries.len() + new_count > 4096 {
            return Err(CastRejection::Capacity);
        }
        let mut candidates = BTreeMap::new();
        for target in targets {
            let candidate = if let Some(registry) = self.registries.get(&target) {
                EnchantmentRegistry::restore(
                    registry.capacity(),
                    registry.revision(),
                    registry.entries().to_vec(),
                )
                .map_err(|_| CastRejection::InvalidState)?
            } else {
                EnchantmentRegistry::new(
                    512.max(
                        entries
                            .iter()
                            .filter(|entry| entry.target == target)
                            .count(),
                    ),
                )
                .map_err(|_| CastRejection::InvalidState)?
            };
            candidates.insert(target, candidate);
        }
        let mut events = Vec::with_capacity(entries.len());
        for prepared in entries {
            let update = candidates
                .get_mut(&prepared.target)
                .ok_or(CastRejection::InvalidState)?
                .add(prepared.entry.clone(), now, true)
                .map_err(|error| {
                    if error == RegistryError::Capacity {
                        CastRejection::Capacity
                    } else {
                        CastRejection::InvalidState
                    }
                })?;
            events.push(MagicEvent::Enchantment {
                actor: prepared.target,
                entry: update.entry,
            });
        }
        for (target, registry) in candidates {
            self.registries.insert(target, registry);
            self.registry_clocks
                .entry(target)
                .or_insert(registry::RegistryClock {
                    active: true,
                    reserved: false,
                    next_due: now + 5.0,
                    error: None,
                });
        }
        self.events.extend(events);
        Ok(())
    }
}
