//! Bounded authority and result queue owned by the simulation thread.
use bace_gameplay_api::{
    ActionContext, CharacterBinding,
    staff::{StaffError, StaffEvent, StaffPrivileges, StaffRegistration},
};
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
pub(crate) struct PendingStaffReward {
    pub context: ActionContext,
    pub token: u64,
    pub target: EntityId,
    pub amount: u64,
    pub issuer_name: String,
    pub target_name: String,
}
pub struct StaffState {
    pub(crate) selections: BTreeMap<EntityId, bace_gameplay_api::selection::TargetSelection>,
    pub(crate) effects: BTreeMap<u32, bace_magic::EnchantmentEntry>,
    pub(crate) rewards: BTreeMap<u64, PendingStaffReward>,
    pub(crate) spell_registries: BTreeMap<u64, EntityId>,
    pub(crate) buffs: BTreeMap<u8, bace_gameplay_api::staff::StaffBuffPlan>,
    pub(crate) spells: BTreeMap<u32, bace_gameplay_api::staff::StaffSpellDefinition>,
    members: BTreeMap<EntityId, StaffRegistration>,
    events: VecDeque<StaffEvent>,
    capacity: usize,
}
impl StaffState {
    pub fn new(capacity: usize) -> Result<Self, StaffError> {
        if capacity == 0 || capacity > 4096 {
            return Err(StaffError::Capacity);
        }
        Ok(Self {
            effects: BTreeMap::new(),
            selections: BTreeMap::new(),
            rewards: BTreeMap::new(),
            spell_registries: BTreeMap::new(),
            spells: BTreeMap::new(),
            buffs: BTreeMap::new(),
            members: BTreeMap::new(),
            events: VecDeque::new(),
            capacity,
        })
    }
    pub fn can_remove(&self, binding: CharacterBinding) -> Result<(), StaffError> {
        if self
            .members
            .get(&binding.actor)
            .is_some_and(|r| r.binding != binding)
        {
            return Err(StaffError::NotBound);
        }
        Ok(())
    }
    pub(crate) fn capacity(&self) -> usize {
        self.capacity
    }
    pub(crate) fn room(&self, count: usize) -> bool {
        self.events.len().saturating_add(count) <= self.capacity
    }
    pub fn has_state(&self) -> bool {
        !self.members.is_empty()
            || !self.events.is_empty()
            || !self.spell_registries.is_empty()
            || !self.rewards.is_empty()
    }
    pub fn registration(&self, actor: EntityId) -> Option<StaffRegistration> {
        self.members.get(&actor).copied()
    }
    pub fn registered(&self, actor: EntityId) -> bool {
        self.members.contains_key(&actor)
    }
    pub fn validate_registration(
        &self,
        registration: &StaffRegistration,
    ) -> Result<(), StaffError> {
        if registration.privileges.account_access > 5 {
            return Err(StaffError::Invalid);
        }
        if self.members.contains_key(&registration.binding.actor) {
            return Err(StaffError::Busy);
        }
        if self.members.len() >= self.capacity {
            return Err(StaffError::Capacity);
        }
        Ok(())
    }
    pub fn register(&mut self, registration: StaffRegistration) -> Result<(), StaffError> {
        self.validate_registration(&registration)?;
        self.members
            .insert(registration.binding.actor, registration);
        self.selections
            .insert(registration.binding.actor, Default::default());
        Ok(())
    }
    pub fn remove(&mut self, binding: CharacterBinding) -> Result<(), StaffError> {
        if self
            .members
            .get(&binding.actor)
            .is_none_or(|r| r.binding != binding)
        {
            return Err(StaffError::NotBound);
        }
        self.members.remove(&binding.actor);
        self.selections.remove(&binding.actor);
        Ok(())
    }
    pub fn refresh(&mut self, registration: StaffRegistration) -> Result<(), StaffError> {
        if registration.privileges.account_access > 5 {
            return Err(StaffError::Invalid);
        }
        let member = self
            .members
            .get_mut(&registration.binding.actor)
            .ok_or(StaffError::NotBound)?;
        if member.binding != registration.binding {
            return Err(StaffError::NotBound);
        }
        *member = registration;
        Ok(())
    }
    pub(crate) fn authorize(&self, context: ActionContext) -> Result<StaffPrivileges, StaffError> {
        let member = self
            .members
            .get(&context.actor)
            .ok_or(StaffError::NotBound)?;
        if member.binding.session != context.session || member.binding.account != context.account {
            return Err(StaffError::NotBound);
        }
        if self.events.len() >= self.capacity {
            return Err(StaffError::Capacity);
        }
        Ok(member.privileges)
    }
    pub(crate) fn push(&mut self, event: StaffEvent) {
        self.events.push_back(event);
    }
    pub fn peek(&self) -> Option<&StaffEvent> {
        self.events.front()
    }
    pub fn take(&mut self) -> Option<StaffEvent> {
        self.events.pop_front()
    }
    pub fn pending_for(&self, actor: EntityId) -> bool {
        self.rewards
            .values()
            .any(|p| p.context.actor == actor || p.target == actor)
            || self.events.iter().any(|e| match e {
                StaffEvent::TargetQuery(event) => event.context.actor == actor,
                StaffEvent::GagProposal(p) => {
                    p.context.actor == actor || p.target.character == actor
                }
                StaffEvent::Broadcast {
                    context,
                    recipients,
                    ..
                } => context.actor == actor || recipients.iter().any(|r| r.actor == actor),
                StaffEvent::Scripts { context, targets } => {
                    context.actor == actor || targets.iter().any(|(id, _)| *id == actor)
                }
                StaffEvent::SpellProposal(ticket) => ticket.context.actor == actor,
                StaffEvent::Spellbook { context, .. } => context.actor == actor,
                StaffEvent::Outcome { actor: target, .. } => *target == Some(actor),
                StaffEvent::Inspection {
                    context, target, ..
                }
                | StaffEvent::Teleported {
                    context, target, ..
                }
                | StaffEvent::Healed {
                    context, target, ..
                } => context.actor == actor || *target == actor,
            })
    }
}
