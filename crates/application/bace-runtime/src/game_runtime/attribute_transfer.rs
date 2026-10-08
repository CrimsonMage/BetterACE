//! Source-prepared AttributeTransferDevice Use, Yes, exact durable operation,
//! and canonical private output. No client attribute values enter the owner.
mod output;
mod preparation;
use super::*;
use crate::{
    skill_saves::{SkillOperationId, SkillSaveOwner},
    skill_service::{SkillCompletion, SkillService},
};
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_simulation::{
    AttributeTransferCommand, AttributeTransferDeviceTicket, AttributeTransferOutcome,
    AttributeTransferResult, Command, PlayerSnapshotOutcome, PlayerSnapshotRequest,
    PreparedAttributeTransfer,
};
use bace_transport::ReceivedMessage;
use bace_types::EntityId;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;

#[derive(Clone)]
struct Quote {
    confirmation: bace_simulation::AttributeTransferConfirmation,
    prompt: String,
    name: String,
}
enum Phase {
    Capture,
    Capturing(u64),
    Captured(Arc<bace_simulation::PlayerReadSnapshot>),
    Ready(Option<AttributeTransferCommand>),
    Submitted,
    Prompt(Quote),
    Proposed,
    Saving,
    Finished(SkillCompletion),
    Rejected(bace_simulation::AttributeTransferDeviceError),
    Cancelled,
}
struct Pending {
    key: SessionKey,
    context: ActionContext,
    item: EntityId,
    identity: SkillOperationId,
    phase: Phase,
    quote: Option<Quote>,
    ticket: Option<AttributeTransferDeviceTicket>,
    proposal_seen: bool,
    request_retry: Option<AttributeTransferCommand>,
    use_action: bool,
}
pub(super) struct AttributeTransferRuntime {
    service: SkillService,
    pending: Option<Pending>,
    quotes: BTreeMap<SessionKey, Quote>,
    unexpected: Option<Box<AttributeTransferOutcome>>,
    unexpected_ticket: Option<AttributeTransferDeviceTicket>,
    unexpected_completion: Option<SkillCompletion>,
    failures: BTreeMap<SessionKey, String>,
}
impl AttributeTransferRuntime {
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
                .is_some_and(|pending| matches!(pending.phase,Phase::Capturing(correlation) if correlation == outcome.correlation))
    }
    pub(super) fn accept_capture(
        &mut self,
        outcome: PlayerSnapshotOutcome,
        unix_millis: u64,
    ) -> Result<(), PlayerSnapshotOutcome> {
        if self.service.owns_capture(&outcome) {
            return self.service.accept_capture(outcome, unix_millis);
        }
        let Some(pending) = self
            .pending
            .as_mut()
            .filter(|pending| matches!(pending.phase,Phase::Capturing(correlation) if correlation == outcome.correlation))
        else {
            return Err(outcome);
        };
        match &outcome.result {
            Ok(snapshot) if snapshot.binding() == binding(pending.context) => {
                pending.phase = Phase::Captured(snapshot.clone())
            }
            Err(_) => pending.phase = Phase::Capture,
            _ => return Err(outcome),
        }
        Ok(())
    }
    fn accept(
        &mut self,
        outcome: AttributeTransferOutcome,
    ) -> Result<(), Box<AttributeTransferOutcome>> {
        let outcome = match self.service.accept_attribute_transfer(outcome) {
            Ok(()) => return Ok(()),
            Err(outcome) => *outcome,
        };
        let Some(pending) = self.pending.as_mut().filter(|pending| {
            outcome.context == Some(pending.context) && matches!(pending.phase, Phase::Submitted)
        }) else {
            return Err(Box::new(outcome));
        };
        match outcome.result {
            Ok(AttributeTransferResult::Confirmation(confirmation))
                if confirmation.item == pending.item
                    && confirmation.actor == pending.context.actor
                    && confirmation.token > 0
                    && confirmation.token <= u64::from(u32::MAX)
                    && pending.quote.as_ref().is_some_and(|source| {
                        source.confirmation.device == confirmation.device
                    }) =>
            {
                let mut quote = pending.quote.clone().expect("source quote");
                quote.confirmation = confirmation;
                pending.phase = Phase::Prompt(quote);
            }
            Ok(AttributeTransferResult::Proposed(Some(ticket)))
                if ticket.character.context == pending.context =>
            {
                if pending
                    .ticket
                    .as_ref()
                    .is_some_and(|prior| prior != &ticket)
                {
                    return Err(Box::new(AttributeTransferOutcome {
                        context: Some(pending.context),
                        result: Ok(AttributeTransferResult::Proposed(Some(ticket))),
                    }));
                }
                pending.ticket = Some(ticket);
                pending.phase = Phase::Proposed;
            }
            Ok(AttributeTransferResult::Proposed(None)) => pending.phase = Phase::Cancelled,
            Err(bace_simulation::AttributeTransferDeviceError::Busy)
                if pending.request_retry.is_some() =>
            {
                pending.phase = Phase::Ready(pending.request_retry.take());
            }
            Err(error) => {
                self.failures
                    .insert(pending.key, format!("attribute transfer: {error:?}"));
                pending.phase = Phase::Rejected(error);
            }
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
}
fn binding(context: ActionContext) -> CharacterBinding {
    CharacterBinding {
        actor: context.actor,
        account: context.account,
        session: context.session,
    }
}
impl GameRuntime {
    pub(super) fn attribute_transfer_ingress_blocked(&self, key: SessionKey) -> bool {
        self.attribute_transfers
            .pending
            .as_ref()
            .is_some_and(|pending| pending.key == key)
    }
    pub(super) fn retry_attribute_transfer_session(&mut self, key: SessionKey) -> bool {
        if self.attribute_transfer_ingress_blocked(key)
            && self.attribute_transfers.service.blocked().is_some()
        {
            self.attribute_transfers.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn forget_attribute_transfer_session(
        &mut self,
        key: SessionKey,
    ) -> Result<(), String> {
        if self.attribute_transfer_ingress_blocked(key) {
            return Err("attribute transfer pending".into());
        }
        self.attribute_transfers.quotes.remove(&key);
        self.attribute_transfers.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_attribute_transfer_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<inventory::InventoryIngress, String> {
        use inventory::InventoryIngress as I;
        let Some(loading) = self
            .sessions
            .get(&key)
            .and_then(|session| session.loading.as_ref())
        else {
            return Ok(I::Unsupported);
        };
        let binding = loading.loaded.binding;
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let decoded = match bace_session::decode_skill_device(
            bace_session::SessionState::WorldConnected,
            context,
            &message.bytes,
            self.limits.message_bytes,
        ) {
            Ok(value) => value,
            Err(
                bace_session::DispatchError::UnsupportedAction(_)
                | bace_session::DispatchError::InvalidTarget(_),
            ) => return Ok(I::Unsupported),
            Err(error) => return Err(format!("attribute transfer input: {error:?}")),
        };
        match decoded.request {
            bace_session::SkillDeviceAction::Use { item } => {
                let item = EntityId(item);
                let Some(saved) = self
                    .online_saves
                    .inventory_baseline(context.actor.0, item.0)
                else {
                    return Ok(I::Unsupported);
                };
                if preparation::device(&saved.entity.state)?.is_none() {
                    return Ok(I::Unsupported);
                }
                if self.attribute_transfers.has_pending() {
                    return Ok(I::Blocked);
                }
                self.stage_attribute_transfer(key, context, item, Phase::Capture, None)?;
                Ok(I::Accepted)
            }
            bace_session::SkillDeviceAction::Confirmation {
                kind,
                token,
                accepted,
            } => {
                if kind != bace_session::SkillDeviceConfirmationType::AlterAttribute {
                    return Ok(I::Unsupported);
                }
                let Some(quote) = self.attribute_transfers.quotes.get(&key) else {
                    return Ok(I::Unsupported);
                };
                if u64::from(token) != quote.confirmation.token {
                    return Err("attribute transfer confirmation mismatch".into());
                }
                if self.attribute_transfers.has_pending() {
                    return Ok(I::Blocked);
                }
                let quote = quote.clone();
                self.attribute_transfers.quotes.remove(&key);
                self.stage_attribute_transfer(
                    key,
                    context,
                    quote.confirmation.item,
                    Phase::Ready(Some(AttributeTransferCommand::Confirm {
                        context,
                        token: u64::from(token),
                        accept: accepted,
                    })),
                    Some(quote),
                )?;
                Ok(I::Accepted)
            }
        }
    }
    fn stage_attribute_transfer(
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
            .map_err(|error| error.to_string())?;
        let identity = SkillOperationId::new(bytes).map_err(|error| error.to_string())?;
        let use_action = matches!(phase, Phase::Capture);
        self.attribute_transfers.pending = Some(Pending {
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
    pub(super) fn poll_attribute_transfers(&mut self) -> Result<(), String> {
        if self.attribute_transfers.unexpected.is_some()
            || self.attribute_transfers.unexpected_ticket.is_some()
            || self.attribute_transfers.unexpected_completion.is_some()
        {
            return Err("uncorrelated attribute transfer output retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.attribute_transfer_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.attribute_transfers.accept(outcome) {
                self.attribute_transfers.unexpected = Some(outcome);
                return Err("attribute transfer outcome correlation".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(ticket) = self.simulation.attribute_transfer_proposals().try_recv() else {
                break;
            };
            let Some(pending) = self.attribute_transfers.pending.as_mut().filter(|pending| {
                pending.context == ticket.character.context
                    && matches!(pending.phase, Phase::Submitted | Phase::Proposed)
            }) else {
                self.attribute_transfers.unexpected_ticket = Some(ticket);
                return Err("attribute transfer proposal correlation".into());
            };
            if pending.proposal_seen
                || pending
                    .ticket
                    .as_ref()
                    .is_some_and(|prior| prior != &ticket)
            {
                self.attribute_transfers.unexpected_ticket = Some(ticket);
                return Err("duplicate attribute transfer proposal".into());
            }
            pending.ticket = Some(ticket);
            pending.proposal_seen = true;
        }
        self.prepare_attribute_transfer_request()?;
        let input = self.simulation.input();
        if self
            .attribute_transfers
            .pending
            .as_ref()
            .is_some_and(|pending| matches!(pending.phase, Phase::Capture))
        {
            let correlation = self.token()?;
            let pending = self.attribute_transfers.pending.as_mut().expect("capture");
            match input.try_submit(Command::PlayerSnapshot(PlayerSnapshotRequest {
                correlation,
                binding: binding(pending.context),
                operation: None,
            })) {
                Ok(()) => pending.phase = Phase::Capturing(correlation),
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("attribute transfer capture lane closed".into());
                }
            }
        }
        if let Some(pending) = self.attribute_transfers.pending.as_mut() {
            if let Phase::Ready(command) = &mut pending.phase
                && let Some(value) = command.take()
            {
                if matches!(value, AttributeTransferCommand::RequestPrepared { .. }) {
                    pending.request_retry = Some(value.clone());
                }
                match input.try_submit(Command::AttributeTransfer(value)) {
                    Ok(()) => pending.phase = Phase::Submitted,
                    Err(TrySendError::Full(Command::AttributeTransfer(value))) => {
                        *command = Some(value)
                    }
                    Err(TrySendError::Disconnected(Command::AttributeTransfer(value))) => {
                        *command = Some(value);
                        return Err("attribute transfer owner closed".into());
                    }
                    _ => return Err("attribute transfer queue returned wrong command".into()),
                }
            }
            if matches!(pending.phase, Phase::Proposed) && pending.proposal_seen {
                self.attribute_transfers
                    .service
                    .stage(
                        pending.identity,
                        SkillSaveOwner::AttributeTransfer(
                            pending.ticket.clone().expect("proposal"),
                        ),
                    )
                    .map_err(|_| "attribute transfer service occupied")?;
                pending.phase = Phase::Saving;
            }
        }
        let correlation = self.token()?;
        let durable = self.attribute_transfers.service.poll(
            &input,
            &mut self.online_saves,
            &self.saves.handle,
            correlation,
        );
        if let Some(completion) = self.attribute_transfers.service.take_completion() {
            let pending = self
                .attribute_transfers
                .pending
                .as_mut()
                .ok_or("attribute transfer completion owner missing")?;
            if !matches!(pending.phase, Phase::Saving)
                || pending.ticket.as_ref().is_none_or(|ticket| {
                    completion.owner != SkillSaveOwner::AttributeTransfer(ticket.clone())
                })
            {
                self.attribute_transfers.unexpected_completion = Some(completion);
                return Err("attribute transfer completion mismatch retained".into());
            }
            pending.phase = Phase::Finished(completion);
        }
        self.project_attribute_transfer_output()?;
        durable
    }
    fn prepare_attribute_transfer_request(&mut self) -> Result<(), String> {
        let Some(pending) = self.attribute_transfers.pending.as_ref() else {
            return Ok(());
        };
        let Phase::Captured(snapshot) = &pending.phase else {
            return Ok(());
        };
        let all = self.online_saves.captured_inventory_baselines(snapshot)?;
        let row = all
            .iter()
            .find(|row| row.entity.object_id == pending.item.0)
            .ok_or("attribute device not in accepted inventory")?;
        let device = preparation::device(&row.entity.state)?
            .ok_or("accepted item no longer an attribute transfer device")?;
        let activation =
            crate::player_assets::prepare_item_activation_requirements(&row.entity.state)?;
        let mut wielded = Vec::new();
        for row in &all {
            if matches!(row.placement, Some(bace_storage_codec::ItemPlacementV2::Contained { equipped, .. }) if equipped != 0)
            {
                wielded.push((
                    EntityId(row.entity.object_id),
                    row.entity.mutation_revision,
                    preparation::wielded_attribute_requirement(&row.entity.state),
                ));
            }
        }
        let name = row
            .entity
            .state
            .properties
            .strings
            .iter()
            .find(|property| property.id == 1)
            .map(|property| property.value.clone())
            .ok_or("attribute device source name missing")?;
        let quote = Quote {
            confirmation: bace_simulation::AttributeTransferConfirmation {
                token: 0,
                actor: pending.context.actor,
                item: pending.item,
                expires: 0,
                device,
            },
            prompt: preparation::prompt(device),
            name,
        };
        let pending = self
            .attribute_transfers
            .pending
            .as_mut()
            .expect("captured owner");
        pending.quote = Some(quote);
        pending.phase = Phase::Ready(Some(AttributeTransferCommand::RequestPrepared {
            context: pending.context,
            item: pending.item,
            revision: row.entity.mutation_revision,
            device,
            activation: Some(activation),
            wielded,
            lifetime: 1800,
        }));
        Ok(())
    }
}
#[cfg(test)]
mod tests;
