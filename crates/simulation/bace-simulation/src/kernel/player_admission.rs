//! Complete player entry validates all owners before transferring any live state.
use super::Kernel;
use crate::player_admission::{PlayerAdmissionError as E, PreparedPlayerAdmission};
use bace_entity::EntityVital;
use std::collections::BTreeSet;
impl Kernel {
    pub fn avatar_locomotion(
        &self,
        actor: bace_types::EntityId,
    ) -> Option<&std::sync::Arc<bace_motion::AnimatedLocomotion>> {
        self.avatar_locomotion.get(&actor)
    }
    pub fn admit_player(
        &mut self,
        input: PreparedPlayerAdmission,
    ) -> Result<(), (E, Box<PreparedPlayerAdmission>)> {
        if self.region_residency.is_quiescing() {
            return Err((E::State, Box::new(input)));
        }
        let inventory = match self.validate_player_admission(&input) {
            Ok(v) => v,
            Err(error) => return Err((error, Box::new(input))),
        };
        let PreparedPlayerAdmission {
            binding,
            mut actor,
            locomotion,
            locomotion_styles,
            death_motions,
            mut state,
            properties,
            mut combatant,
            caster,
            magic_damage,
            server_magic,
            physical,
            physical_motions,
            physical_source,
            skills,
            vital_inputs,
            items: _,
            item_spell_targets,
            containers: _,
            presence,
            chat_eligibility,
            staff,
            portal_access,
        } = input;
        let id = binding.actor;
        combatant
            .stamp_incarnation(binding.session.0)
            .expect("player incarnation preflight");
        self.magic
            .register_instant_batch(server_magic)
            .expect("instant proc assets preflight");
        actor
            .body
            .adopt_animated_style(locomotion.clone(), true)
            .expect("initial interpreter preflight");
        self.world
            .insert(actor)
            .expect("same-owner geometry admission preflight");
        self.world
            .register_properties(id, properties)
            .expect("new actor properties");
        self.world
            .register_combatant(id, combatant)
            .expect("new actor combatant");
        self.world
            .begin_player_entry(id)
            .expect("loading-body lifecycle preflight");
        self.avatar_locomotion.insert(id, locomotion);
        self.world
            .register_locomotion_styles(id, locomotion_styles)
            .expect("same-owner locomotion style preflight");
        self.world
            .register_death_motions(id, death_motions)
            .expect("same-owner death program preflight");
        self.inventory.adopt_player_admission(inventory);
        self.characters
            .register_complete(binding, state.character)
            .expect("same-owner character admission preflight");
        let registry = state
            .enchantments
            .take()
            .expect("mandatory registry preflight");
        self.register_magic_registry(id, registry, true)
            .expect("registry capacity preflight");
        for (item, registry) in state.item_enchantments {
            self.register_magic_registry(item, registry, true)
                .expect("item registry capacity preflight");
        }
        for item in item_spell_targets {
            self.magic
                .register_item_type(item.item, item.item_type)
                .expect("item spell type preflight");
            self.magic
                .register_item_spell_quality(
                    item.item,
                    item.resist_magic,
                    item.non_projectile_immune,
                )
                .expect("item spell quality preflight");
        }
        self.register_magic_caster(id, caster)
            .expect("caster preflight");
        self.register_magic_damage_profile(id, magic_damage)
            .expect("magic damage profile preflight");
        self.restore_magic_recovery(id, state.recovery.expect("recovery preflight"), 0.0)
            .expect("recovery preflight");
        self.register_physical_combat(id, physical)
            .expect("physical profile preflight");
        self.combat
            .register_physical_refresh_source(id, physical_source)
            .expect("physical raw source preflight");
        for (motion, speed, chain) in physical_motions {
            self.register_physical_motion(id, motion, speed, chain)
                .expect("physical chain preflight");
        }
        self.restore_physical_recovery(id, state.physical_recovery)
            .expect("physical recovery preflight");
        self.vital_inputs.insert(id, vital_inputs);
        self.register_character_skill_inputs(id, skills)
            .expect("skill projection preflight");
        self.register_portal_links(
            id,
            state.portal_links.take().expect("portal preflight"),
            portal_access,
        )
        .expect("portal capacity preflight");
        self.register_player_death_state(id, state.death.take().expect("death state preflight"))
            .expect("death capacity preflight");
        if let Some(mana) = state.equipment_mana.take() {
            self.register_equipment_mana(binding, mana)
                .expect("equipment mana admission preflight");
        }
        for prepared in state.item_experience {
            self.register_item_experience(prepared)
                .expect("item metadata preflight");
        }
        self.register_staff(staff).expect("staff preflight");
        self.register_social_presence(presence, state.social.take().expect("social preflight"))
            .expect("social projection preflight");
        if let Some(gag) = state.gag.take() {
            self.register_gag(id, gag).expect("gag preflight");
        }
        self.register_chat_eligibility(id, chat_eligibility)
            .expect("chat eligibility preflight");
        let snapshot = self
            .player_world_snapshot(id)
            .expect("accepted actor vitals");
        self.player_world_seen.insert(id, snapshot);
        Ok(())
    }
    fn validate_player_admission(
        &self,
        input: &PreparedPlayerAdmission,
    ) -> Result<crate::inventory::admission::PreparedInventoryAdmission, E> {
        let id = input.binding.actor;
        if input
            .chat_eligibility
            .account_created_unix
            .is_some_and(|v| v < 0)
            || self.social.chat_eligibility.contains_key(&id)
            || input.chat_eligibility.player_age_seconds > i32::MAX as u64
            || input
                .state
                .chat_age
                .is_some_and(|age| age != input.chat_eligibility.player_age_seconds)
        {
            return Err(E::Social);
        }
        bace_magic::validate_magic_damage_profile(&input.magic_damage).map_err(|_| E::Magic)?;
        if !input.magic_damage.player {
            return Err(E::Magic);
        }
        if !input.state.physical_recovery.is_finite()
            || !(0.0..=180.0).contains(&input.state.physical_recovery)
        {
            return Err(E::Combat);
        }
        if input.actor.id != id
            || input.presence.identity.character != id
            || input.presence.identity.account != input.binding.account
            || !input.presence.online
            || input.staff.binding != input.binding
            || input.presence.access != input.staff.privileges.account_access
            || !input.caster.player
            || input
                .combatant
                .validate_incarnation(input.binding.session.0)
                .is_err()
        {
            return Err(E::Identity);
        }
        if self.avatar_locomotion.contains_key(&id) || self.avatar_locomotion.len() >= 4096 {
            return Err(E::Capacity);
        }
        input
            .locomotion
            .profile
            .interpret(bace_motion::LocomotionControls::default(), 1.0)
            .map_err(|_| E::Geometry)?;
        if input.actor.body.collision_shape().is_none() {
            return Err(E::Geometry);
        }
        self.world
            .validate_actor(&input.actor)
            .map_err(|_| E::Geometry)?;
        if self.population.reserves_identity(id)
            || self.generator_reserves_identity(id)
            || self.physical_reserves_identity(id)
            || self.magic.reserves_identity(id)
            || self.inventory.item(id).is_some()
        {
            return Err(E::Identity);
        }
        self.characters
            .validate_player_admission(input.binding, &input.state.character)
            .map_err(|_| E::Character)?;
        let world = input.state.world.ok_or(E::State)?;
        let accepted = input.actor.body.accepted();
        if world.cell != input.actor.cell
            || world.position != accepted.position()
            || world.heading != accepted.heading_radians()
        {
            return Err(E::State);
        }
        for (index, vital) in [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
        {
            if input.combatant.vital(vital).is_none()
                || world.vitals[index] != input.combatant.vital(vital)
            {
                return Err(E::State);
            }
        }
        let inventory = self
            .inventory
            .prepare_player_admission(id, &input.items, &input.containers)
            .map_err(|_| E::Inventory)?;
        for item in &input.items {
            if self.world.contains_identity(item.id)
                || self.population.reserves_identity(item.id)
                || self.generator_reserves_identity(item.id)
                || self.physical_reserves_identity(item.id)
                || self.magic.reserves_identity(item.id)
            {
                return Err(E::Identity);
            }
        }
        let registry = input.state.enchantments.as_ref().ok_or(E::State)?;
        if input.vital_inputs.equipped_health.len() > 64 {
            return Err(E::Capacity);
        }
        let mut vital_items = BTreeSet::new();
        let gear_health = input.vital_inputs.equipped_health.iter().try_fold(0u32, |total, (item, amount)| {
            if !vital_items.insert(*item) { return Err(E::State); }
            let equipped = input.items.iter().find(|i| i.id == *item).ok_or(E::State)?;
            if !matches!(equipped.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == id && equipped != 0) { return Err(E::State); }
            total.checked_add(*amount).ok_or(E::State)
        })?;
        let maxima = super::live_vitals::project_maxima(
            &input.state.character.progression,
            registry,
            input.vital_inputs.formulas,
            gear_health,
            input
                .state
                .character
                .native_services
                .as_ref()
                .ok_or(E::State)?
                .enlightenment,
        )
        .map_err(|_| E::State)?;
        input
            .combatant
            .validate_vital_maxima(maxima)
            .map_err(|_| E::State)?;
        for (index, vital) in [EntityVital::Health, EntityVital::Stamina, EntityVital::Mana]
            .into_iter()
            .enumerate()
        {
            if input
                .combatant
                .vital(vital)
                .is_none_or(|pool| pool.maximum != maxima[index])
            {
                return Err(E::State);
            }
        }
        let mut registries = vec![(id, registry)];
        if input.state.item_enchantments.len() != input.items.len() {
            return Err(E::State);
        }
        if input.item_spell_targets.len() != input.items.len() {
            return Err(E::State);
        }
        let mut item_spell_ids = BTreeSet::new();
        for item in &input.item_spell_targets {
            if !item_spell_ids.insert(item.item)
                || !input.items.iter().any(|i| i.id == item.item)
                || item.resist_magic > i32::MAX as u32
            {
                return Err(E::State);
            }
        }
        self.magic
            .validate_item_spell_targets(
                &input
                    .item_spell_targets
                    .iter()
                    .map(|item| {
                        (
                            item.item,
                            item.item_type,
                            item.resist_magic,
                            item.non_projectile_immune,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| E::Magic)?;
        for (item, registry) in &input.state.item_enchantments {
            if !input.items.iter().any(|i| i.id == *item) {
                return Err(E::Identity);
            }
            registries.push((*item, registry));
        }
        let recovery = input.state.recovery.ok_or(E::State)?;
        self.magic
            .validate_player_admission(
                id,
                &input.caster,
                &registries,
                recovery,
                self.tick as f64 / 30.0,
            )
            .map_err(|_| E::Magic)?;
        let ui = input.state.character.ui.as_ref().ok_or(E::State)?;
        if input.caster.known_spells != ui.known_spells.iter().copied().collect() {
            return Err(E::Magic);
        }
        self.combat
            .validate_player_admission(id, &input.physical)
            .map_err(|_| E::Combat)?;
        let mut required = bace_magic::required_proc_spells(&input.magic_damage, &input.physical)
            .map_err(|_| E::Magic)?;
        required.extend(bace_combat::physical::physical_dirty_spell_dependencies(
            &input.physical,
        ));
        required.sort_unstable();
        required.dedup();
        if required.len() > 32 {
            return Err(E::Magic);
        }
        let mut supplied: Vec<_> = input
            .server_magic
            .definitions
            .iter()
            .map(|d| d.spell.spell.id)
            .collect();
        supplied.sort_unstable();
        if required != supplied {
            return Err(E::Magic);
        }
        self.magic
            .validate_instant_batch(&input.server_magic)
            .map_err(|_| E::Magic)?;
        self.combat
            .validate_physical_motions(id, &input.physical_motions)
            .map_err(|_| E::Combat)?;
        self.world
            .validate_locomotion_styles(id, &input.locomotion_styles)
            .map_err(|_| E::Geometry)?;
        self.world
            .validate_death_motions(id, &input.death_motions)
            .map_err(|_| E::Geometry)?;
        self.world
            .validate_player_entry(id)
            .map_err(|_| E::Geometry)?;
        input
            .actor
            .body
            .validate_animated_style(&input.locomotion, true)
            .map_err(|_| E::Geometry)?;
        if !input
            .locomotion_styles
            .iter()
            .any(|style| style.profile.style == input.locomotion.profile.style)
        {
            return Err(E::Geometry);
        }
        self.combat
            .validate_physical_refresh_source(id, &input.physical_source)
            .map_err(|_| E::Combat)?;
        if *input.physical_source.profile != *input.physical {
            return Err(E::Combat);
        }
        if !self
            .combat
            .proposed_equipment_current(&input.physical, id, inventory.inventory())
        {
            return Err(E::Combat);
        }
        let skills = super::skill_refresh::project_skills(
            &input.state.character.progression,
            &input.skills,
            Some(registry),
        )
        .map_err(|_| E::Combat)?;
        if [31, 32, 33, 34, 43, 15, 16]
            .iter()
            .any(|id| !skills.iter().any(|s| s.skill == *id))
        {
            return Err(E::Combat);
        }
        self.validate_player_portal_admission(id)
            .map_err(|_| E::State)?;
        if input.state.portal_links.is_none() {
            return Err(E::State);
        }
        if self.player_deaths.states.contains_key(&id)
            || self.player_deaths.states.len() >= 4096
            || input
                .state
                .death
                .as_ref()
                .is_none_or(|d| d.validate().is_err())
        {
            return Err(E::State);
        }
        if self.gag_pending(id) {
            return Err(E::State);
        }
        if let Some(gag) = input.state.gag {
            gag.validate().map_err(|_| E::State)?;
            if gag.state.active
                && input
                    .state
                    .equipment_mana
                    .as_ref()
                    .is_none_or(|m| m.heartbeat <= 0.)
            {
                return Err(E::State);
            }
            if self.social_gags.states.contains_key(&id)
                || self.social_gags.states.len() >= 4096
                || gag.state.active != input.presence.gagged
            {
                return Err(E::State);
            }
        } else if input.presence.gagged {
            return Err(E::State);
        }
        if let Some(mana) = &input.state.equipment_mana {
            mana.validate(id).map_err(|_| E::Inventory)?;
            if mana.fresh && mana.heartbeat > 0. {
                self.equipment_mana_initial_delay(input.binding)
                    .map_err(|_| E::State)?;
            }
            if self.equipment_mana.players.contains_key(&id)
                || self.equipment_mana.players.len() >= self.equipment_mana.capacity
                || self
                    .equipment_mana
                    .players
                    .values()
                    .map(|p| p.items.len())
                    .sum::<usize>()
                    + mana.items.len()
                    > self.equipment_mana.capacity
                || mana
                    .items
                    .iter()
                    .any(|item| !input.items.iter().any(|i| i.id == item.item))
            {
                return Err(E::Inventory);
            }
        }
        let mut xp = BTreeSet::new();
        let mut orders = BTreeSet::new();
        if self.item_experience.items.len() + input.state.item_experience.len()
            > self.item_experience.capacity
        {
            return Err(E::Capacity);
        }
        for p in &input.state.item_experience {
            let item = input
                .items
                .iter()
                .find(|i| i.id == p.item)
                .ok_or(E::Inventory)?;
            if p.actor != id
                || !xp.insert(p.item)
                || !orders.insert(p.equipment_order)
                || self.item_experience.items.contains_key(&p.item)
                || p.validate(item).is_err()
            {
                return Err(E::Inventory);
            }
        }
        if inventory
            .inventory()
            .equipped_items(id)
            .any(|i| !xp.contains(&i.id))
        {
            return Err(E::Inventory);
        }
        self.staff
            .validate_registration(&input.staff)
            .map_err(|_| E::Staff)?;
        let prefs = input.state.social.as_ref().ok_or(E::Social)?;
        self.social
            .directory
            .validate_registration(&input.presence, prefs)
            .map_err(|_| E::Social)?;
        if prefs
            .friends
            .iter()
            .any(|id| self.social.directory.presence(*id).is_none())
        {
            return Err(E::Social);
        }
        if self.social.events.len() + self.social.directory.watchers(id).count() + 1
            > self.social.capacity
        {
            return Err(E::Capacity);
        }
        Ok(inventory)
    }
}
