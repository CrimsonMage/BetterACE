use super::*;
use crate::{SkillDeviceCommand, SkillDeviceOutcome, SkillDeviceResult};

struct PreparedDeviceRequest {
    context: bace_gameplay_api::ActionContext,
    item: EntityId,
    revision: u64,
    device: crate::PreparedSkillDevice,
    activation: Option<bace_inventory::ActivationRequirements>,
    cooldown_seconds: Option<f64>,
    wielded: Vec<(
        EntityId,
        u64,
        [Option<bace_character::SkillWieldRequirement>; 4],
    )>,
    lifetime: u64,
}

impl Kernel {
    pub(super) fn apply_skill_device_command(
        &mut self,
        command: SkillDeviceCommand,
    ) -> SkillDeviceOutcome {
        let context = match &command {
            SkillDeviceCommand::RequestInactive { context, .. }
            | SkillDeviceCommand::RequestPrepared { context, .. }
            | SkillDeviceCommand::Request { context, .. }
            | SkillDeviceCommand::Confirm { context, .. } => Some(*context),
            _ => None,
        };
        let result = match command {
            SkillDeviceCommand::RequestInactive {
                context,
                item,
                revision,
            } => self
                .request_inactive_skill_device(context, item, revision)
                .map(|()| SkillDeviceResult::Inactive),
            SkillDeviceCommand::RequestPrepared {
                context,
                item,
                revision,
                device,
                activation,
                cooldown_seconds,
                wielded,
                lifetime,
            } => self
                .request_prepared_skill_device(PreparedDeviceRequest {
                    context,
                    item,
                    revision,
                    device,
                    activation,
                    cooldown_seconds,
                    wielded,
                    lifetime,
                })
                .map(SkillDeviceResult::Confirmation),
            SkillDeviceCommand::Request {
                context,
                item,
                lifetime,
            } => self
                .request_skill_device(context, item, lifetime)
                .map(SkillDeviceResult::Confirmation),
            SkillDeviceCommand::Confirm {
                context,
                token,
                accept,
            } => self
                .confirm_skill_device(context, token, accept)
                .map(SkillDeviceResult::Proposed),
            SkillDeviceCommand::Commit { receipt } => self
                .confirm_skill_device_committed(&receipt)
                .map(SkillDeviceResult::Committed),
            SkillDeviceCommand::Rollback { operation } => self
                .reject_skill_device(operation)
                .map(|()| SkillDeviceResult::RolledBack(operation)),
        };
        SkillDeviceOutcome { context, result }
    }
    pub fn take_skill_device_outcome(&mut self) -> Option<SkillDeviceOutcome> {
        self.skill_device_outcomes.pop_front()
    }
}

impl Kernel {
    fn request_prepared_skill_device(
        &mut self,
        request: PreparedDeviceRequest,
    ) -> Result<crate::SkillDeviceConfirmation, crate::SkillDeviceError> {
        use crate::SkillDeviceError as E;
        let PreparedDeviceRequest {
            context,
            item,
            revision,
            device,
            activation,
            cooldown_seconds,
            wielded,
            lifetime,
        } = request;
        if wielded.len() > 1023 || lifetime == 0 || lifetime > 1800 {
            return Err(E::Content);
        }
        self.check_device_busy(context.actor)?;
        let mut ids = std::collections::BTreeSet::new();
        for (id, rev, _) in &wielded {
            if !ids.insert(*id) || !self.inventory.owned(context.actor,*id)
                || self.inventory.item(*id).is_none_or(|i|i.revision!=*rev || !matches!(i.place,bace_inventory::ItemPlace::Contained{equipped,..} if equipped!=0))
                || self.inventory.reserved(*id) {return Err(E::Stale);}
        }
        if matches!(device,crate::PreparedSkillDevice::Lower(_)) && self.inventory.items().any(|i|
            self.inventory.owned(context.actor,i.id) && matches!(i.place,bace_inventory::ItemPlace::Contained{equipped,..} if equipped!=0) && !ids.contains(&i.id)) {return Err(E::MissingWieldProfile);}
        if !self.inventory.owned(context.actor, item) {
            return Err(E::Ownership);
        }
        if self
            .inventory
            .item(item)
            .is_none_or(|i| i.revision != revision)
        {
            return Err(E::Stale);
        }
        let new_wield = wielded
            .iter()
            .filter(|(id, _, _)| !self.skill_devices.wielded.contains_key(id))
            .count();
        if self.skill_devices.wielded.len() + new_wield > self.skill_devices.capacity
            || (!self.skill_devices.devices.contains_key(&item)
                && self.skill_devices.devices.len() >= self.skill_devices.capacity)
        {
            return Err(E::Capacity);
        }
        self.register_skill_device_with_cooldown(
            item,
            revision,
            device,
            activation,
            cooldown_seconds,
        )?;
        for (id, revision, requirements) in wielded {
            self.register_skill_wield_requirements(id, revision, requirements)?;
        }
        self.request_skill_device(context, item, lifetime)
    }
}
