//! Bounded accepted-social output composition. One event and one exact network
//! batch are retained through pressure; session counters remain with PlayerService.
use crate::{network::NetworkCommand, simulation::SimulationWorker};
use bace_admin::{ChatPublisher, FeedError};
use bace_gameplay_api::{
    CharacterBinding,
    social::{ChatChannel, SocialEvent},
};
use bace_replication::{BatchLimits, EventSequencer, Sequences};
use bace_session::SessionKey;
use bace_types::EntityId;
use std::sync::mpsc::Receiver;
struct Pending {
    event: SocialEvent,
    recipients: Vec<EntityId>,
    next: usize,
    published: bool,
}
pub struct SocialService {
    pending: Option<Pending>,
    network: Option<NetworkCommand>,
    publisher: Option<ChatPublisher>,
    feed_failures: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SocialPump {
    pub events: usize,
    pub recipients: usize,
    pub blocked: bool,
    pub feed_failures: u64,
}
pub struct RecoveredSocialService {
    pub event: Option<SocialEvent>,
    pub remaining_recipients: Vec<EntityId>,
    pub api_attempted: bool,
    pub network: Option<NetworkCommand>,
}
impl SocialService {
    pub fn new(publisher: Option<ChatPublisher>) -> Self {
        Self {
            pending: None,
            network: None,
            publisher,
            feed_failures: 0,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.network.is_some()
    }
    pub fn feed_failures(&self) -> u64 {
        self.feed_failures
    }
    pub fn pump(
        &mut self,
        worker: &SimulationWorker,
        budget: usize,
        project: impl FnMut(EntityId, &SocialEvent) -> Result<Option<NetworkCommand>, String>,
        send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<SocialPump, String> {
        self.pump_events(worker.social_events(), budget, project, send)
    }
    /// Keep admission-time social events until their accepted recipient has
    /// completed the entry receipt. The readiness callback does not project or
    /// advance a sequence, so retry preserves the exact event and order.
    pub fn pump_ready(
        &mut self,
        worker: &SimulationWorker,
        budget: usize,
        ready: impl FnMut(EntityId) -> bool,
        project: impl FnMut(EntityId, &SocialEvent) -> Result<Option<NetworkCommand>, String>,
        send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<SocialPump, String> {
        self.pump_events_ready(worker.social_events(), budget, ready, project, send)
    }
    pub fn pump_events(
        &mut self,
        events: &Receiver<SocialEvent>,
        budget: usize,
        project: impl FnMut(EntityId, &SocialEvent) -> Result<Option<NetworkCommand>, String>,
        send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<SocialPump, String> {
        self.pump_events_ready(events, budget, |_| true, project, send)
    }
    pub fn pump_events_ready(
        &mut self,
        events: &Receiver<SocialEvent>,
        budget: usize,
        mut ready: impl FnMut(EntityId) -> bool,
        mut project: impl FnMut(EntityId, &SocialEvent) -> Result<Option<NetworkCommand>, String>,
        mut send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<SocialPump, String> {
        if !(1..=4096).contains(&budget) {
            return Err("social dispatch budget outside bounds".into());
        }
        let mut report = SocialPump::default();
        for _ in 0..budget {
            if let Some(command) = self.network.take() {
                if let Err(command) = send(command) {
                    self.network = Some(command);
                    report.blocked = true;
                    break;
                }
                self.pending
                    .as_mut()
                    .ok_or("missing retained social event")?
                    .next += 1;
                report.recipients += 1;
                continue;
            }
            if self.pending.is_none() {
                let event = match events.try_recv() {
                    Ok(event) => event,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                };
                let recipients = recipients(&event);
                self.pending = Some(Pending {
                    event,
                    recipients,
                    next: 0,
                    published: false,
                });
            }
            let pending = self.pending.as_mut().expect("admitted pending event");
            if pending.recipients.len() > 4096 || pending.recipients.iter().any(|id| id.0 == 0) {
                return Err("invalid accepted social recipients".into());
            }
            if !pending.published {
                if let (Some(publisher), SocialEvent::Chat { accepted, .. }) =
                    (&self.publisher, &pending.event)
                    && matches!(
                        accepted.channel,
                        ChatChannel::General | ChatChannel::Trade | ChatChannel::Audit
                    )
                {
                    let publication = if accepted.channel == ChatChannel::Audit {
                        publisher.try_publish_audit(accepted)
                    } else {
                        publisher.publish(accepted)
                    };
                    match publication {
                        Ok(()) => {}
                        Err(FeedError::Full) if accepted.channel == ChatChannel::Audit => {
                            // No feed sequence was assigned. Keep the exact event
                            // and every recipient until the consumer makes room.
                            report.blocked = true;
                            break;
                        }
                        Err(FeedError::Closed) if accepted.channel == ChatChannel::Audit => {
                            // This is a degraded owner, not successful Audit
                            // delivery. Recovery still carries the same event.
                            return Err("accepted Audit feed publication closed".into());
                        }
                        Err(FeedError::Full | FeedError::Closed) => {
                            self.feed_failures = self.feed_failures.saturating_add(1);
                        }
                        Err(error) => {
                            return Err(format!("accepted public chat invalid: {error:?}"));
                        }
                    }
                }
                pending.published = true;
            }
            if pending.next == pending.recipients.len() {
                self.pending = None;
                report.events += 1;
                continue;
            }
            if !ready(pending.recipients[pending.next]) {
                report.blocked = true;
                break;
            }
            match project(pending.recipients[pending.next], &pending.event)? {
                Some(command) => self.network = Some(command),
                None => {
                    pending.next += 1;
                    report.recipients += 1;
                }
            }
        }
        report.feed_failures = self.feed_failures;
        Ok(report)
    }
    /// Drain this recovery value before admitting another owner of the same sessions.
    pub fn recover(self) -> RecoveredSocialService {
        let (event, remaining_recipients, api_attempted) =
            self.pending.map_or((None, vec![], false), |p| {
                (Some(p.event), p.recipients[p.next..].to_vec(), p.published)
            });
        RecoveredSocialService {
            event,
            remaining_recipients,
            api_attempted,
            network: self.network,
        }
    }
}
pub fn project_social_for_session(
    key: SessionKey,
    binding: CharacterBinding,
    event: &SocialEvent,
    events: &mut EventSequencer,
    properties: &mut Sequences,
    limits: BatchLimits,
) -> Result<NetworkCommand, String> {
    if key.generation != binding.session.0 {
        return Err("stale social session generation".into());
    }
    let batch = events
        .project_social_event(binding, event, properties, limits)
        .map_err(|e| format!("social projection: {e:?}"))?;
    crate::game_messages::session_batch_command(key, batch).map_err(|e| e.to_string())
}
fn recipients(event: &SocialEvent) -> Vec<EntityId> {
    match event {
        SocialEvent::Chat { recipients, .. }
        | SocialEvent::Fellowship { recipients, .. }
        | SocialEvent::FellowshipLeft { recipients, .. } => recipients.clone(),
        SocialEvent::Allegiance { recipient, .. }
        | SocialEvent::TurbineEcho { recipient, .. }
        | SocialEvent::Transient { recipient, .. }
        | SocialEvent::EquipmentMana { recipient, .. }
        | SocialEvent::System { recipient, .. }
        | SocialEvent::Error { recipient, .. }
        | SocialEvent::Friends { recipient, .. }
        | SocialEvent::Squelches { recipient, .. }
        | SocialEvent::TurbineResponse { recipient, .. }
        | SocialEvent::Channels { recipient, .. }
        | SocialEvent::Confirmation { recipient, .. }
        | SocialEvent::Afk { recipient, .. } => vec![*recipient],
    }
}
