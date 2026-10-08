//! Confirmation rechecks and one durable character+device operation.
use super::*;
use crate::skill_devices::*;
use bace_character::SkillWieldRequirement;
use bace_inventory::ItemPlace;

impl Kernel {
    /// Pinned WorldObject_Use.OnActivate returns for Int119 Active=0 before
    /// requirements, cooldown, response flags or ActOnUse. The outer player
    /// action still emits UseDone after this authenticated acknowledgement.
    pub fn request_inactive_skill_device(
        &mut self,
        context: ActionContext,
        item: EntityId,
        revision: u64,
    ) -> Result<(), SkillDeviceError> {
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| SkillDeviceError::Ownership)?;
        if !self.inventory.owned(context.actor, item) {
            return Err(SkillDeviceError::Ownership);
        }
        if self
            .inventory
            .item(item)
            .is_none_or(|current| current.revision != revision)
        {
            return Err(SkillDeviceError::Stale);
        }
        Ok(())
    }
    /// Register a trusted content projection against the current inventory revision.
    pub fn register_skill_device(
        &mut self,
        item: EntityId,
        revision: u64,
        device: PreparedSkillDevice,
    ) -> Result<(), SkillDeviceError> {
        self.register_skill_device_with_activation(item, revision, device, None)
    }
    pub fn register_skill_device_with_activation(
        &mut self,
        item: EntityId,
        revision: u64,
        device: PreparedSkillDevice,
        activation: Option<bace_inventory::ActivationRequirements>,
    ) -> Result<(), SkillDeviceError> {
        self.register_skill_device_with_cooldown(item, revision, device, activation, None)
    }
    pub fn register_skill_device_with_cooldown(
        &mut self,
        item: EntityId,
        revision: u64,
        device: PreparedSkillDevice,
        activation: Option<bace_inventory::ActivationRequirements>,
        cooldown_seconds: Option<f64>,
    ) -> Result<(), SkillDeviceError> {
        if cooldown_seconds.is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 86400.0)
            || cooldown_seconds.is_some() && activation.as_ref().and_then(|a| a.cooldown).is_none()
        {
            return Err(SkillDeviceError::Content);
        }
        if self
            .skill_devices
            .confirmations
            .values()
            .any(|quote| quote.public.item == item)
        {
            return Err(SkillDeviceError::Busy);
        }
        self.check_device_profile_registration(item, revision)?;
        if !self.skill_devices.devices.contains_key(&item)
            && self.skill_devices.devices.len() >= self.skill_devices.capacity
        {
            return Err(SkillDeviceError::Capacity);
        }
        self.skill_devices.devices.insert(
            item,
            DeviceProfile {
                revision,
                device,
                activation,
                cooldown_seconds,
            },
        );
        Ok(())
    }
    /// All four authored requirements, after the prepared content adapter converts
    /// legacy weapon skill IDs to the character's current MoA skill.
    pub fn register_skill_wield_requirements(
        &mut self,
        item: EntityId,
        revision: u64,
        requirements: [Option<SkillWieldRequirement>; 4],
    ) -> Result<(), SkillDeviceError> {
        self.check_device_profile_registration(item, revision)?;
        if !self.skill_devices.wielded.contains_key(&item)
            && self.skill_devices.wielded.len() >= self.skill_devices.capacity
        {
            return Err(SkillDeviceError::Capacity);
        }
        self.skill_devices.wielded.insert(
            item,
            WieldProfile {
                revision,
                requirements,
            },
        );
        Ok(())
    }
    fn check_device_profile_registration(
        &self,
        item: EntityId,
        revision: u64,
    ) -> Result<(), SkillDeviceError> {
        if self.inventory.reserved(item) {
            return Err(SkillDeviceError::Busy);
        }
        if self
            .inventory
            .item(item)
            .is_none_or(|i| i.revision != revision)
        {
            return Err(SkillDeviceError::Stale);
        }
        Ok(())
    }
    fn device_intent(
        &self,
        actor: EntityId,
        item: EntityId,
        revision: u64,
    ) -> Result<crate::SkillIntent, SkillDeviceError> {
        let current = self
            .inventory
            .item(item)
            .ok_or(SkillDeviceError::Ownership)?;
        if !self.inventory.owned(actor, item)
            || !matches!(current.place, ItemPlace::Contained { equipped: 0, .. })
        {
            return Err(SkillDeviceError::Ownership);
        }
        let profile = self
            .skill_devices
            .devices
            .get(&item)
            .ok_or(SkillDeviceError::Content)?;
        if current.revision != revision || profile.revision != revision {
            return Err(SkillDeviceError::Stale);
        }
        Ok(match profile.device {
            PreparedSkillDevice::Specialize(skill) => crate::SkillIntent::Specialize(skill),
            PreparedSkillDevice::Augment {
                skill,
                experience_cost,
            } => crate::SkillIntent::Augment {
                skill,
                experience_cost,
            },
            PreparedSkillDevice::Lower(skill) => {
                let mut wielded = Vec::new();
                for item in self.inventory.items().filter(|i| {
                    self.inventory.owned(actor, i.id)
                        && matches!(i.place, ItemPlace::Contained{equipped,..} if equipped != 0)
                }) {
                    let profile = self
                        .skill_devices
                        .wielded
                        .get(&item.id)
                        .ok_or(SkillDeviceError::MissingWieldProfile)?;
                    if profile.revision != item.revision {
                        return Err(SkillDeviceError::Stale);
                    }
                    for requirement in profile.requirements.iter().flatten().copied() {
                        let mapped = |id| self.device_moa_skill(actor, id);
                        wielded.push(match requirement {
                            SkillWieldRequirement::RawSkill(id) => {
                                SkillWieldRequirement::RawSkill(mapped(id)?)
                            }
                            SkillWieldRequirement::CurrentSkill(id) => {
                                SkillWieldRequirement::CurrentSkill(mapped(id)?)
                            }
                            SkillWieldRequirement::Training { skill, advancement } => {
                                SkillWieldRequirement::Training {
                                    skill: mapped(skill)?,
                                    advancement,
                                }
                            }
                        });
                    }
                }
                crate::SkillIntent::Lower { skill, wielded }
            }
        })
    }
    pub(super) fn check_device_busy(&self, actor: EntityId) -> Result<(), SkillDeviceError> {
        if self.inventory.reserved(actor)
            || self.recall_busy(actor)
            || self.characters.reserved(actor)
            || self.npcs.reserved(actor)
            || self.housing.reserved(actor)
            || self.magic.registry_reserved(actor)
            || self.magic.busy(actor)
        {
            return Err(SkillDeviceError::Busy);
        }
        Ok(())
    }
    fn check_skill_device_activation(
        &self,
        actor: EntityId,
        requirements: &bace_inventory::ActivationRequirements,
    ) -> Result<(), SkillDeviceError> {
        use bace_inventory::{ActivationFailure, ActivationRequirements};
        if self
            .social
            .directory
            .presence(actor)
            .is_some_and(|p| p.olthoi)
        {
            return Err(SkillDeviceError::Olthoi);
        }
        let cooldown_only = requirements.cooldown.is_some()
            && *requirements
                == ActivationRequirements {
                    cooldown: requirements.cooldown,
                    object: requirements.object,
                    ..Default::default()
                };
        if cooldown_only {
            let group = requirements
                .cooldown
                .and_then(|v| u16::try_from(v).ok())
                .filter(|v| *v > 0 && *v < 0x8000)
                .ok_or(SkillDeviceError::Activation(
                    ActivationFailure::MissingValue,
                ))?;
            let registry = self
                .magic
                .registry(actor)
                .ok_or(SkillDeviceError::Activation(
                    ActivationFailure::MissingValue,
                ))?;
            let spell = u32::from(0x8000 | group);
            if registry.entries().iter().any(|entry| {
                entry.spell == spell
                    && entry.spec.duration >= 0.0
                    && entry.start_time > -entry.spec.duration
            }) {
                return Err(SkillDeviceError::Activation(ActivationFailure::Cooldown));
            }
            return Ok(());
        }
        self.check_inventory_item_activation(actor, requirements)
            .map_err(SkillDeviceError::Activation)
    }
    pub fn request_skill_device(
        &mut self,
        context: ActionContext,
        item: EntityId,
        lifetime: u64,
    ) -> Result<SkillDeviceConfirmation, SkillDeviceError> {
        self.check_device_busy(context.actor)?;
        if self
            .social
            .directory
            .presence(context.actor)
            .is_some_and(|p| p.olthoi)
        {
            return Err(SkillDeviceError::Olthoi);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| SkillDeviceError::Ownership)?;
        if let Some(requirements) = self
            .skill_devices
            .devices
            .get(&item)
            .and_then(|p| p.activation.as_ref())
        {
            self.check_skill_device_activation(context.actor, requirements)?;
        }
        self.skill_devices
            .confirmations
            .retain(|_, q| q.public.expires > self.tick);
        if self
            .skill_devices
            .confirmations
            .contains_key(&context.actor)
        {
            return Err(SkillDeviceError::Confirmation);
        }
        if self.skill_devices.confirmations.len() >= self.skill_devices.capacity {
            return Err(SkillDeviceError::Capacity);
        }
        let revision = self
            .inventory
            .item(item)
            .ok_or(SkillDeviceError::Ownership)?
            .revision;
        let intent = self.device_intent(context.actor, item, revision)?;
        let character = self
            .characters
            .get(context.actor)
            .ok_or(SkillDeviceError::Ownership)?;
        match intent {
            crate::SkillIntent::Specialize(skill) => character.propose_specialize_skill(skill),
            crate::SkillIntent::Lower { skill, wielded } => {
                character.propose_lower_skill(skill, &wielded)
            }
            crate::SkillIntent::Augment {
                skill,
                experience_cost,
            } => character.propose_augment_skill(skill, experience_cost),
            _ => return Err(SkillDeviceError::Content),
        }
        .map_err(SkillDeviceError::Domain)?;
        if lifetime == 0 || lifetime > u32::MAX as u64 {
            return Err(SkillDeviceError::Content);
        }
        let token = self
            .skill_devices
            .next
            .checked_add(1)
            .filter(|token| *token <= u32::MAX as u64)
            .ok_or(SkillDeviceError::Capacity)?;
        let public = SkillDeviceConfirmation {
            token,
            actor: context.actor,
            item,
            expires: self
                .tick
                .checked_add(lifetime)
                .ok_or(SkillDeviceError::Capacity)?,
            device: self.skill_devices.devices[&item].device,
        };
        self.skill_devices.next = token;
        self.skill_devices.confirmations.insert(
            context.actor,
            Confirmation {
                public,
                context,
                revision,
            },
        );
        Ok(public)
    }
    /// A yes response re-reads ownership, revision, all equipped requirements and
    /// character costs. Success remains pending until the combined durable receipt.
    pub fn confirm_skill_device(
        &mut self,
        context: ActionContext,
        token: u64,
        accept: bool,
    ) -> Result<Option<SkillDeviceTicket>, SkillDeviceError> {
        let quote = *self
            .skill_devices
            .confirmations
            .get(&context.actor)
            .ok_or(SkillDeviceError::Confirmation)?;
        if quote.public.token != token
            || quote.context.account != context.account
            || quote.context.session != context.session
        {
            return Err(SkillDeviceError::Confirmation);
        }
        if !accept {
            self.characters
                .authorize(context, self.world.body(context.actor).is_ok())
                .map_err(|_| SkillDeviceError::Ownership)?;
            self.skill_devices.confirmations.remove(&context.actor);
            return Ok(None);
        }
        if quote.public.expires <= self.tick {
            self.skill_devices.confirmations.remove(&context.actor);
            return Err(SkillDeviceError::Expired);
        }
        self.check_device_busy(context.actor)?;
        if self.skill_devices.pending.len() >= self.skill_devices.capacity
            || self.skill_devices.outbox.len() >= self.skill_devices.capacity
        {
            return Err(SkillDeviceError::Capacity);
        }
        let intent = self.device_intent(context.actor, quote.public.item, quote.revision)?;
        if self.skill_devices.devices[&quote.public.item].device != quote.public.device {
            return Err(SkillDeviceError::Stale);
        }
        if let Some(requirements) = self
            .skill_devices
            .devices
            .get(&quote.public.item)
            .and_then(|p| p.activation.as_ref())
        {
            self.check_skill_device_activation(context.actor, requirements)?;
        }
        self.reserve_device_registries(context.actor, quote.public.item, true)?;
        if self.sync_registry_revisions().is_err() {
            self.reserve_device_registries(context.actor, quote.public.item, false)?;
            return Err(SkillDeviceError::Busy);
        }
        // A due item heartbeat can advance its inventory revision during reserve.
        if self
            .device_intent(context.actor, quote.public.item, quote.revision)
            .is_err()
        {
            self.reserve_device_registries(context.actor, quote.public.item, false)?;
            return Err(SkillDeviceError::Stale);
        }
        let cooldown = match self.preview_skill_device_cooldown(context.actor, quote.public.item) {
            Ok(value) => value,
            Err(error) => {
                self.reserve_device_registries(context.actor, quote.public.item, false)?;
                return Err(error);
            }
        };
        // This authorization consumes the confirmation action's sequence once.
        let skill = match self.characters.propose_skill(
            context,
            intent,
            self.world.body(context.actor).is_ok(),
        ) {
            Ok(v) => v,
            Err(error) => {
                self.reserve_device_registries(context.actor, quote.public.item, false)?;
                return Err(SkillDeviceError::Skill(error));
            }
        };
        let proposed = self
            .characters
            .proposed_skill_state(skill)
            .map_err(SkillDeviceError::Skill)?;
        if self
            .validate_proposed_skill_refresh(context.actor, proposed)
            .is_err()
        {
            self.characters
                .reject_skill(skill)
                .map_err(SkillDeviceError::Skill)?;
            self.reserve_device_registries(context.actor, quote.public.item, false)?;
            return Err(SkillDeviceError::Content);
        }
        let operation = match self.inventory.take(context.actor, quote.public.item, 1) {
            Ok(v) => v,
            Err(error) => {
                self.characters
                    .reject_skill(skill)
                    .map_err(SkillDeviceError::Skill)?;
                self.reserve_device_registries(context.actor, quote.public.item, false)?;
                return Err(SkillDeviceError::Inventory(error));
            }
        };
        if let Err(error) =
            self.reserve_inventory_registries(operation, &[context.actor, quote.public.item])
        {
            self.inventory
                .reject(operation)
                .map_err(SkillDeviceError::Inventory)?;
            self.characters
                .reject_skill(skill)
                .map_err(SkillDeviceError::Skill)?;
            self.reserve_device_registries(context.actor, quote.public.item, false)?;
            return Err(SkillDeviceError::Inventory(error));
        }
        let inventory = self
            .inventory
            .pending_ticket(operation)
            .cloned()
            .ok_or(SkillDeviceError::Stale)?;
        self.inventory
            .claim(operation)
            .map_err(SkillDeviceError::Inventory)?;
        let ticket = SkillDeviceTicket {
            skill,
            inventory,
            cooldown,
        };
        self.skill_devices.pending.insert(operation, ticket.clone());
        self.skill_devices.outbox.push_back(operation);
        self.skill_devices.confirmations.remove(&context.actor);
        Ok(Some(ticket))
    }
    pub fn take_skill_device_proposal(&mut self) -> Option<SkillDeviceTicket> {
        while let Some(operation) = self.skill_devices.outbox.pop_front() {
            if let Some(ticket) = self.skill_devices.pending.get(&operation) {
                self.skill_devices.submitted.insert(operation);
                return Some(ticket.clone());
            }
        }
        None
    }
    pub fn retry_skill_device(&mut self, operation: u64) -> Result<(), SkillDeviceError> {
        if !self.skill_devices.pending.contains_key(&operation) {
            return Err(SkillDeviceError::Stale);
        }
        if self.skill_devices.outbox.contains(&operation) {
            return Err(SkillDeviceError::Busy);
        }
        if self.skill_devices.outbox.len() >= self.skill_devices.capacity {
            return Err(SkillDeviceError::Capacity);
        }
        self.skill_devices.submitted.remove(&operation);
        self.skill_devices.outbox.push_back(operation);
        Ok(())
    }
    /// Definite rollback only; timeouts retain both owners and the same ticket.
    pub fn reject_skill_device(&mut self, operation: u64) -> Result<(), SkillDeviceError> {
        let ticket = self
            .skill_devices
            .pending
            .get(&operation)
            .ok_or(SkillDeviceError::Stale)?
            .clone();
        self.characters
            .validate_skill_ticket(ticket.skill)
            .map_err(SkillDeviceError::Skill)?;
        self.preflight_inventory_registries(operation, false)
            .map_err(SkillDeviceError::Inventory)?;
        self.characters
            .reject_skill(ticket.skill)
            .map_err(SkillDeviceError::Skill)?;
        let actor = ticket.skill.context.actor;
        self.inventory
            .reject(operation)
            .map_err(SkillDeviceError::Inventory)?;
        let item = ticket
            .inventory
            .proposal
            .changes
            .first()
            .ok_or(SkillDeviceError::Stale)?
            .after
            .id;
        self.release_inventory_registries(operation)
            .map_err(SkillDeviceError::Inventory)?;
        self.reserve_device_registries(actor, item, false)?;
        self.skill_devices.submitted.remove(&operation);
        self.skill_devices.pending.remove(&operation);
        self.skill_devices.outbox.retain(|v| *v != operation);
        Ok(())
    }
    pub fn confirm_skill_device_committed(
        &mut self,
        receipt: &crate::InventoryReceipt,
    ) -> Result<SkillDeviceTicket, SkillDeviceError> {
        if !self.skill_devices.submitted.contains(&receipt.operation) {
            return Err(SkillDeviceError::Stale);
        }
        let ticket = self
            .skill_devices
            .pending
            .get(&receipt.operation)
            .ok_or(SkillDeviceError::Stale)?
            .clone();
        self.characters
            .validate_skill_ticket(ticket.skill)
            .map_err(SkillDeviceError::Skill)?;
        self.inventory
            .validate_receipt(receipt)
            .map_err(SkillDeviceError::Inventory)?;
        self.preflight_inventory_registries(receipt.operation, true)
            .map_err(SkillDeviceError::Inventory)?;
        let cooldown_proposal = if let Some(cooldown) = &ticket.cooldown {
            let registry = self
                .magic
                .registry(ticket.skill.context.actor)
                .ok_or(SkillDeviceError::Content)?;
            if registry.revision() != cooldown.before_revision
                || cooldown.after_revision
                    != cooldown
                        .before_revision
                        .checked_add(1)
                        .ok_or(SkillDeviceError::Capacity)?
            {
                return Err(SkillDeviceError::Stale);
            }
            let proposal = registry
                .propose_cooldown(
                    cooldown.group,
                    ticket
                        .inventory
                        .proposal
                        .changes
                        .first()
                        .ok_or(SkillDeviceError::Stale)?
                        .after
                        .id
                        .0,
                    cooldown.seconds,
                )
                .map_err(|_| SkillDeviceError::Stale)?;
            if registry
                .preview(&proposal)
                .map_err(|_| SkillDeviceError::Stale)?
                != cooldown.after
            {
                return Err(SkillDeviceError::Stale);
            }
            Some(proposal)
        } else {
            None
        };
        // Both validated under the same single-thread owner; no await or reentry.
        let proposed = self
            .characters
            .proposed_skill_state(ticket.skill)
            .map_err(SkillDeviceError::Skill)?;
        self.validate_proposed_skill_refresh(ticket.skill.context.actor, proposed)
            .map_err(|_| SkillDeviceError::Content)?;
        self.characters
            .commit_skill(ticket.skill)
            .map_err(SkillDeviceError::Skill)?;
        self.inventory
            .confirm(receipt)
            .map_err(SkillDeviceError::Inventory)?;
        if let Some(proposal) = cooldown_proposal {
            self.magic
                .adopt_skill_device_cooldown(ticket.skill.context.actor, proposal);
        }
        let actor = ticket.skill.context.actor;
        self.refresh_character_skills(actor)
            .map_err(|_| SkillDeviceError::Content)?;
        let item = ticket
            .inventory
            .proposal
            .changes
            .first()
            .ok_or(SkillDeviceError::Stale)?
            .after
            .id;
        self.retire_inventory_registries(&ticket.inventory)
            .map_err(SkillDeviceError::Inventory)?;
        self.release_inventory_registries(receipt.operation)
            .map_err(SkillDeviceError::Inventory)?;
        self.reserve_device_registries(actor, item, false)?;
        self.skill_devices.submitted.remove(&receipt.operation);
        self.skill_devices.pending.remove(&receipt.operation);
        self.skill_devices
            .outbox
            .retain(|v| *v != receipt.operation);
        for change in &ticket.inventory.proposal.changes {
            if change.after.place == ItemPlace::Removed {
                self.skill_devices.devices.remove(&change.after.id);
                self.skill_devices.wielded.remove(&change.after.id);
                continue;
            }
            if let Some(profile) = self.skill_devices.devices.get_mut(&change.after.id) {
                profile.revision = change.after.revision;
            }
        }
        Ok(ticket)
    }
    pub(super) fn reserve_device_registries(
        &mut self,
        actor: EntityId,
        item: EntityId,
        reserved: bool,
    ) -> Result<(), SkillDeviceError> {
        if reserved
            && [actor, item]
                .iter()
                .any(|id| self.magic.registry_reserved(*id))
        {
            return Err(SkillDeviceError::Busy);
        }
        let now = self.tick as f64 / 30.0;
        let mut completed = None;
        for id in [actor, item] {
            if self.magic.registry(id).is_none() {
                continue;
            }
            if self.magic.reserve_registry(id, reserved, now).is_err() {
                if reserved && let Some(previous) = completed {
                    self.magic
                        .reserve_registry(previous, false, now)
                        .map_err(|_| SkillDeviceError::Busy)?;
                }
                return Err(SkillDeviceError::Busy);
            }
            completed = Some(id);
        }
        Ok(())
    }
    fn preview_skill_device_cooldown(
        &self,
        actor: EntityId,
        item: EntityId,
    ) -> Result<Option<SkillDeviceCooldown>, SkillDeviceError> {
        let profile = self
            .skill_devices
            .devices
            .get(&item)
            .ok_or(SkillDeviceError::Content)?;
        let Some(seconds) = profile.cooldown_seconds else {
            return Ok(None);
        };
        let group = profile
            .activation
            .as_ref()
            .and_then(|r| r.cooldown)
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v > 0 && *v < 0x8000)
            .ok_or(SkillDeviceError::Content)?;
        let registry = self
            .magic
            .registry(actor)
            .ok_or(SkillDeviceError::Content)?;
        let proposal = registry
            .propose_cooldown(group, item.0, seconds)
            .map_err(|_| {
                SkillDeviceError::Activation(bace_inventory::ActivationFailure::Cooldown)
            })?;
        let after = registry
            .preview(&proposal)
            .map_err(|_| SkillDeviceError::Stale)?;
        Ok(Some(SkillDeviceCooldown {
            group,
            seconds,
            before_revision: registry.revision(),
            after_revision: registry
                .revision()
                .checked_add(1)
                .ok_or(SkillDeviceError::Capacity)?,
            after,
        }))
    }
    pub fn has_skill_device_work(&self) -> bool {
        !self.skill_devices.pending.is_empty() || !self.skill_devices.outbox.is_empty()
    }
}

impl Kernel {
    fn device_moa_skill(&self, actor: EntityId, id: u32) -> Result<u32, SkillDeviceError> {
        match id {
            1 | 4 | 5 | 9 | 10 | 11 | 13 => {
                let value = |skill| {
                    self.character_skill_projection(actor, skill)
                        .map(|(_, v)| v.current)
                        .ok_or(SkillDeviceError::MissingWieldProfile)
                };
                let mut best = 45;
                for other in [44, 46] {
                    if value(other)? > value(best)? {
                        best = other;
                    }
                }
                Ok(best)
            }
            2 | 3 | 8 | 12 => Ok(47),
            _ => Ok(id),
        }
    }
}
