//! Registry half of durable XP/vitae changes; progression supplies the exact f32.
use super::*;
impl Magic {
    pub(crate) fn register_vitae_template(
        &mut self,
        entry: EnchantmentEntry,
    ) -> Result<(), CastRejection> {
        EnchantmentRegistry::new(1)
            .map_err(|_| CastRejection::InvalidState)?
            .propose_vitae(1, Some(&entry), Some(0.95), None)
            .map_err(|_| CastRejection::InvalidState)?;
        self.vitae_template = Some(entry);
        Ok(())
    }
    pub(crate) fn prepare_vitae(
        &self,
        actor: EntityId,
        value: Option<f32>,
        remove_after: Option<f64>,
    ) -> Result<bace_magic::VitaeMutation, CastRejection> {
        let registry = self
            .registries
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        registry
            .propose_vitae(actor.0, self.vitae_template.as_ref(), value, remove_after)
            .map_err(|_| CastRejection::InvalidState)
    }
    pub(crate) fn adopt_vitae(
        &mut self,
        actor: EntityId,
        mutation: bace_magic::VitaeMutation,
        now: f64,
    ) -> Result<(), (CastRejection, Box<bace_magic::VitaeMutation>)> {
        if self.events.len() >= self.capacity || !now.is_finite() || now < 0.0 {
            return Err((CastRejection::Capacity, Box::new(mutation)));
        }
        let Some(registry) = self.registries.get_mut(&actor) else {
            return Err((CastRejection::MissingActor, Box::new(mutation)));
        };
        if registry.revision() != mutation.before_revision() {
            return Err((CastRejection::InvalidState, Box::new(mutation)));
        }
        let Some(sequence) = self
            .vitae_sequences
            .get(&actor)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
        else {
            return Err((CastRejection::InvalidState, Box::new(mutation)));
        };
        if mutation
            .remove_after_seconds()
            .is_some_and(|delay| !(now + delay).is_finite() || now + delay <= now)
        {
            return Err((CastRejection::InvalidState, Box::new(mutation)));
        }
        let delay = mutation.remove_after_seconds();
        let changed = mutation.before_revision() != mutation.after_revision();
        let entry = registry
            .adopt_vitae(mutation)
            .expect("prechecked registry revision");
        self.vitae_sequences.insert(actor, sequence);
        self.vitae_removals.remove(&actor);
        if let Some(delay) = delay {
            self.vitae_removals.insert(actor, (now + delay, sequence));
        }
        if changed {
            self.events.push_back(if let Some(entry) = entry {
                MagicEvent::Enchantment { actor, entry }
            } else {
                MagicEvent::EnchantmentsRemoved {
                    actor,
                    entries: vec![(666, 0)],
                }
            });
        }
        Ok(())
    }
    pub(super) fn service_vitae_removals(&mut self, now: f64) {
        let due: Vec<_> = self
            .vitae_removals
            .iter()
            .filter(|(_, v)| v.0 <= now)
            .map(|(&id, &(_, generation))| (id, generation))
            .take(self.capacity)
            .collect();
        for (actor, generation) in due {
            if self.events.len() >= self.capacity {
                break;
            }
            if self
                .registry_clocks
                .get(&actor)
                .is_none_or(|clock| clock.reserved || !clock.active)
            {
                continue;
            }
            if self.vitae_sequences.get(&actor) != Some(&generation) {
                self.vitae_removals.remove(&actor);
                continue;
            }
            let Some(registry) = self.registries.get(&actor) else {
                self.vitae_removals.remove(&actor);
                continue;
            };
            if registry
                .entries()
                .iter()
                .find(|e| e.spell == 666)
                .is_none_or(|e| e.spec.value < 0.9999)
            {
                self.vitae_removals.remove(&actor);
                continue;
            }
            let Ok(mutation) = self.prepare_vitae(actor, None, None) else {
                continue;
            };
            if self.adopt_vitae(actor, mutation, now).is_err() {
                break;
            }
        }
    }
}
impl Magic {
    pub(crate) fn validate_vitae_batch(
        &self,
        patches: &[(EntityId, &bace_magic::VitaeMutation)],
        now: f64,
    ) -> Result<(), CastRejection> {
        self.validate_vitae_batch_after(patches, &[], now)
    }
    pub(crate) fn validate_vitae_batch_after(
        &self,
        patches: &[(EntityId, &bace_magic::VitaeMutation)],
        items: &[crate::ItemExperienceRegistryChange],
        now: f64,
    ) -> Result<(), CastRejection> {
        if patches.len() > 4096 || !now.is_finite() || now < 0. {
            return Err(CastRejection::InvalidState);
        }
        let outputs = items
            .iter()
            .try_fold(
                patches
                    .iter()
                    .filter(|(_, p)| p.before_revision() != p.after_revision())
                    .count(),
                |n, p| n.checked_add(p.events.len()),
            )
            .ok_or(CastRejection::Capacity)?;
        if self.events.len() + outputs > self.capacity {
            return Err(CastRejection::Capacity);
        }
        for (i, (actor, patch)) in patches.iter().enumerate() {
            if patches[..i].iter().any(|(id, _)| id == actor)
                || items
                    .iter()
                    .find(|p| p.actor == *actor)
                    .map(|p| p.after_revision)
                    .or_else(|| self.registries.get(actor).map(|r| r.revision()))
                    != Some(patch.before_revision())
                || self
                    .vitae_sequences
                    .get(actor)
                    .is_some_and(|v| *v == u64::MAX)
                || patch
                    .remove_after_seconds()
                    .is_some_and(|d| !(now + d).is_finite() || now + d <= now)
            {
                return Err(CastRejection::InvalidState);
            }
        }
        Ok(())
    }
}
