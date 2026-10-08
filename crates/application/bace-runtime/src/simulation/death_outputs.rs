//! Player death output is retained on the sole owner until bounded publication.
use super::*;
use bace_simulation::{
    CorpseAccessOutcome, CorpseConsentOutcome, CorpseExpiryEvent, CorpseExpiryTicket,
    PlayerDeathEvent, PlayerDeathServiceOutcome, PlayerDeathTicket,
};
#[derive(Default)]
pub struct RecoveredPlayerDeaths {
    pub proposals: Vec<PlayerDeathTicket>,
    pub outcomes: Vec<PlayerDeathServiceOutcome>,
    pub events: Vec<PlayerDeathEvent>,
    pub corpse_expiry: Vec<CorpseExpiryTicket>,
    pub corpse_events: Vec<CorpseExpiryEvent>,
    pub corpse_access: Vec<CorpseAccessOutcome>,
    pub corpse_consent: Vec<CorpseConsentOutcome>,
}
impl RecoveredPlayerDeaths {
    pub(super) fn has_state(&self) -> bool {
        !self.proposals.is_empty()
            || !self.outcomes.is_empty()
            || !self.events.is_empty()
            || !self.corpse_expiry.is_empty()
            || !self.corpse_events.is_empty()
            || !self.corpse_access.is_empty()
            || !self.corpse_consent.is_empty()
    }
}
pub(super) struct DeathReceivers {
    outcomes: mpsc::Receiver<PlayerDeathServiceOutcome>,
    proposals: mpsc::Receiver<PlayerDeathTicket>,
    events: mpsc::Receiver<PlayerDeathEvent>,
    corpse_expiry: mpsc::Receiver<CorpseExpiryTicket>,
    corpse_events: mpsc::Receiver<CorpseExpiryEvent>,
    corpse_access: mpsc::Receiver<CorpseAccessOutcome>,
    corpse_consent: mpsc::Receiver<CorpseConsentOutcome>,
}
pub(super) struct DeathSenders {
    outcomes: mpsc::SyncSender<PlayerDeathServiceOutcome>,
    proposals: mpsc::SyncSender<PlayerDeathTicket>,
    events: mpsc::SyncSender<PlayerDeathEvent>,
    corpse_expiry: mpsc::SyncSender<CorpseExpiryTicket>,
    corpse_events: mpsc::SyncSender<CorpseExpiryEvent>,
    corpse_access: mpsc::SyncSender<CorpseAccessOutcome>,
    corpse_consent: mpsc::SyncSender<CorpseConsentOutcome>,
}
pub(super) fn channels(capacity: usize) -> (DeathSenders, DeathReceivers) {
    let (p, pr) = mpsc::sync_channel(capacity);
    let (o, or) = mpsc::sync_channel(1);
    let (e, er) = mpsc::sync_channel(capacity);
    let (c, cr) = mpsc::sync_channel(1);
    let (ce, cer) = mpsc::sync_channel(capacity);
    let (a, ar) = mpsc::sync_channel(capacity);
    let (consent, consent_rx) = mpsc::sync_channel(capacity);
    (
        DeathSenders {
            outcomes: o,
            proposals: p,
            events: e,
            corpse_expiry: c,
            corpse_events: ce,
            corpse_access: a,
            corpse_consent: consent,
        },
        DeathReceivers {
            outcomes: or,
            proposals: pr,
            events: er,
            corpse_expiry: cr,
            corpse_events: cer,
            corpse_access: ar,
            corpse_consent: consent_rx,
        },
    )
}
impl DeathSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(outcome) = kernel.take_corpse_consent_outcome() else {
                break;
            };
            if let Err(error) = self.corpse_consent.try_send(outcome) {
                let outcome = match error {
                    mpsc::TrySendError::Full(v) | mpsc::TrySendError::Disconnected(v) => v,
                };
                assert!(kernel.restore_corpse_consent_outcome(outcome).is_ok());
                break;
            }
        }
        for _ in 0..budget {
            let Some(outcome) = kernel.take_corpse_access_outcome() else {
                break;
            };
            if let Err(error) = self.corpse_access.try_send(outcome) {
                let outcome = match error {
                    mpsc::TrySendError::Full(v) | mpsc::TrySendError::Disconnected(v) => v,
                };
                assert!(kernel.restore_corpse_access_outcome(outcome).is_ok());
                break;
            }
        }
        if let Some(outcome) = kernel.take_player_death_service_outcome()
            && let Err(error) = self.outcomes.try_send(outcome)
        {
            let outcome = match error {
                mpsc::TrySendError::Full(v) | mpsc::TrySendError::Disconnected(v) => v,
            };
            assert!(kernel.restore_player_death_service_outcome(outcome).is_ok());
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_corpse_expiry_event() else {
                break;
            };
            if self.corpse_events.try_send(*event).is_err() {
                break;
            }
            kernel.take_corpse_expiry_event();
        }
        if let Some(ticket) = kernel.peek_corpse_expiry_proposal()
            && self.corpse_expiry.try_send(ticket.clone()).is_ok()
        {
            kernel.take_corpse_expiry_proposal();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_player_death_event() else {
                break;
            };
            if self.events.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_player_death_event();
        }
        for _ in 0..budget {
            let Some(ticket) = kernel.peek_player_death_proposal() else {
                break;
            };
            if self.proposals.try_send(ticket.clone()).is_err() {
                break;
            }
            kernel.take_player_death_proposal();
        }
    }
}
impl DeathReceivers {
    pub(super) fn recover(&self, out: &mut RecoveredPlayerDeaths) {
        out.events.extend(self.events.try_iter());
        out.outcomes.extend(self.outcomes.try_iter());
        out.proposals.extend(self.proposals.try_iter());
        out.corpse_expiry.extend(self.corpse_expiry.try_iter());
        out.corpse_events.extend(self.corpse_events.try_iter());
        out.corpse_access.extend(self.corpse_access.try_iter());
        out.corpse_consent.extend(self.corpse_consent.try_iter());
    }
}
impl SimulationWorker {
    pub fn corpse_access_outcomes(&self) -> &mpsc::Receiver<CorpseAccessOutcome> {
        &self.player_deaths.corpse_access
    }
    pub fn corpse_consent_outcomes(&self) -> &mpsc::Receiver<CorpseConsentOutcome> {
        &self.player_deaths.corpse_consent
    }
    pub fn player_death_service_outcomes(&self) -> &mpsc::Receiver<PlayerDeathServiceOutcome> {
        &self.player_deaths.outcomes
    }
    pub fn corpse_expiry_proposals(&self) -> &mpsc::Receiver<CorpseExpiryTicket> {
        &self.player_deaths.corpse_expiry
    }
    pub fn corpse_expiry_events(&self) -> &mpsc::Receiver<CorpseExpiryEvent> {
        &self.player_deaths.corpse_events
    }
    pub fn player_death_proposals(&self) -> &mpsc::Receiver<PlayerDeathTicket> {
        &self.player_deaths.proposals
    }
    pub fn player_death_events(&self) -> &mpsc::Receiver<PlayerDeathEvent> {
        &self.player_deaths.events
    }
}
