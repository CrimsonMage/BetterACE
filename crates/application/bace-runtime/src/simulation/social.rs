//! Social worker channels retain owner output until nonblocking publication succeeds.
use super::*;
use bace_gameplay_api::social::{SocialEvent, SocialOutcome};
use bace_simulation::{AllegianceTicket, SocialControlOutcome};
#[derive(Default)]
pub struct RecoveredSocial {
    pub events: Vec<SocialEvent>,
    pub outcomes: Vec<SocialOutcome>,
    pub controls: Vec<SocialControlOutcome>,
    pub allegiance: Vec<AllegianceTicket>,
}
impl RecoveredSocial {
    pub(super) fn has_state(&self) -> bool {
        !self.events.is_empty()
            || !self.outcomes.is_empty()
            || !self.controls.is_empty()
            || !self.allegiance.is_empty()
    }
}
pub(super) struct SocialReceivers {
    events: mpsc::Receiver<SocialEvent>,
    outcomes: mpsc::Receiver<SocialOutcome>,
    controls: mpsc::Receiver<SocialControlOutcome>,
    allegiance: mpsc::Receiver<AllegianceTicket>,
}
pub(super) struct SocialSenders {
    events: mpsc::SyncSender<SocialEvent>,
    outcomes: mpsc::SyncSender<SocialOutcome>,
    controls: mpsc::SyncSender<SocialControlOutcome>,
    allegiance: mpsc::SyncSender<AllegianceTicket>,
}
pub(super) fn channels(capacity: usize) -> (SocialSenders, SocialReceivers) {
    let (events, er) = mpsc::sync_channel(capacity);
    let (outcomes, or) = mpsc::sync_channel(capacity);
    let (controls, cr) = mpsc::sync_channel(capacity);
    let (allegiance, ar) = mpsc::sync_channel(capacity);
    (
        SocialSenders {
            events,
            outcomes,
            controls,
            allegiance,
        },
        SocialReceivers {
            events: er,
            outcomes: or,
            controls: cr,
            allegiance: ar,
        },
    )
}
impl SocialSenders {
    pub(super) fn flush(&self, kernel: &mut Kernel, budget: usize) {
        for _ in 0..budget {
            let Some(event) = kernel.peek_social_event() else {
                break;
            };
            if self.events.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_social_event();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_social_outcome() else {
                break;
            };
            if self.outcomes.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_social_outcome();
        }
        for _ in 0..budget {
            let Some(event) = kernel.peek_social_control_outcome() else {
                break;
            };
            if self.controls.try_send(event.clone()).is_err() {
                break;
            }
            kernel.take_social_control_outcome();
        }
        if let Some(ticket) = kernel.peek_allegiance_proposal()
            && self.allegiance.try_send(ticket.clone()).is_ok()
        {
            kernel.take_allegiance_proposal();
        }
    }
}
impl SocialReceivers {
    pub(super) fn recover(&self, out: &mut RecoveredSocial) {
        out.events.extend(self.events.try_iter());
        out.outcomes.extend(self.outcomes.try_iter());
        out.controls.extend(self.controls.try_iter());
        out.allegiance.extend(self.allegiance.try_iter());
    }
}
impl SimulationWorker {
    pub fn social_events(&self) -> &mpsc::Receiver<SocialEvent> {
        &self.social.events
    }
    pub fn social_outcomes(&self) -> &mpsc::Receiver<SocialOutcome> {
        &self.social.outcomes
    }
    pub fn social_control_outcomes(&self) -> &mpsc::Receiver<SocialControlOutcome> {
        &self.social.controls
    }
    pub fn allegiance_proposals(&self) -> &mpsc::Receiver<AllegianceTicket> {
        &self.social.allegiance
    }
}
pub(super) fn bounded(command: &Command) -> bool {
    use bace_gameplay_api::social::{
        AllegianceRequest as A, FellowshipRequest as F, SocialRequest as S,
    };
    match command {
        Command::SocialControl(c) => c.valid_bounds(),
        Command::SkillDevice(c) => c.valid_bounds(),
        Command::PlayerDeathService(c) => c.valid_bounds(),
        Command::Social { request, .. } => match request {
            S::Talk(t)
            | S::Emote(t)
            | S::SoulEmote(t)
            | S::Channel { text: t, .. }
            | S::Turbine { text: t, .. }
            | S::TalkDirect { text: t, .. } => t.len() <= 1024,
            S::Tell { text, target_name } => text.len() <= 1024 && target_name.len() <= 100,
            S::SetAfkMessage(t) => t.len() <= 4096,
            S::AddFriend(t)
            | S::AccountSquelch { name: t, .. }
            | S::CharacterSquelch { name: t, .. } => t.len() <= 100,
            _ => true,
        },
        Command::Fellowship {
            request: F::Create { name, .. },
            ..
        } => name.len() <= 255,
        Command::Allegiance { request, .. } => match request {
            A::SetMotd(t) => t.len() <= 4096,
            A::SetName(t) => t.len() <= 1024,
            A::SetOfficerTitle { title, .. } => title.len() <= 256,
            A::ChatBoot { name, reason } => name.len() <= 100 && reason.len() <= 4096,
            A::Info(t)
            | A::RemoveOfficer(t)
            | A::ApproveVassal(t)
            | A::AddBan(t)
            | A::RemoveBan(t)
            | A::SetOfficer { name: t, .. }
            | A::ChatGag { name: t, .. }
            | A::Boot { name: t, .. } => t.len() <= 100,
            _ => true,
        },
        _ => true,
    }
}
