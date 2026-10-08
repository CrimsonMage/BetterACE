//! Pure equipment effect proposals and receipt-gated adoption. The inventory
//! owner reserves every participant and performs the joint durable transition.
use super::*;
use crate::equipment_effects::*;
use bace_gameplay_api::InventoryRejection as E;
use bace_inventory::{InventoryItem, InventoryProposal, ItemPlace};
use bace_magic::EnchantmentRegistry;
use std::collections::{BTreeMap, BTreeSet};
mod registries;
use registries::*;
impl Kernel {
    pub fn prepare_equipment_effects(
        &self,
        actor: EntityId,
        proposal: &InventoryProposal,
        inputs: &PreparedEquipmentEffectInputs,
    ) -> Result<EquipmentEffectsPatch, E> {
        if inputs.actor != actor
            || inputs.items.len() > 128
            || proposal.changes.len() > 128
            || self
                .characters
                .get(actor)
                .is_none_or(|p| p.revision() != inputs.before_revision)
        {
            return Err(E::InvalidState);
        }
        let mut prepared = BTreeMap::new();
        for input in &inputs.items {
            if input.item == actor
                || input.experience.item != input.item
                || input.experience.actor != actor
                || prepared.insert(input.item, input).is_some()
                || input.removals.len() > 256
                || input.activations.len() > 256
            {
                return Err(E::InvalidState);
            }
            let fresh = proposal.changes.iter().find(|change| {
                change.before.is_none()
                    && change.after.id == input.item
                    && change.after.revision == 1
                    && input.before_revision == 0
            });
            let mut fresh_before = fresh.map(|change| change.after.clone());
            if let Some(item) = &mut fresh_before {
                item.revision = 0;
            }
            let item = self
                .inventory
                .item(input.item)
                .or(fresh_before.as_ref())
                .ok_or(E::MissingItem)?;
            if item.revision != input.before_revision {
                return Err(E::InvalidState);
            }
            input.experience.validate(item)?;
            if let Some(current) = self.item_experience.items.get(&input.item) {
                let mut expected = current.clone();
                if let Some(xp) = &mut expected.experience {
                    xp.revision = item.revision;
                }
                if expected != input.experience {
                    return Err(E::InvalidState);
                }
            }

            for entry in input.removals.iter().chain(&input.activations) {
                if entry.entry.caster != input.item.0
                    || ![actor, input.item].contains(&entry.target)
                {
                    return Err(E::InvalidState);
                }
            }
        }
        let mut equipped = Vec::new();
        for item in self.inventory.equipped_items(actor) {
            if !prepared.contains_key(&item.id) {
                return Err(E::InvalidState);
            }
            equipped.push(
                self.item_experience
                    .items
                    .get(&item.id)
                    .cloned()
                    .ok_or(E::InvalidState)?,
            );
        }
        if equipped.len() > 128 {
            return Err(E::Capacity);
        }
        equipped.sort_by_key(|p| p.equipment_order);
        if equipped
            .windows(2)
            .any(|w| w[0].equipment_order == w[1].equipment_order)
        {
            return Err(E::InvalidState);
        }
        let mut patch = EquipmentEffectsPatch {
            mana: Vec::new(),
            actor,
            before_revision: inputs.before_revision,
            registries: vec![],
            item_experience: vec![],
            properties: vec![],
            minimum_vital_maxima: None,
            activation_messages: vec![],
        };
        for change in &proposal.changes {
            let Some(before) = change.before.as_ref() else {
                if change.after.revision != 1 || !prepared.contains_key(&change.after.id) {
                    return Err(E::InvalidState);
                }
                continue;
            };
            if self.inventory.item(before.id) != Some(before)
                || change.after.id != before.id
                || change.after.revision != before.revision.checked_add(1).ok_or(E::Overflow)?
            {
                return Err(E::InvalidState);
            }
            let old = location(before, actor);
            let new = location(&change.after, actor);
            if old == 0 || new != 0 {
                continue;
            }
            let input = prepared.get(&before.id).ok_or(E::InvalidState)?;
            for entry in &input.removals {
                remove_spell(
                    &self.magic,
                    &mut patch.registries,
                    entry.target,
                    entry.entry.spell,
                    Some(before.id.0),
                    None,
                )?;
            }
            let previous = equipped.clone();
            equipped.retain(|p| p.item != before.id);
            update_set(
                &self.magic,
                &mut patch.registries,
                actor,
                &input.experience,
                &previous,
                &equipped,
                false,
                self.tick as f64 / 30.,
            )?;
            let registry = patch.registry(actor).map_err(|_| E::InvalidState)?;
            let registry = registry
                .as_ref()
                .or_else(|| self.magic.registry(actor))
                .ok_or(E::InvalidState)?;
            let gear = equipment_health(&equipped, &prepared)?;
            let inputs = self.vital_inputs.get(&actor).ok_or(E::InvalidState)?;
            let maxima = super::live_vitals::project_maxima(
                self.characters.get(actor).ok_or(E::InvalidState)?,
                registry,
                inputs.formulas,
                gear,
                self.characters
                    .native_services(actor)
                    .ok_or(E::InvalidState)?
                    .enlightenment,
            )
            .map_err(|_| E::InvalidState)?;
            patch.minimum_vital_maxima =
                Some(patch.minimum_vital_maxima.map_or(maxima, |previous| {
                    std::array::from_fn(|i| previous[i].min(maxima[i]))
                }));
            patch.properties.push(EquipmentItemPropertyChange {
                item: before.id,
                before_revision: before.revision,
                after_revision: change.after.revision,
                mana_before: input.current_mana,
                mana_after: input.current_mana,
                affecting_before: input.affecting,
                affecting_after: None,
            });
        }
        let mut order = self
            .item_experience
            .items
            .values()
            .filter(|p| p.actor == actor)
            .map(|p| p.equipment_order)
            .max()
            .unwrap_or(0);
        for change in &proposal.changes {
            let before = change.before.as_ref();
            let id = change.after.id;
            let old = before.map_or(0, |item| location(item, actor));
            let new = location(&change.after, actor);
            if (old == 0) == (new == 0) {
                continue;
            }
            let input = prepared.get(&id).ok_or(E::InvalidState)?;
            let owned_after = match change.after.place {
                ItemPlace::Contained { container, .. } => {
                    container == actor || self.inventory.owned(actor, container)
                }
                _ => false,
            };
            let mut metadata = owned_after.then(|| input.experience.clone());
            if let Some(metadata) = &mut metadata {
                if let Some(xp) = &mut metadata.experience {
                    xp.revision = change.after.revision;
                }
                if new != 0 {
                    order = order.checked_add(1).ok_or(E::Overflow)?;
                    metadata.equipment_order = order;
                }
            }
            patch.item_experience.push(EquipmentItemExperienceChange {
                item: id,
                before: self.item_experience.items.get(&id).cloned(),
                after: metadata.clone(),
            });
            if new == 0 {
                continue;
            }
            let property = if let Some(index) = patch.properties.iter().position(|p| p.item == id) {
                index
            } else {
                patch.properties.push(EquipmentItemPropertyChange {
                    item: id,
                    before_revision: before.map_or(0, |item| item.revision),
                    after_revision: change.after.revision,
                    mana_before: input.current_mana,
                    mana_after: input.current_mana,
                    affecting_before: input.affecting,
                    affecting_after: input.affecting,
                });
                patch.properties.len() - 1
            };
            if input.current_mana.is_some_and(|mana| mana > 0) {
                let registry = patch.registry(actor).map_err(|_| E::InvalidState)?;
                let registry = registry
                    .as_ref()
                    .or_else(|| self.magic.registry(actor))
                    .ok_or(E::InvalidState)?;
                let gear = equipment_health(&equipped, &prepared)?
                    .checked_add(input.gear_health.unwrap_or(0))
                    .ok_or(E::Overflow)?;
                let activation = self.check_inventory_item_activation_after(
                    actor,
                    &input.activation,
                    registry,
                    gear,
                );
                if activation == Err(bace_inventory::ActivationFailure::MissingValue) {
                    return Err(E::InvalidState);
                }
                if activation.is_ok() {
                    let (mana, affecting, cast_spells) =
                        crate::equipment_effects::activation_properties(
                            input.current_mana,
                            patch.properties[property].affecting_after,
                            true,
                        );
                    patch.properties[property].mana_after = mana;
                    patch.properties[property].affecting_after = affecting;
                    if cast_spells {
                        for entry in &input.activations {
                            add_spell(
                                &self.magic,
                                &mut patch.registries,
                                entry,
                                self.tick as f64 / 30.,
                            )?;
                        }
                    }
                } else if let Err(failure) = activation
                    && let Some(message) = failure.message(&input.experience.name)
                {
                    patch.activation_messages.push(message);
                }
            }
            let previous = equipped.clone();
            let metadata = metadata.ok_or(E::InvalidState)?;
            equipped.push(metadata.clone());
            update_set(
                &self.magic,
                &mut patch.registries,
                actor,
                &metadata,
                &previous,
                &equipped,
                true,
                self.tick as f64 / 30.,
            )?;
        }
        for property in &patch.properties {
            let input = prepared.get(&property.item).ok_or(E::InvalidState)?;
            let owner = self.equipment_mana.players.get(&actor);
            let before = owner.and_then(|p| p.items.get(&property.item)).cloned();
            if input.current_mana.is_some() && owner.is_none() {
                return Err(E::InvalidState);
            }
            let mut after = before.clone().or_else(|| input.mana.clone());
            if let Some(mana) = &mut after {
                if mana.current != property.mana_before
                    || mana.affecting != property.affecting_before
                    || mana.removal_remaining.is_some()
                {
                    return Err(E::InvalidState);
                }
                mana.current = property.mana_after;
                mana.affecting = property.affecting_after;
                if property.affecting_after == Some(true)
                    && property.mana_before.and_then(|n| n.checked_sub(1)) == property.mana_after
                {
                    mana.accumulator = 0.;
                    mana.warned = false;
                }
            }
            if let Some(metadata) = patch
                .item_experience
                .iter()
                .find(|m| m.item == property.item)
                && metadata.after.is_none()
            {
                after = None;
            }
            patch.mana.push((property.item, before, after));
        }
        patch
            .registries
            .retain(|p| p.before_revision != p.after_revision);
        Ok(patch)
    }
    pub fn validate_equipment_effects(
        &self,
        patch: &EquipmentEffectsPatch,
        proposal: &InventoryProposal,
    ) -> Result<(), E> {
        if self
            .characters
            .get(patch.actor)
            .is_none_or(|p| p.revision() != patch.before_revision)
            || patch.registries.len() > 129
            || patch.item_experience.len() > 128
            || patch.properties.len() > 128
        {
            return Err(E::InvalidState);
        }
        if patch.mana.len() > 128 || self.equipment_mana_pending(patch.actor) {
            return Err(E::DurabilityPending);
        }
        let mut mana_count = self
            .equipment_mana
            .players
            .values()
            .map(|p| p.items.len())
            .sum::<usize>();
        let mut mana_ids = BTreeSet::new();
        for (id, before, after) in &patch.mana {
            mana_count = mana_count
                .checked_sub(usize::from(before.is_some()))
                .and_then(|n| n.checked_add(usize::from(after.is_some())))
                .ok_or(E::Capacity)?;
            if !mana_ids.insert(*id)
                || self
                    .equipment_mana
                    .players
                    .get(&patch.actor)
                    .and_then(|p| p.items.get(id))
                    != before.as_ref()
                || !patch.properties.iter().any(|p| p.item == *id)
                || after.as_ref().is_some_and(|p| p.item != *id)
            {
                return Err(E::InvalidState);
            }
        }
        if mana_count > self.equipment_mana.capacity {
            return Err(E::Capacity);
        }
        let mut ids = BTreeSet::new();
        let mut count = self.item_experience.items.len();
        for change in &patch.item_experience {
            if !ids.insert(change.item)
                || self.item_experience.items.get(&change.item) != change.before.as_ref()
            {
                return Err(E::InvalidState);
            }
            count = count
                .checked_sub(usize::from(change.before.is_some()))
                .and_then(|n| n.checked_add(usize::from(change.after.is_some())))
                .ok_or(E::Capacity)?;
            if let Some(after) = &change.after {
                let item = proposal
                    .changes
                    .iter()
                    .find(|c| c.after.id == change.item)
                    .ok_or(E::InvalidState)?;
                after.validate(&item.after)?;
            }
        }
        if count > self.item_experience.capacity {
            return Err(E::Capacity);
        }
        ids.clear();
        for registry in &patch.registries {
            if !ids.insert(registry.actor)
                || (registry.actor != patch.actor
                    && !proposal.changes.iter().any(|change| {
                        change.after.id == registry.actor
                            && change.before.as_ref().is_some_and(|before| {
                                before.revision.checked_add(1) == Some(change.after.revision)
                            })
                    }))
            {
                return Err(E::InvalidState);
            }
        }
        ids.clear();
        for property in &patch.properties {
            if !ids.insert(property.item) {
                return Err(E::InvalidState);
            }
            if !proposal.changes.iter().any(|c| {
                c.after.id == property.item
                    && c.after.revision == property.after_revision
                    && c.before.as_ref().map_or(
                        property.before_revision == 0 && c.after.revision == 1,
                        |before| before.revision == property.before_revision,
                    )
            }) {
                return Err(E::InvalidState);
            }
        }
        self.magic
            .validate_equipment_registries(&patch.registries)
            .map_err(|_| E::DurabilityPending)
    }
    /// Call only after joint preflight and the exact durable inventory receipt,
    /// before releasing the registry reservations. No fallible mutation follows.
    pub(crate) fn adopt_equipment_effects(&mut self, patch: &EquipmentEffectsPatch) {
        self.magic.adopt_equipment_registries(&patch.registries);
        if let Some(owner) = self.equipment_mana.players.get_mut(&patch.actor) {
            for (id, _, after) in &patch.mana {
                if let Some(after) = after {
                    owner.items.insert(*id, after.clone());
                } else {
                    owner.items.remove(id);
                }
            }
        }

        for registry in &patch.registries {
            self.registry_revisions
                .insert(registry.actor, registry.after_revision);
        }
        for change in &patch.item_experience {
            if let Some(after) = &change.after {
                self.item_experience
                    .items
                    .insert(change.item, after.clone());
            } else {
                self.item_experience.items.remove(&change.item);
            }
        }
    }
}
fn location(item: &InventoryItem, actor: EntityId) -> u32 {
    match item.place {
        ItemPlace::Contained {
            container,
            equipped,
            ..
        } if container == actor => equipped,
        _ => 0,
    }
}

fn equipment_health(
    equipped: &[crate::PreparedItemExperience],
    inputs: &BTreeMap<EntityId, &PreparedEquipmentItemEffects>,
) -> Result<u32, E> {
    equipped.iter().try_fold(0u32, |sum, item| {
        sum.checked_add(
            inputs
                .get(&item.item)
                .ok_or(E::InvalidState)?
                .gear_health
                .unwrap_or(0),
        )
        .ok_or(E::Overflow)
    })
}
