//! Generation-bound skill-device prompts and bounded response admission.
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_replication::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_session::{DispatchedSkillDevice, SkillDeviceAction, SkillDeviceConfirmationType};
use bace_simulation::{
    Command, PreparedSkillDevice, SkillDeviceCommand, SkillDeviceConfirmation, SkillDeviceOutcome,
    SkillDeviceResult,
};
use bace_types::EntityId;
use bace_wire::CraftingEvent;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillDeviceOutputError {
    InvalidConfirmation,
    WrongBinding,
    Expired,
    Capacity,
    Projection(SessionProjectionError),
}
#[derive(Debug, PartialEq, Eq)]
pub enum SkillDeviceAdmissionError<E> {
    Request(SkillDeviceOutputError),
    Admission(E),
}
pub struct SkillDevicePrompt<'a> {
    pub outcome: &'a SkillDeviceOutcome,
    /// Prepared server text; the wire packet never supplies prompt text.
    pub text: &'a str,
    pub now: u64,
}
struct Entry {
    binding: CharacterBinding,
    quote: SkillDeviceConfirmation,
}
pub struct SkillDeviceConfirmations {
    entries: BTreeMap<EntityId, Entry>,
    capacity: usize,
}
impl SkillDeviceConfirmations {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity: capacity.min(64),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Called only for the simulation's request outcome. Pending/committed skill
    /// changes are not confirmation prompts and never manufacture a prompt here.
    pub fn project(
        &mut self,
        sequencer: &mut EventSequencer,
        binding: CharacterBinding,
        prompt: SkillDevicePrompt<'_>,
        limits: BatchLimits,
    ) -> Result<Option<SessionBatch>, SkillDeviceOutputError> {
        let Ok(SkillDeviceResult::Confirmation(quote)) = prompt.outcome.result else {
            return Ok(None);
        };
        let context = prompt
            .outcome
            .context
            .ok_or(SkillDeviceOutputError::WrongBinding)?;
        if action_binding(context) != binding || quote.actor != binding.actor {
            return Err(SkillDeviceOutputError::WrongBinding);
        }
        let token =
            u32::try_from(quote.token).map_err(|_| SkillDeviceOutputError::InvalidConfirmation)?;
        if token == 0 {
            return Err(SkillDeviceOutputError::InvalidConfirmation);
        }
        if quote.expires <= prompt.now {
            return Err(SkillDeviceOutputError::Expired);
        }
        if self
            .entries
            .get(&binding.actor)
            .is_some_and(|entry| entry.binding == binding && entry.quote.token == quote.token)
        {
            return Err(SkillDeviceOutputError::InvalidConfirmation);
        }
        if self.capacity == 0
            || self.entries.len() >= self.capacity && !self.entries.contains_key(&binding.actor)
        {
            return Err(SkillDeviceOutputError::Capacity);
        }
        let batch = sequencer
            .project_crafting(
                binding,
                &[CraftingEvent::ConfirmationRequest {
                    confirmation_type: confirmation_kind(quote.device).wire_id(),
                    context: token,
                    text: prompt.text,
                }],
                limits,
            )
            .map_err(SkillDeviceOutputError::Projection)?;
        self.entries.insert(binding.actor, Entry { binding, quote });
        Ok(Some(batch))
    }
    /// Server object-kind routing selects this adapter for Use. Confirmations
    /// must match the exact prepared device/type/token and authenticated session.
    /// Failed bounded queue admission retains the quote; successful admission of
    /// either acceptance or refusal consumes it exactly once.
    pub fn submit_request<E>(
        &mut self,
        request: &DispatchedSkillDevice,
        now: u64,
        lifetime: u64,
        admit: impl FnOnce(Command) -> Result<(), E>,
    ) -> Result<(), SkillDeviceAdmissionError<E>> {
        use SkillDeviceAdmissionError::{Admission, Request};
        let action = match request.request {
            SkillDeviceAction::Use { item } => {
                if lifetime == 0 || lifetime > u64::from(u32::MAX) {
                    return Err(Request(SkillDeviceOutputError::InvalidConfirmation));
                }
                SkillDeviceCommand::Request {
                    context: request.context,
                    item: EntityId(item),
                    lifetime,
                }
            }
            SkillDeviceAction::Confirmation {
                kind,
                token,
                accepted,
            } => {
                let entry = self
                    .entries
                    .get(&request.context.actor)
                    .ok_or(Request(SkillDeviceOutputError::InvalidConfirmation))?;
                if action_binding(request.context) != entry.binding {
                    return Err(Request(SkillDeviceOutputError::WrongBinding));
                }
                if u64::from(token) != entry.quote.token
                    || kind != confirmation_kind(entry.quote.device)
                {
                    return Err(Request(SkillDeviceOutputError::InvalidConfirmation));
                }
                if entry.quote.expires <= now {
                    return Err(Request(SkillDeviceOutputError::Expired));
                }
                SkillDeviceCommand::Confirm {
                    context: request.context,
                    token: u64::from(token),
                    accept: accepted,
                }
            }
        };
        admit(Command::SkillDevice(action)).map_err(Admission)?;
        if matches!(request.request, SkillDeviceAction::Confirmation { .. }) {
            self.entries.remove(&request.context.actor);
        }
        Ok(())
    }
    pub fn invalidate(&mut self, binding: CharacterBinding) {
        if self
            .entries
            .get(&binding.actor)
            .is_some_and(|entry| entry.binding == binding)
        {
            self.entries.remove(&binding.actor);
        }
    }
    pub fn expire(&mut self, now: u64) {
        self.entries.retain(|_, entry| entry.quote.expires > now);
    }
}
fn action_binding(context: ActionContext) -> CharacterBinding {
    CharacterBinding {
        actor: context.actor,
        account: context.account,
        session: context.session,
    }
}
fn confirmation_kind(device: PreparedSkillDevice) -> SkillDeviceConfirmationType {
    match device {
        PreparedSkillDevice::Specialize(_) | PreparedSkillDevice::Lower(_) => {
            SkillDeviceConfirmationType::AlterSkill
        }
        PreparedSkillDevice::Augment { .. } => SkillDeviceConfirmationType::Augmentation,
    }
}
