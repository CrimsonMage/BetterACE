//! Source-owned skill-device Use, exact confirmation and atomic skill/item saves.
mod activation_response;
mod output;
mod preparation;
use super::*;
use crate::skill_saves::{SkillOperationId, SkillSaveOwner};
use crate::skill_service::{SkillCompletion, SkillService};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    Command, PlayerSnapshotOutcome, PlayerSnapshotRequest, PreparedSkillDevice, SkillDeviceCommand,
    SkillDeviceOutcome, SkillDeviceResult, SkillDeviceTicket,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;
#[derive(Clone)]
struct Quote {
    quote: bace_simulation::SkillDeviceConfirmation,
    prompt: String,
    name: String,
    activation_talk: Option<String>,
}
enum Phase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>),
    Ready(Option<SkillDeviceCommand>),
    Submitted,
    Prompt(Quote),
    Proposed,
    Saving,
    Finished(SkillCompletion),
    Inactive,
    Rejected(bace_simulation::SkillDeviceError),
    Cancelled,
}
struct Pending {
    key: SessionKey,
    context: ActionContext,
    item: EntityId,
    identity: SkillOperationId,
    phase: Phase,
    quote: Option<Quote>,
    ticket: Option<SkillDeviceTicket>,
    proposal_seen: bool,
    request_retry: Option<SkillDeviceCommand>,
    use_action: bool,
}
pub(super) struct SkillDeviceRuntime {
    pub(super) service: SkillService,
    pending: Option<Pending>,
    quotes: BTreeMap<SessionKey, Quote>,
    unexpected: Option<Box<SkillDeviceOutcome>>,
    unexpected_ticket: Option<SkillDeviceTicket>,
    unexpected_completion: Option<SkillCompletion>,
    failures: BTreeMap<SessionKey, String>,
}
impl SkillDeviceRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: SkillService::new(),
            pending: None,
            quotes: BTreeMap::new(),
            unexpected: None,
            unexpected_ticket: None,
            unexpected_completion: None,
            failures: BTreeMap::new(),
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.service.requires_drain()
            || self.unexpected.is_some()
            || self.unexpected_ticket.is_some()
            || self.unexpected_completion.is_some()
    }
    pub(super) fn owns_capture(&self, outcome: &PlayerSnapshotOutcome) -> bool {
        self.service.owns_capture(outcome)
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.service.owns_capture(&outcome) {
            return self.service.accept_capture(outcome, unix);
        }
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| matches!(p.phase,Phase::Capturing(c) if c==outcome.correlation))
        else {
            return Err(outcome);
        };
        match &outcome.result {
            Ok(s) if s.binding() == binding(p.context) => p.phase = Phase::Captured(s.clone()),
            Err(_) => p.phase = Phase::Capture,
            _ => return Err(outcome),
        }
        Ok(())
    }
    fn accept(&mut self, outcome: SkillDeviceOutcome) -> Result<(), Box<SkillDeviceOutcome>> {
        let outcome = match self.service.accept_device(outcome) {
            Ok(()) => return Ok(()),
            Err(v) => *v,
        };
        let Some(p) = self
            .pending
            .as_mut()
            .filter(|p| outcome.context == Some(p.context) && matches!(p.phase, Phase::Submitted))
        else {
            return Err(Box::new(outcome));
        };
        match outcome.result {
            Ok(SkillDeviceResult::Confirmation(q))
                if q.item == p.item
                    && q.actor == p.context.actor
                    && q.token > 0
                    && q.token <= u64::from(u32::MAX)
                    && p.quote
                        .as_ref()
                        .is_some_and(|source| source.quote.device == q.device) =>
            {
                let mut quote = p.quote.clone().expect("prepared source quote");
                quote.quote = q;
                p.phase = Phase::Prompt(quote);
            }
            Ok(SkillDeviceResult::Proposed(Some(ticket))) if ticket.skill.context == p.context => {
                if p.ticket.as_ref().is_some_and(|t| t != &ticket) {
                    return Err(Box::new(SkillDeviceOutcome {
                        context: Some(p.context),
                        result: Ok(SkillDeviceResult::Proposed(Some(ticket))),
                    }));
                }
                p.ticket = Some(ticket);
                p.phase = Phase::Proposed;
            }
            Ok(SkillDeviceResult::Proposed(None)) => p.phase = Phase::Cancelled,
            Ok(SkillDeviceResult::Inactive) if p.use_action && p.quote.is_none() => {
                p.phase = Phase::Inactive;
            }
            Err(bace_simulation::SkillDeviceError::Busy) if p.request_retry.is_some() => {
                p.phase = Phase::Ready(p.request_retry.take());
            }
            Err(e) => {
                self.failures.insert(p.key, format!("skill device: {e:?}"));
                p.phase = Phase::Rejected(e);
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
}
fn binding(c: ActionContext) -> CharacterBinding {
    CharacterBinding {
        actor: c.actor,
        account: c.account,
        session: c.session,
    }
}
impl GameRuntime {
    pub fn skill_device_failure(&self, key: SessionKey) -> Option<&str> {
        self.skill_devices
            .failures
            .get(&key)
            .map(String::as_str)
            .or_else(|| {
                self.skill_device_ingress_blocked(key)
                    .then(|| self.skill_devices.service.blocked())
                    .flatten()
            })
    }
    pub(super) fn skill_device_ingress_blocked(&self, key: SessionKey) -> bool {
        self.skill_devices
            .pending
            .as_ref()
            .is_some_and(|p| p.key == key)
    }
    pub(super) fn retry_skill_device(&mut self, key: SessionKey) -> bool {
        if self.skill_device_ingress_blocked(key) && self.skill_devices.service.blocked().is_some()
        {
            self.skill_devices.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn forget_skill_device_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.skill_device_ingress_blocked(key) {
            return Err("skill device pending".into());
        }
        self.skill_devices.quotes.remove(&key);
        self.skill_devices.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_skill_device_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(loading) = self.sessions.get(&key).and_then(|s| s.loading.as_ref()) else {
            return Ok(I::Unsupported);
        };
        let b = loading.loaded.binding;
        let context = ActionContext {
            actor: b.actor,
            account: b.account,
            session: b.session,
            sequence: message.sequence,
        };
        let decoded = match bace_session::decode_skill_device(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(v) => v,
            Err(
                bace_session::DispatchError::UnsupportedAction(_)
                | bace_session::DispatchError::InvalidTarget(_),
            ) => return Ok(I::Unsupported),
            Err(e) => return Err(format!("skill device input: {e:?}")),
        };
        match decoded.request {
            bace_session::SkillDeviceAction::Use { item } => {
                self.handle_skill_device_use(key, context, EntityId(item))
            }
            bace_session::SkillDeviceAction::Confirmation {
                kind,
                token,
                accepted,
            } => {
                let Some(q) = self.skill_devices.quotes.get(&key) else {
                    return Ok(I::Unsupported);
                };
                let expected = if matches!(q.quote.device, PreparedSkillDevice::Augment { .. }) {
                    6
                } else {
                    2
                };
                if kind.wire_id() != expected || u64::from(token) != q.quote.token {
                    return Err("skill device confirmation mismatch".into());
                }
                if self.skill_devices.has_pending() {
                    return Ok(I::Blocked);
                }
                let q = q.clone();
                self.skill_devices.quotes.remove(&key);
                self.stage_device(
                    key,
                    context,
                    q.quote.item,
                    Phase::Ready(Some(SkillDeviceCommand::Confirm {
                        context,
                        token: u64::from(token),
                        accept: accepted,
                    })),
                    Some(q),
                )?;
                Ok(I::Accepted)
            }
        }
    }
    pub(super) fn handle_skill_device_use(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        item: EntityId,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(saved) = self
            .online_saves
            .inventory_baseline(context.actor.0, item.0)
        else {
            return Ok(I::Unsupported);
        };
        let inactive = activation_response::inactive(&saved.entity.state);
        if !inactive && preparation::device(&saved.entity.state)?.is_none()
            || inactive && !matches!(saved.entity.state.weenie_type, 62 | 67)
        {
            return Ok(I::Unsupported);
        }
        if self.skill_devices.has_pending() {
            return Ok(I::Blocked);
        }
        self.stage_device(key, context, item, Phase::Capture, None)?;
        Ok(I::Accepted)
    }
    fn stage_device(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        item: EntityId,
        phase: Phase,
        quote: Option<Quote>,
    ) -> Result<(), String> {
        let mut bytes = [0; 16];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|e| e.to_string())?;
        let identity = SkillOperationId::new(bytes).map_err(|e| e.to_string())?;
        let use_action = matches!(phase, Phase::Capture);
        self.skill_devices.pending = Some(Pending {
            key,
            context,
            item,
            identity,
            phase,
            quote,
            ticket: None,
            proposal_seen: false,
            request_retry: None,
            use_action,
        });
        Ok(())
    }
    pub(super) fn poll_skill_devices(&mut self) -> Result<(), String> {
        if self.skill_devices.unexpected.is_some() || self.skill_devices.unexpected_ticket.is_some()
        {
            return Err("uncorrelated skill device owner output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.skill_device_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.skill_devices.accept(outcome) {
                self.skill_devices.unexpected = Some(outcome);
                return Err("skill device outcome correlation".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(ticket) = self.simulation.skill_device_proposals().try_recv() else {
                break;
            };
            let Some(p) = self.skill_devices.pending.as_mut().filter(|p| {
                p.context == ticket.skill.context
                    && matches!(p.phase, Phase::Submitted | Phase::Proposed)
            }) else {
                self.skill_devices.unexpected_ticket = Some(ticket);
                return Err("skill device proposal correlation".into());
            };
            if p.proposal_seen || p.ticket.as_ref().is_some_and(|t| t != &ticket) {
                self.skill_devices.unexpected_ticket = Some(ticket);
                return Err("duplicate skill device proposal".into());
            }
            p.ticket = Some(ticket);
            p.proposal_seen = true;
        }
        self.prepare_skill_device_request()?;
        let input = self.simulation.input();
        if self
            .skill_devices
            .pending
            .as_ref()
            .is_some_and(|p| matches!(p.phase, Phase::Capture))
        {
            let correlation = self.token()?;
            let p = self.skill_devices.pending.as_mut().expect("capture");
            match input.try_submit(Command::PlayerSnapshot(PlayerSnapshotRequest {
                correlation,
                binding: binding(p.context),
                operation: None,
            })) {
                Ok(()) => p.phase = Phase::Capturing(correlation),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("skill device capture lane closed".into());
                }
            }
        }
        if let Some(p) = self.skill_devices.pending.as_mut() {
            if let Phase::Ready(command) = &mut p.phase
                && let Some(c) = command.take()
            {
                if matches!(c, SkillDeviceCommand::RequestPrepared { .. }) {
                    p.request_retry = Some(c.clone());
                }
                match input.try_submit(Command::SkillDevice(c)) {
                    Ok(()) => p.phase = Phase::Submitted,
                    Err(TrySendError::Full(Command::SkillDevice(c))) => *command = Some(c),
                    Err(TrySendError::Disconnected(Command::SkillDevice(c))) => {
                        *command = Some(c);
                        return Err("skill device owner closed".into());
                    }
                    _ => return Err("skill device queue returned wrong command".into()),
                }
            }
            if matches!(p.phase, Phase::Proposed) && p.proposal_seen {
                self.skill_devices
                    .service
                    .stage(
                        p.identity,
                        SkillSaveOwner::Device(p.ticket.clone().expect("proposal")),
                    )
                    .map_err(|_| "skill device service occupied")?;
                p.phase = Phase::Saving;
            }
        }
        let correlation = self.token()?;
        let durable = self.skill_devices.service.poll(
            &input,
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        );
        if let Some(completion) = self.skill_devices.service.take_completion() {
            let p = self
                .skill_devices
                .pending
                .as_mut()
                .ok_or("device completion owner missing")?;
            if !matches!(p.phase, Phase::Saving)
                || p.ticket
                    .as_ref()
                    .is_none_or(|t| completion.owner != SkillSaveOwner::Device(t.clone()))
            {
                self.skill_devices.unexpected_completion = Some(completion);
                return Err("device completion mismatch retained".into());
            }
            p.phase = Phase::Finished(completion);
        }
        self.project_skill_device_output()?;
        durable
    }
    fn prepare_skill_device_request(&mut self) -> Result<(), String> {
        let Some(p) = self.skill_devices.pending.as_ref() else {
            return Ok(());
        };
        let Phase::Captured(snapshot) = &p.phase else {
            return Ok(());
        };
        let all = self.online_saves.captured_inventory_baselines(snapshot)?;
        let row = all
            .iter()
            .find(|r| r.entity.object_id == p.item.0)
            .ok_or("device not in accepted inventory")?;
        if activation_response::inactive(&row.entity.state) {
            if !matches!(row.entity.state.weenie_type, 62 | 67) {
                return Err("inactive skill device subtype changed".into());
            }
            let p = self.skill_devices.pending.as_mut().expect("captured owner");
            p.phase = Phase::Ready(Some(SkillDeviceCommand::RequestInactive {
                context: p.context,
                item: p.item,
                revision: row.entity.mutation_revision,
            }));
            return Ok(());
        }
        let device = preparation::device(&row.entity.state)?
            .ok_or("accepted item no longer a skill device")?;
        let activation =
            crate::player_assets::prepare_item_activation_requirements(&row.entity.state)?;
        let cooldown_seconds = preparation::cooldown_seconds(&row.entity.state)?;
        let mut wielded = Vec::new();
        for r in &all {
            if matches!(r.placement,Some(bace_storage_codec::ItemPlacementV2::Contained{equipped,..}) if equipped!=0)
            {
                wielded.push((
                    EntityId(r.entity.object_id),
                    r.entity.mutation_revision,
                    preparation::requirements(&r.entity.state)?,
                ));
            }
        }
        let skill = match device {
            PreparedSkillDevice::Specialize(s)
            | PreparedSkillDevice::Lower(s)
            | PreparedSkillDevice::Augment { skill: s, .. } => s,
        };
        let assets = self
            .sessions
            .get(&p.key)
            .and_then(|s| s.loading.as_ref())
            .and_then(|l| l.character_assets.as_ref())
            .ok_or("skill device DAT definitions missing")?;
        let definition = assets
            .skill_table()
            .skills
            .get(&skill)
            .ok_or("skill definition missing")?;
        let class = snapshot
            .skill_values()
            .iter()
            .find(|s| s.skill == skill)
            .ok_or("skill values missing")?
            .advancement;
        let name = row
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|v| v.id == 1)
            .map(|v| v.value.clone())
            .ok_or("device source name missing")?;
        let prompt = preparation::prompt(device, &name, definition, class)?;
        let activation_talk =
            activation_response::talk(&row.entity.state, self.limits.message_bytes)?;
        let revision = row.entity.mutation_revision;
        let p = self.skill_devices.pending.as_mut().expect("captured owner");
        p.quote = Some(Quote {
            quote: bace_simulation::SkillDeviceConfirmation {
                token: 0,
                actor: p.context.actor,
                item: p.item,
                expires: 0,
                device,
            },
            prompt,
            name,
            activation_talk,
        });
        p.phase = Phase::Ready(Some(SkillDeviceCommand::RequestPrepared {
            context: p.context,
            item: p.item,
            revision,
            device,
            activation: Some(activation),
            cooldown_seconds,
            wielded,
            lifetime: 1800,
        }));
        Ok(())
    }
}
#[cfg(test)]
mod tests;
