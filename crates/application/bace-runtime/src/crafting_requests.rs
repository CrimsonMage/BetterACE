//! Bounded server-issued confirmation tokens. No clocks, I/O, or random draws.
use bace_gameplay_api::{ActionContext, CharacterBinding};
use bace_session::DispatchedCrafting;
use bace_simulation::{
    CraftingCommand, CraftingCommandKind, CraftingOutcome, CraftingResult, TinkerCommandInput,
};
use bace_wire::CraftingAction;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CraftingRequestError {
    Capacity,
    InvalidQuote,
    InvalidConfirmation,
    WrongBinding,
    Expired,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CraftingAdmissionError<E> {
    Request(CraftingRequestError),
    Admission(E),
}
struct TokenEntry {
    binding: CharacterBinding,
    input: Box<TinkerCommandInput>,
    issued_at: u64,
    expires_at: u64,
}
/// The adapter retains the exact prepared inputs while a quote command is in
/// flight. Register only its successful correlated outcome, then encode the
/// returned token in CraftingEvent::ConfirmationRequest (type 5).
pub struct CraftingConfirmations {
    entries: BTreeMap<u32, TokenEntry>,
    next: u64,
    capacity: usize,
}
impl CraftingConfirmations {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            next: 1,
            capacity: capacity.min(64),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// issued_at/expires_at are explicit simulation ticks for the original
    /// quote, not a new lifetime beginning when this adapter receives a reply.
    pub fn register_quoted(
        &mut self,
        binding: CharacterBinding,
        input: Box<TinkerCommandInput>,
        issued_at: u64,
        expires_at: u64,
        expected_correlation: u64,
        outcome: &CraftingOutcome,
    ) -> Result<u32, CraftingRequestError> {
        let Ok(CraftingResult::Quoted(chance)) = outcome.result else {
            return Err(CraftingRequestError::InvalidQuote);
        };
        if outcome.correlation != expected_correlation
            || input.context.actor != binding.actor.0
            || expires_at <= issued_at
            || expires_at - issued_at > 1800
            || bace_crafting::tinker_chance(input.context.chance).ok() != Some(chance)
        {
            return Err(CraftingRequestError::InvalidQuote);
        }
        let command = CraftingCommand {
            correlation: expected_correlation,
            action: CraftingCommandKind::Confirm {
                context: ActionContext {
                    actor: binding.actor,
                    account: binding.account,
                    session: binding.session,
                    sequence: 0,
                },
                input,
            },
        };
        command
            .validate_bounds()
            .map_err(|_| CraftingRequestError::InvalidQuote)?;
        let CraftingCommandKind::Confirm { input, .. } = command.action else {
            return Err(CraftingRequestError::InvalidQuote);
        };
        if self.next > u64::from(u32::MAX)
            || self.capacity == 0
            || self.entries.len() >= self.capacity
                && !self
                    .entries
                    .values()
                    .any(|entry| entry.binding.actor == binding.actor)
        {
            return Err(CraftingRequestError::Capacity);
        }
        let token = self.next as u32;
        self.next += 1;
        // One active quote per actor, matching the simulation owner. Tokens are
        // never reused in this coordinator, even across authentication changes.
        self.entries
            .retain(|_, entry| entry.binding.actor != binding.actor);
        self.entries.insert(
            token,
            TokenEntry {
                binding,
                input,
                issued_at,
                expires_at,
            },
        );
        Ok(token)
    }
    /// Tokens remain retryable on bounded-queue backpressure. Only successful
    /// command admission consumes one, including an explicitly declined quote.
    pub fn submit_response<E>(
        &mut self,
        request: &DispatchedCrafting,
        now: u64,
        correlation: u64,
        admit: impl FnOnce(CraftingCommand) -> Result<(), E>,
    ) -> Result<(), CraftingAdmissionError<E>> {
        use CraftingAdmissionError::{Admission, Request};
        let CraftingAction::Confirmation {
            confirmation_type: 5,
            context: token,
            accepted,
        } = request.request
        else {
            return Err(Request(CraftingRequestError::InvalidConfirmation));
        };
        let entry = self
            .entries
            .get(&token)
            .ok_or(Request(CraftingRequestError::InvalidConfirmation))?;
        let binding = CharacterBinding {
            actor: request.context.actor,
            account: request.context.account,
            session: request.context.session,
        };
        if binding != entry.binding {
            return Err(Request(CraftingRequestError::WrongBinding));
        }
        if now < entry.issued_at {
            return Err(Request(CraftingRequestError::InvalidConfirmation));
        }
        if now >= entry.expires_at {
            return Err(Request(CraftingRequestError::Expired));
        }
        let action = if accepted {
            CraftingCommandKind::Confirm {
                context: request.context,
                input: entry.input.clone(),
            }
        } else {
            CraftingCommandKind::Cancel {
                context: request.context,
            }
        };
        admit(CraftingCommand {
            correlation,
            action,
        })
        .map_err(Admission)?;
        self.entries.remove(&token);
        Ok(())
    }
    /// Generation-fenced logout/replacement cleanup; cannot erase a newer login.
    pub fn invalidate(&mut self, binding: CharacterBinding) {
        self.entries.retain(|_, entry| entry.binding != binding);
    }
    pub fn expire(&mut self, now: u64) {
        self.entries.retain(|_, entry| entry.expires_at > now);
    }
}
