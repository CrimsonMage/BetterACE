//! Bounded social owner queues. Preferences have the character aggregate's revision.
use bace_gameplay_api::social::{SocialEvent, SocialOutcome};
use bace_social::SocialDirectory;
use bace_types::EntityId;
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Debug)]
pub enum SocialConfirmation {
    Fellowship {
        inviter: EntityId,
        target: EntityId,
        fellowship: u64,
        revision: u64,
    },
    Allegiance {
        vassal: EntityId,
        patron: EntityId,
        revision: u64,
    },
}
#[derive(Clone, Debug)]
pub struct PendingSocialConfirmation {
    pub token: u32,
    pub expires_tick: u64,
    pub confirmation: SocialConfirmation,
}
pub(crate) struct SocialState {
    pub(crate) chat_policy: bace_social::ChatPolicy,
    pub(crate) chat_eligibility: BTreeMap<EntityId, (bace_social::ChatEligibility, u64)>,
    pub(crate) directory: SocialDirectory,
    pub(crate) events: VecDeque<SocialEvent>,
    pub(crate) outcomes: VecDeque<SocialOutcome>,
    pub(crate) controls: VecDeque<crate::SocialControlOutcome>,
    pub(crate) confirmations: BTreeMap<EntityId, PendingSocialConfirmation>,
    pub(crate) capacity: usize,
    pub(crate) sequence: u64,
    pub(crate) next_confirmation: u32,
    pub(crate) epoch: i64,
    pub(crate) random: Option<std::sync::Arc<bace_random::RandomRoot>>,
}
impl SocialState {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            chat_policy: Default::default(),
            chat_eligibility: BTreeMap::new(),
            directory: SocialDirectory::new(capacity.max(1)).expect("bounded kernel capacity"),
            events: VecDeque::new(),
            outcomes: VecDeque::new(),
            controls: VecDeque::new(),
            confirmations: BTreeMap::new(),
            capacity,
            sequence: 0,
            next_confirmation: 0,
            epoch: 0,
            random: None,
        }
    }
    pub(crate) fn has_state(&self) -> bool {
        self.directory.online().next().is_some()
            || !self.events.is_empty()
            || !self.outcomes.is_empty()
            || !self.controls.is_empty()
            || !self.confirmations.is_empty()
    }
}
