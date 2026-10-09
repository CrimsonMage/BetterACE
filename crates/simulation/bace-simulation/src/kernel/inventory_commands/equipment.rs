//! Inventory, character, registries and vitals share one durable equipment hold.
use super::*;
use bace_entity::EntityVital;
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken};
use bace_world::{VitalReservationDomain, VitalReservationToken};
#[cfg(test)]
mod tests;
fn token(operation: u64) -> VitalReservationToken {
    VitalReservationToken {
        domain: VitalReservationDomain::Equipment,
        operation,
    }
}
/// Pinned ACE Player_Inventory.TryShuffleStance/RequiresStanceSwap at
/// 47edade3bd3f6044b676d4eb877c4965c7eda62b. Only selectable slots
/// change the active combat mode; missile ammo has a special dequip rule.
fn equipment_mode_after(
    request: InventoryRequest,
    before_location: u32,
    mode: u32,
    slots: &[bace_inventory::WieldSlotItem],
) -> Option<u32> {
    if mode == 1 {
        return None;
    }
    let thrown = slots.iter().any(|slot| {
        slot.location != before_location
            && slot.style == Some(0x80)
            && matches!(slot.location, 0x100000 | 0x400000 | 0x2000000)
    });
    match request {
        InventoryRequest::Equip { location, .. }
        | InventoryRequest::SplitToWield { location, .. } => match location {
            0x400000 => Some(4),
            0x1000000 => Some(8),
            0x200000 if thrown => Some(4),
            0x100000 | 0x200000 | 0x2000000 => Some(2),
            _ => None,
        },
        InventoryRequest::Move { .. } => match before_location {
            0x800000 if mode == 4 => Some(1),
            0x100000 | 0x200000 | 0x400000 | 0x1000000 | 0x2000000 => {
                Some(if thrown { 4 } else { 2 })
            }
            _ => None,
        },
        _ => None,
    }
}
fn validate_dequip_slot_roster(
    inventory: &crate::inventory::Inventory,
    actor: EntityId,
    source: EntityId,
    slots: &[bace_inventory::WieldSlotItem],
) -> Result<(), E> {
    let mut ids = std::collections::BTreeSet::new();
    for metadata in slots {
        if !ids.insert(metadata.item) || metadata.location == 0 {
            return Err(E::InvalidState);
        }
        let accepted = inventory.item(metadata.item).ok_or(E::MissingItem)?;
        if !inventory.owned(actor, metadata.item)
            || accepted.revision != metadata.revision
            || !matches!(
                accepted.place,
                ItemPlace::Contained { container, equipped, .. }
                    if container == actor && equipped == metadata.location
            )
        {
            return Err(E::InvalidState);
        }
    }
    if !ids.contains(&source)
        || inventory
            .equipped_items(actor)
            .any(|item| !ids.contains(&item.id))
    {
        return Err(E::InvalidState);
    }
    Ok(())
}
impl Kernel {
    pub(super) fn prepare_inventory_equipment(
        &mut self,
        correlation: u64,
        mut p: crate::PreparedEquipmentRequest,
    ) -> Result<InventoryDecision, E> {
        let actor = p.request.context.actor;
        if self.inventory_commands.pending.len() >= self.inventory_commands.capacity
            || p.request.drop.is_some()
        {
            return Err(E::Capacity);
        }
        let item = match p.request.request {
            InventoryRequest::Equip { item, .. } => item,
            InventoryRequest::SplitToWield { item, .. } => item,
            InventoryRequest::Move {
                item, container, ..
            } => {
                self.validate_owned_inventory_path(actor, container)?;
                if self.inventory.item(item).is_none_or(|i|!matches!(i.place,ItemPlace::Contained{container,equipped,..}if container==actor&&equipped!=0)){return Err(E::InvalidEquip);}
                item
            }
            _ => return Err(E::InvalidEquip),
        };
        let split_target = p.request.split.as_ref().map(|split| split.fresh.id);
        if matches!(p.request.request, InventoryRequest::SplitToWield { .. })
            != split_target.is_some()
        {
            return Err(E::InvalidState);
        }
        if let Some(id) = split_target {
            self.validate_split_identity(id)?;
            if p.request.authority.new_item != Some(id) {
                return Err(E::InvalidState);
            }
        }
        if !self.inventory.owned(actor, item) {
            return Err(E::OwnershipMismatch);
        }
        if self.characters.reserved(actor)
            || self.combat.active(actor)
            || self.magic.busy(actor)
            || self.world.motion_busy(actor)
            || self.recall_busy(actor)
            || self.portals.reserved(actor)
            || self.world.is_in_portal_transit(actor)
            || self.world.has_reserved_vitals(actor)
        {
            return Err(E::Busy);
        }
        let combat_mode = self.world.combatant(actor).ok_or(E::InvalidState)?.mode();
        let before_location = match self.inventory.item(item).ok_or(E::MissingItem)?.place {
            ItemPlace::Contained { equipped, .. } => equipped,
            _ => return Err(E::InvalidEquip),
        };
        if matches!(p.request.request, InventoryRequest::Move { .. }) {
            validate_dequip_slot_roster(&self.inventory, actor, item, &p.slots)?;
        }
        let needs_pretransition = combat_mode != 1
            && matches!(p.request.request, InventoryRequest::Move { .. })
            && matches!(
                before_location,
                0x100000 | 0x200000 | 0x400000 | 0x1000000 | 0x2000000
            );
        let stance = p.stance.take();
        if needs_pretransition
            && self
                .world
                .source_motion_state(actor)
                .is_some_and(|s| s.style != 0x8000_003c)
                != stance.is_some()
            || !needs_pretransition && stance.is_some()
            || stance.as_ref().is_some_and(|chain| {
                chain.motion != 0x8000_003c
                    || chain.speed != 1.
                    || chain.source_transition().is_none_or(|source| {
                        self.world.source_motion_state(actor) != Some(source.before)
                            || source.after.style != 0x8000_003c
                            || source.continues_cycle
                    })
            })
        {
            return Err(E::Busy);
        }
        let stance_epoch = if stance.is_some() {
            Some(
                self.world
                    .body(actor)
                    .map_err(|_| E::NotBound)?
                    .accepted()
                    .epoch(),
            )
        } else {
            None
        };
        let stance_deadline = self.tick.checked_add(450).ok_or(E::Overflow)?;
        self.prepare_inventory_time()?;
        if self
            .characters
            .get(actor)
            .is_none_or(|c| c.revision() != p.effects.before_revision)
            || p.effects.actor != actor
        {
            return Err(E::InvalidState);
        }
        if matches!(
            p.request.request,
            InventoryRequest::Equip { .. } | InventoryRequest::SplitToWield { .. }
        ) {
            self.check_inventory_wield_requirements(actor, p.wield)
                .map_err(|_| E::Requirements)?;
        }
        if let InventoryRequest::Equip { item, location }
        | InventoryRequest::SplitToWield { item, location, .. } = p.request.request
        {
            let mut ids = std::collections::BTreeSet::new();
            for metadata in &p.slots {
                if !ids.insert(metadata.item) {
                    return Err(E::InvalidState);
                }
                if Some(metadata.item) == split_target {
                    let fresh = &p.request.split.as_ref().ok_or(E::InvalidState)?.fresh;
                    if metadata.revision != fresh.revision
                        || metadata.location != location
                        || fresh.place
                            != (ItemPlace::Contained {
                                container: actor,
                                slot: 0,
                                equipped: location,
                            })
                    {
                        return Err(E::InvalidState);
                    }
                    continue;
                }
                let accepted = self.inventory.item(metadata.item).ok_or(E::MissingItem)?;
                if !self.inventory.owned(actor, metadata.item)
                    || accepted.revision != metadata.revision
                    || match accepted.place {
                        ItemPlace::Contained { equipped, .. } => equipped != metadata.location,
                        _ => true,
                    }
                {
                    return Err(E::InvalidState);
                }
            }
            if self
                .inventory
                .equipped_items(actor)
                .any(|i| !ids.contains(&i.id))
                || p.slots
                    .iter()
                    .any(|m| m.location == 0 && m.item != item && Some(m.item) != split_target)
            {
                return Err(E::InvalidState);
            }
            let target = p
                .slots
                .iter()
                .find(|m| m.item == split_target.unwrap_or(item))
                .ok_or(E::InvalidState)?;
            let gear = p
                .slots
                .iter()
                .filter(|m| m.location != 0 && m.item != target.item)
                .cloned()
                .collect::<Vec<_>>();
            let flags = match self
                .world
                .properties(actor)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 322))
            {
                Some(bace_entity::PropertyValue::Int(v)) => *v as u32,
                _ => 0,
            };
            let mode = self.world.combatant(actor).ok_or(E::InvalidState)?.mode();
            let location = bace_inventory::check_wield_slots(target, location, &gear, mode, flags)
                .map_err(|_| E::InvalidEquip)?;
            p.request.request = match p.request.request {
                InventoryRequest::SplitToWield { amount, .. } => InventoryRequest::SplitToWield {
                    item,
                    amount,
                    location,
                },
                _ => InventoryRequest::Equip { item, location },
            };
        }
        let mode_after =
            equipment_mode_after(p.request.request, before_location, combat_mode, &p.slots);
        p.request.authority.in_range = true;
        p.request.authority.clear_path = true;
        p.request.authority.geometry_ready = true;
        p.request.authority.source_view = None;
        p.request.authority.destination_view = None;
        p.request.authority.actor = actor;
        self.authorize_inventory(p.request.context, p.request.authority)?;
        let split = p.request.split.take();
        let operation = self.stage_inventory(actor, |inventory| {
            inventory.apply_equipment(p.request.request, p.request.authority, split)
        })?;
        let binding = CharacterBinding {
            actor,
            session: p.request.context.session,
            account: p.request.context.account,
        };
        let result = (|| {
            self.characters
                .reserve_equipment(actor, operation)
                .map_err(|_| E::DurabilityPending)?;
            self.world
                .reserve_vitals(
                    &[EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
                        .map(|v| (actor, v)),
                    token(operation),
                )
                .map_err(|_| E::DurabilityPending)?;
            let mut value =
                self.capture_inventory_operation(binding, operation, p.request.request)?;
            let effects =
                self.prepare_equipment_effects(actor, &value.ticket.proposal, &p.effects)?;
            self.validate_equipment_effects(&effects, &value.ticket.proposal)?;
            let effect_target = split_target.unwrap_or(item);
            value.equipment_health_update = p
                .effects
                .items
                .iter()
                .any(|i| i.item == effect_target && i.gear_health.is_some())
                && value.ticket.proposal.changes.iter().any(|c| {
                    c.after.id == effect_target
                        && (c.before.is_none()
                            || c.before.as_ref().is_some_and(|b| {
                                matches!(b.place, ItemPlace::Contained { equipped: 0, .. })
                            }) != matches!(
                                c.after.place,
                                ItemPlace::Contained { equipped: 0, .. }
                            ))
                });
            value.equipment = Some(std::sync::Arc::new(effects));
            value.equipment_mode_after = mode_after;
            self.inventory.claim(operation)?;
            self.inventory_commands
                .pending
                .insert(operation, value.clone());
            Ok(value)
        })();
        if result.is_err() {
            if self.characters.equipment_operation(actor) == Some(operation) {
                self.characters.finish_equipment(actor, operation, false);
            }
            self.world.release_vitals(token(operation));
            self.reject_inventory_inner(operation)?;
        }
        let value = result?;
        if let Some(stance) = stance {
            let source_style = stance
                .source_transition()
                .expect("preflighted equipment stance")
                .before
                .style;
            let motion_token = MotionToken {
                domain: MotionDomain::Inventory,
                owner: correlation,
                sequence: 1,
            };
            if self.inventory_commands.equipment_stances.len() >= self.inventory_commands.capacity
                || self
                    .world
                    .begin_motion(actor, motion_token, stance)
                    .is_err()
            {
                self.reject_inventory_inner(value.ticket.operation)?;
                self.inventory_commands
                    .pending
                    .remove(&value.ticket.operation);
                return Err(E::Busy);
            }
            self.inventory_commands.equipment_stances.insert(
                correlation,
                EquipmentStance {
                    actor,
                    operation: value.ticket.operation,
                    token: motion_token,
                    epoch: stance_epoch.expect("prepared stance epoch"),
                    deadline: stance_deadline,
                },
            );
            Ok(InventoryDecision::Motion(crate::InventoryMotion {
                actor,
                style: source_style,
                command: Some(0x8000_003c),
            }))
        } else {
            Ok(InventoryDecision::Proposed(Box::new(value)))
        }
    }
    pub(in crate::kernel) fn step_inventory_equipment_stances(&mut self) {
        let ids: Vec<_> = self
            .inventory_commands
            .equipment_stances
            .keys()
            .copied()
            .take(64)
            .collect();
        for id in ids {
            if self.inventory_commands.outcomes.len() >= self.inventory_commands.capacity {
                break;
            }
            let hold = &self.inventory_commands.equipment_stances[&id];
            let actor = hold.actor;
            let motion_token = hold.token;
            let invalid = self.tick >= hold.deadline
                || self.world.body(actor).map(|b| b.accepted().epoch()).ok() != Some(hold.epoch);
            let mut completed = false;
            let mut cancelled = invalid;
            for _ in 0..128 {
                let Some(event) = self.world.take_motion_event_matching(actor, motion_token) else {
                    break;
                };
                match event.event {
                    MotionExecutionEvent::Completed => {
                        completed = true;
                        break;
                    }
                    MotionExecutionEvent::Cancelled => {
                        cancelled = true;
                        break;
                    }
                    _ => {}
                }
            }
            if !completed && !cancelled {
                continue;
            }
            let hold = self
                .inventory_commands
                .equipment_stances
                .remove(&id)
                .expect("selected equipment stance");
            let result = if completed
                && !cancelled
                && self
                    .world
                    .source_motion_state(actor)
                    .is_some_and(|s| s.style == 0x8000_003c)
                && self.world.combatant(actor).is_some_and(|c| c.health() != 0)
            {
                self.world
                    .combatant_mut(actor)
                    .expect("checked equipment actor")
                    .set_mode(2);
                self.inventory_commands
                    .pending
                    .get(&hold.operation)
                    .cloned()
                    .map(|operation| InventoryDecision::Proposed(Box::new(operation)))
                    .ok_or(E::InvalidState)
            } else {
                Err(E::InvalidState)
            };
            if result.is_err() {
                if self.world.source_motion_token(actor) == Some(motion_token) {
                    let _ = self.world.cancel_motion(actor, motion_token);
                }
                if self.reject_inventory_inner(hold.operation).is_err() {
                    self.inventory_commands.equipment_stances.insert(id, hold);
                    continue;
                }
                self.inventory_commands.pending.remove(&hold.operation);
            }
            self.inventory_commands
                .outcomes
                .push_back(InventoryOutcome {
                    correlation: id,
                    result,
                });
        }
    }
    pub(super) fn prepare_inventory_equipment_physical(
        &mut self,
        operation: u64,
        prepared: crate::PreparedEquipmentPhysical,
    ) -> Result<InventoryOperation, E> {
        let mut value = self
            .inventory_commands
            .pending
            .get(&operation)
            .ok_or(E::InvalidState)?
            .clone();
        if value.equipment_vitals.is_some() {
            return Err(E::InvalidState);
        }
        let (_, vitals) = self.validate_equipment_candidate(&value, &prepared)?;
        // Cache immutable semantics before the database write. This does not
        // activate equipment and removes a post-commit shared-cache capacity race.
        self.magic
            .register_instant_batch(prepared.server_magic.clone())
            .map_err(|_| E::InvalidState)?;
        value.equipment_vitals = Some(vitals);
        self.inventory_commands
            .equipment
            .insert(operation, std::sync::Arc::new(prepared));
        self.inventory_commands
            .pending
            .insert(operation, value.clone());
        Ok(value)
    }
    fn validate_equipment_candidate(
        &self,
        operation: &InventoryOperation,
        prepared: &crate::PreparedEquipmentPhysical,
    ) -> Result<
        (
            Vec<bace_character::SkillValues>,
            crate::EquipmentVitalChange,
        ),
        E,
    > {
        let actor = operation.binding.actor;
        let effects = operation.equipment.as_ref().ok_or(E::InvalidState)?;
        if prepared.actor != actor
            || prepared.before_revision != operation.actor_revision
            || self.characters.equipment_operation(actor) != Some(operation.ticket.operation)
        {
            return Err(E::InvalidState);
        }
        self.validate_equipment_effects(effects, &operation.ticket.proposal)?;
        self.world
            .validate_death_motions(actor, &prepared.death_motions)
            .map_err(|_| E::InvalidState)?;
        let mut required =
            bace_magic::required_proc_spells(&prepared.magic_damage, &prepared.source.profile)
                .map_err(|_| E::InvalidState)?;
        required.extend(bace_combat::physical::physical_dirty_spell_dependencies(
            &prepared.source.profile,
        ));
        required.sort_unstable();
        required.dedup();
        if required.len() > 32 {
            return Err(E::InvalidState);
        }
        let mut supplied: Vec<_> = prepared
            .server_magic
            .definitions
            .iter()
            .map(|d| d.spell.spell.id)
            .collect();
        supplied.sort_unstable();
        if required != supplied {
            return Err(E::InvalidState);
        }
        self.magic
            .validate_instant_batch(&prepared.server_magic)
            .map_err(|_| E::InvalidState)?;
        self.world
            .validate_replacement_locomotion_styles(actor, &prepared.locomotion_styles)
            .map_err(|_| E::InvalidState)?;
        self.combat
            .validate_equipment_replacement(prepared)
            .map_err(|_| E::InvalidState)?;
        if let Some(mode) = operation.equipment_mode_after {
            self.combat
                .validate_equipment_mode_replacement(&self.world, prepared, mode)
                .map_err(|_| E::InvalidState)?;
        }
        let mut expected: BTreeMap<_, _> = self
            .inventory
            .equipped_items(actor)
            .map(|i| {
                (
                    i.id,
                    (
                        i.revision,
                        match i.place {
                            ItemPlace::Contained { equipped, .. } => equipped,
                            _ => 0,
                        },
                    ),
                )
            })
            .collect();
        for change in &operation.ticket.proposal.changes {
            expected.remove(&change.after.id);
            if let ItemPlace::Contained {
                container,
                equipped,
                ..
            } = change.after.place
                && container == actor
                && equipped != 0
            {
                expected.insert(change.after.id, (change.after.revision, equipped));
            }
        }
        if expected.len() != prepared.source.profile.equipment.len()
            || prepared
                .source
                .profile
                .equipment
                .iter()
                .any(|s| expected.get(&EntityId(s.entity)) != Some(&(s.revision, s.location)))
        {
            return Err(E::InvalidEquip);
        }
        let candidate = effects.registry(actor).map_err(|_| E::InvalidState)?;
        let registry = candidate
            .as_ref()
            .or_else(|| self.magic.registry(actor))
            .ok_or(E::InvalidState)?;
        let character = self.characters.get(actor).ok_or(E::NotBound)?;
        let values = super::super::skill_refresh::project_skills(
            character,
            &prepared.skills,
            Some(registry),
        )
        .map_err(|_| E::InvalidState)?;
        self.magic
            .validate_equipment_profile(actor, &prepared.magic_damage, &values)
            .map_err(|_| E::InvalidState)?;
        let gear = prepared
            .vital_inputs
            .equipped_health
            .iter()
            .try_fold(0u32, |n, (item, health)| {
                expected
                    .contains_key(item)
                    .then(|| n.checked_add(*health))
                    .flatten()
            })
            .ok_or(E::InvalidState)?;
        let old_inputs = self.vital_inputs.get(&actor).ok_or(E::InvalidState)?;
        if old_inputs.formulas != prepared.vital_inputs.formulas
            || prepared.vital_inputs.equipped_health.len() != expected.len()
        {
            return Err(E::InvalidState);
        }
        let maxima = super::super::live_vitals::project_maxima(
            character,
            registry,
            prepared.vital_inputs.formulas,
            gear,
            self.characters
                .native_services(actor)
                .ok_or(E::InvalidState)?
                .enlightenment,
        )
        .map_err(|_| E::InvalidState)?;
        let mut before = [0; 3];
        for (i, v) in [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
        {
            before[i] = self
                .world
                .vital(actor, v)
                .map_err(|_| E::InvalidState)?
                .current;
        }
        let mut after = before;
        for i in 0..3 {
            after[i] = after[i].min(maxima[i]);
            if let Some(minimum) = effects.minimum_vital_maxima {
                after[i] = after[i].min(minimum[i]);
            }
        }
        if operation.equipment_health_update {
            self.world
                .properties(actor)
                .ok_or(E::InvalidState)?
                .propose(
                    bace_entity::PropertyFamily::Int,
                    379,
                    if gear == 0 {
                        None
                    } else {
                        Some(bace_entity::PropertyValue::Int(
                            i32::try_from(gear).map_err(|_| E::Overflow)?,
                        ))
                    },
                )
                .map_err(|_| E::Capacity)?;
        }
        let vitals = crate::EquipmentVitalChange {
            gear_health: operation.equipment_health_update.then_some(gear),
            before,
            after,
            maxima,
            after_revision: operation.actor_revision.checked_add(1).ok_or(E::Overflow)?,
        };
        self.world
            .validate_equipment_vitals(
                actor,
                token(operation.ticket.operation),
                before,
                after,
                maxima,
            )
            .map_err(|_| E::InvalidState)?;
        Ok((values, vitals))
    }
    pub(in crate::kernel) fn preflight_inventory_equipment(&self, operation: u64) -> Result<(), E> {
        let Some(value) = self
            .inventory_commands
            .pending
            .get(&operation)
            .filter(|p| p.equipment.is_some())
        else {
            return Ok(());
        };
        let prepared = self
            .inventory_commands
            .equipment
            .get(&operation)
            .ok_or(E::InvalidState)?;
        let (_, vitals) = self.validate_equipment_candidate(value, prepared)?;
        if value.equipment_vitals.as_ref() != Some(&vitals) {
            return Err(E::InvalidState);
        }
        Ok(())
    }
    pub(in crate::kernel) fn finish_inventory_equipment(
        &mut self,
        operation: u64,
        committed: bool,
    ) {
        let Some(value) = self
            .inventory_commands
            .pending
            .get(&operation)
            .filter(|p| p.equipment.is_some())
            .cloned()
        else {
            return;
        };
        let actor = value.binding.actor;
        if committed {
            let prepared = self
                .inventory_commands
                .equipment
                .remove(&operation)
                .expect("prepared equipment profile");
            self.world
                .replace_locomotion_styles(actor, prepared.locomotion_styles.clone())
                .expect("held equipment style preflight");
            self.world
                .register_death_motions(actor, prepared.death_motions.clone())
                .expect("held death program preflight");
            let effects = value
                .equipment
                .as_ref()
                .expect("equipment effect companion");
            let vitals = value
                .equipment_vitals
                .as_ref()
                .expect("prepared equipment vital companion");
            let candidate = effects
                .registry(actor)
                .expect("validated equipment registry");
            let registry = candidate
                .as_ref()
                .or_else(|| self.magic.registry(actor))
                .expect("equipment wearer registry");
            let values = super::super::skill_refresh::project_skills(
                self.characters.get(actor).expect("held character"),
                &prepared.skills,
                Some(registry),
            )
            .expect("preflighted equipment skills");
            let registry_revision = registry.revision();
            self.adopt_equipment_effects(effects);
            self.combat.adopt_equipment_replacement(
                &prepared,
                &values,
                vitals.after_revision,
                registry_revision,
            );
            if let Some(mode) = value.equipment_mode_after {
                self.combat
                    .adopt_equipment_mode(&mut self.world, actor, mode);
            }
            self.magic.adopt_equipment_profile(
                actor,
                prepared.magic_damage.clone(),
                &values,
                prepared.skills.shield,
            );
            if let Some(health) = vitals.gear_health {
                let properties = self
                    .world
                    .properties_mut(actor)
                    .expect("preflighted properties");
                let change = properties
                    .propose(
                        bace_entity::PropertyFamily::Int,
                        379,
                        if health == 0 {
                            None
                        } else {
                            Some(bace_entity::PropertyValue::Int(health as i32))
                        },
                    )
                    .expect("preflighted gear health");
                properties
                    .adopt(change)
                    .expect("held equipment property owner");
            }
            self.world.adopt_equipment_vitals(
                actor,
                token(operation),
                vitals.before,
                vitals.after,
                vitals.maxima,
            );
            // These exact pools are represented by the committed character and
            // gear rows. A routine pass must not invent another revision for the
            // same mutation. Keep observed pose and any earlier dirty age intact.
            let observed_vitals = [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
                .map(|v| self.world.vital(actor, v).ok());
            if let Some(seen) = self.player_world_seen.get_mut(&actor) {
                seen.vitals = observed_vitals;
            }
            self.vital_inputs
                .insert(actor, prepared.vital_inputs.clone());
            self.physical_refresh_dirty.remove(&actor);
            self.locomotion_dirty.insert(actor);
        } else {
            self.inventory_commands.equipment.remove(&operation);
        }
        self.characters
            .finish_equipment(actor, operation, committed);
        self.world.release_vitals(token(operation));
    }
}
