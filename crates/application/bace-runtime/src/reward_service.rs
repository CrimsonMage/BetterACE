//! Accepted XP output composition with bounded recovery and canonical counters.
use crate::{network::NetworkCommand, player_service::PlayerService, simulation::SimulationWorker};
use bace_gameplay_api::{experience::ExperienceEvent, item_experience::ItemExperienceEvent};
use bace_replication::{BatchLimits, ReplicationMessage};
use bace_types::EntityId;
use std::{collections::VecDeque, sync::mpsc::Receiver};
#[derive(Clone, Debug)]
pub enum RewardOutput {
    Player(ExperienceEvent),
    Item(ItemExperienceEvent),
}
pub struct RewardProjection {
    pub actor: EntityId,
    pub owner: NetworkCommand,
    pub observers: Vec<ReplicationMessage>,
}
pub struct RewardRecovery {
    pub event: Option<RewardOutput>,
    pub projection: Option<RewardProjection>,
    pub commands: VecDeque<NetworkCommand>,
    pub routed: bool,
}
pub struct RewardService {
    event: Option<RewardOutput>,
    projection: Option<RewardProjection>,
    commands: VecDeque<NetworkCommand>,
    routed: bool,
    item_turn: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RewardPump {
    pub events: usize,
    pub commands: usize,
    pub blocked: bool,
}
impl Default for RewardService {
    fn default() -> Self {
        Self::new()
    }
}
impl RewardService {
    pub fn new() -> Self {
        Self {
            event: None,
            projection: None,
            commands: VecDeque::new(),
            routed: false,
            item_turn: true,
        }
    }
    pub fn has_pending(&self) -> bool {
        self.event.is_some() || self.projection.is_some() || !self.commands.is_empty()
    }
    pub fn pump(
        &mut self,
        worker: &SimulationWorker,
        budget: usize,
        project: impl FnMut(&RewardOutput) -> Result<Option<RewardProjection>, String>,
        observers: impl FnMut(&RewardProjection) -> Result<Vec<NetworkCommand>, String>,
        send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<RewardPump, String> {
        self.pump_events(
            worker.experience_events(),
            worker.item_experience_events(),
            budget,
            project,
            observers,
            send,
        )
    }
    pub fn pump_events(
        &mut self,
        players: &Receiver<ExperienceEvent>,
        items: &Receiver<ItemExperienceEvent>,
        budget: usize,
        mut project: impl FnMut(&RewardOutput) -> Result<Option<RewardProjection>, String>,
        mut observers: impl FnMut(&RewardProjection) -> Result<Vec<NetworkCommand>, String>,
        mut send: impl FnMut(NetworkCommand) -> Result<(), NetworkCommand>,
    ) -> Result<RewardPump, String> {
        if !(1..=4096).contains(&budget) {
            return Err("reward output budget outside bounds".into());
        }
        let mut report = RewardPump::default();
        for _ in 0..budget {
            if let Some(command) = self.commands.pop_front() {
                if let Err(command) = send(command) {
                    self.commands.push_front(command);
                    report.blocked = true;
                    break;
                }
                report.commands += 1;
                continue;
            }
            if self.routed {
                self.event = None;
                self.routed = false;
                report.events += 1;
                continue;
            }
            if let Some(projection) = &self.projection {
                let routed = observers(projection)?;
                if routed.len() > 4096 {
                    return Err("reward observer capacity".into());
                }
                let mut total = 0usize;
                for command in &routed {
                    let NetworkCommand::SendOrderedBatch { messages, .. } = command else {
                        return Err("reward observer requires ordered batch".into());
                    };
                    if messages.len() > 16 {
                        return Err("reward observer message capacity".into());
                    }
                    for (_, bytes) in messages {
                        total = total
                            .checked_add(bytes.len())
                            .ok_or("reward observer byte overflow")?;
                    }
                }
                if total > 16 * 1024 * 1024 {
                    return Err("reward observer byte capacity".into());
                }
                let projection = self.projection.take().expect("retained projection");
                self.commands.push_back(projection.owner);
                self.commands.extend(routed);
                self.routed = true;
                continue;
            }
            if let Some(event) = &self.event {
                self.projection = project(event)?;
                if self.projection.is_none() {
                    self.event = None;
                    report.events += 1;
                }
                continue;
            }
            self.event = if self.item_turn {
                items
                    .try_recv()
                    .ok()
                    .map(RewardOutput::Item)
                    .or_else(|| players.try_recv().ok().map(RewardOutput::Player))
            } else {
                players
                    .try_recv()
                    .ok()
                    .map(RewardOutput::Player)
                    .or_else(|| items.try_recv().ok().map(RewardOutput::Item))
            };
            self.item_turn = !self.item_turn;
            if self.event.is_none() {
                break;
            }
        }
        Ok(report)
    }
    pub fn recover(self) -> RewardRecovery {
        RewardRecovery {
            event: self.event,
            projection: self.projection,
            commands: self.commands,
            routed: self.routed,
        }
    }
}
pub fn project_reward_for_player(
    players: &mut PlayerService,
    event: &RewardOutput,
    limits: BatchLimits,
) -> Result<Option<RewardProjection>, String> {
    let actor = match event {
        RewardOutput::Player(e) => e.actor,
        RewardOutput::Item(e) => e.actor,
    };
    let Some(key) = players.replication(actor).map(|r| r.key) else {
        return Ok(None);
    };
    let (owner, observers) = match event {
        RewardOutput::Player(event) => {
            let batch = players
                .project_experience(event, limits)?
                .ok_or("player XP binding changed")?;
            (batch.owner, batch.observers)
        }
        RewardOutput::Item(event) => {
            let batch = players
                .project_item_experience(event, limits)?
                .ok_or("item XP binding changed")?;
            (batch.owner, batch.observers)
        }
    };
    Ok(Some(RewardProjection {
        actor,
        owner: crate::game_messages::session_batch_command(key, owner)
            .map_err(|e| e.to_string())?,
        observers,
    }))
}
