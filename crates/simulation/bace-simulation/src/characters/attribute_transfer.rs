//! Character half of one durable attribute-transfer-device operation.
use super::*;
use bace_character::{AttributeTransferError, AttributeTransferProposal};
use bace_gameplay_api::AttributeId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttributeTransferTicket {
    pub operation: u64,
    pub context: ActionContext,
    pub proposal: AttributeTransferProposal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeTransferActionError {
    Authority(ProgressionActionRejection),
    Domain(AttributeTransferError),
    Busy,
    Capacity,
    Receipt,
}

pub(super) struct PendingAttributeTransfer {
    ticket: AttributeTransferTicket,
}

impl Characters {
    pub(crate) fn pending_attribute_transfer_operation(&self, actor: EntityId) -> Option<u64> {
        self.entries
            .get(&actor)?
            .attribute_transfer
            .as_ref()
            .map(|pending| pending.ticket.operation)
    }

    pub(crate) fn propose_attribute_transfer(
        &mut self,
        context: ActionContext,
        from: AttributeId,
        to: AttributeId,
        equipped_attribute_requirement: bool,
        actor_exists: bool,
    ) -> Result<AttributeTransferTicket, AttributeTransferActionError> {
        if self.reserved(context.actor) {
            return Err(AttributeTransferActionError::Busy);
        }
        self.authorize(context, actor_exists)
            .map_err(AttributeTransferActionError::Authority)?;
        let operation = self
            .next_attribute_transfer
            .checked_add(1)
            .ok_or(AttributeTransferActionError::Capacity)?;
        let entry = self
            .entries
            .get_mut(&context.actor)
            .ok_or(AttributeTransferActionError::Receipt)?;
        let proposal = entry
            .progression
            .propose_attribute_transfer(from, to, equipped_attribute_requirement)
            .map_err(AttributeTransferActionError::Domain)?;
        let ticket = AttributeTransferTicket {
            operation,
            context,
            proposal,
        };
        entry.attribute_transfer = Some(PendingAttributeTransfer { ticket });
        self.next_attribute_transfer = operation;
        Ok(ticket)
    }

    pub(crate) fn validate_attribute_transfer_ticket(
        &self,
        ticket: AttributeTransferTicket,
    ) -> Result<(), AttributeTransferActionError> {
        let entry = self
            .entries
            .get(&ticket.context.actor)
            .ok_or(AttributeTransferActionError::Receipt)?;
        if entry
            .attribute_transfer
            .as_ref()
            .is_none_or(|pending| pending.ticket != ticket)
            || entry.progression.revision() != ticket.proposal.expected_revision
            || entry
                .progression
                .projection(ticket.proposal.from_before.target)
                != Some(ticket.proposal.from_before)
            || entry
                .progression
                .projection(ticket.proposal.to_before.target)
                != Some(ticket.proposal.to_before)
        {
            return Err(AttributeTransferActionError::Receipt);
        }
        Ok(())
    }

    pub(crate) fn commit_attribute_transfer(
        &mut self,
        ticket: AttributeTransferTicket,
    ) -> Result<(), AttributeTransferActionError> {
        self.validate_attribute_transfer_ticket(ticket)?;
        let entry = self
            .entries
            .get_mut(&ticket.context.actor)
            .ok_or(AttributeTransferActionError::Receipt)?;
        entry
            .progression
            .adopt_attribute_transfer(ticket.proposal)
            .map_err(AttributeTransferActionError::Domain)?;
        entry.attribute_transfer = None;
        Ok(())
    }

    pub(crate) fn reject_attribute_transfer(
        &mut self,
        ticket: AttributeTransferTicket,
    ) -> Result<(), AttributeTransferActionError> {
        self.validate_attribute_transfer_ticket(ticket)?;
        self.entries
            .get_mut(&ticket.context.actor)
            .ok_or(AttributeTransferActionError::Receipt)?
            .attribute_transfer = None;
        Ok(())
    }
}
