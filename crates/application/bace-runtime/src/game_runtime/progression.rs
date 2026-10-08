//! Authenticated UI and progression ingress. Each session retains one action
//! through owner acknowledgment; valuable training also waits for durable commit.
use super::*;
use crate::{
    skill_saves::{SkillOperationId, SkillSaveOwner},
    skill_service::SkillService,
};
use bace_gameplay_api::{
    ActionContext, ProgressionActionRejection, ProgressionChange, RaiseProgression, TrainSkill,
    UiRequest,
};
use bace_replication::ProgressionCursor;
use bace_simulation::{
    Command, SkillActionError, SkillOutcome, SkillStage, SkillTicket, UiOutcome,
};
use bace_transport::ReceivedMessage;
use rand_core::{OsRng, RngCore};
use std::sync::mpsc::TrySendError;

#[derive(Clone)]
enum Action {
    Raise(RaiseProgression),
    Ui(UiRequest),
    Train(TrainSkill, SkillOperationId),
}
impl Action {
    fn command(&self, context: ActionContext) -> Command {
        match self {
            Self::Raise(request) => Command::RaiseProgression {
                context,
                request: *request,
            },
            Self::Ui(request) => Command::Ui {
                context,
                request: request.clone(),
            },
            Self::Train(request, _) => Command::TrainSkill {
                context,
                request: *request,
            },
        }
    }
}
enum Phase {
    Queued,
    Submitted,
    Proposed(SkillTicket),
    Saving,
    Raised(ProgressionChange),
    Trained(SkillTicket),
}
struct Pending {
    context: ActionContext,
    action: Action,
    phase: Phase,
}
enum Unexpected {
    Raise(bace_gameplay_api::ProgressionOutcome),
    Ui(UiOutcome),
    Skill(SkillOutcome),
}
pub(super) struct ProgressionRuntime {
    pub(super) service: SkillService,
    pending: BTreeMap<SessionKey, Pending>,
    cursors: BTreeMap<SessionKey, ProgressionCursor>,
    failures: BTreeMap<SessionKey, String>,
    unexpected: Option<Unexpected>,
    completion: Option<crate::skill_service::SkillCompletion>,
    ingress_cursor: Option<SessionKey>,
    output_cursor: Option<SessionKey>,
}
impl ProgressionRuntime {
    pub(super) fn new() -> Self {
        Self {
            service: SkillService::new(),
            pending: BTreeMap::new(),
            cursors: BTreeMap::new(),
            failures: BTreeMap::new(),
            unexpected: None,
            completion: None,
            ingress_cursor: None,
            output_cursor: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.service.requires_drain()
            || !self.pending.is_empty()
            || self.unexpected.is_some()
            || self.completion.is_some()
    }
    fn training_busy(&self) -> bool {
        self.service.requires_drain()
            || self
                .pending
                .values()
                .any(|p| matches!(p.action, Action::Train(..)))
    }
    fn matched(&self, context: ActionContext, kind: u8) -> Option<SessionKey> {
        self.pending.iter().find_map(|(key, p)| {
            let matches = matches!(
                (&p.action, kind),
                (Action::Raise(_), 0) | (Action::Ui(_), 1) | (Action::Train(..), 2)
            );
            (matches && p.context == context && matches!(p.phase, Phase::Submitted)).then_some(*key)
        })
    }
    fn reject(&mut self, key: SessionKey, error: impl std::fmt::Debug) {
        self.failures.insert(key, format!("{error:?}"));
        self.pending.remove(&key);
    }
    fn accept_raise(
        &mut self,
        outcome: bace_gameplay_api::ProgressionOutcome,
    ) -> Result<(), Box<bace_gameplay_api::ProgressionOutcome>> {
        let Some(key) = self.matched(outcome.context, 0) else {
            return Err(Box::new(outcome));
        };
        match outcome.result {
            Ok(change) => {
                self.pending.get_mut(&key).expect("matched action").phase = Phase::Raised(change)
            }
            Err(ProgressionActionRejection::DurabilityPending) => {
                self.pending.get_mut(&key).expect("matched action").phase = Phase::Queued
            }
            Err(error) => self.reject(key, error),
        }
        Ok(())
    }
    fn accept_ui(&mut self, outcome: UiOutcome) -> Result<(), UiOutcome> {
        let Some(key) = self.matched(outcome.context, 1) else {
            return Err(outcome);
        };
        match outcome.result {
            Err(bace_gameplay_api::UiError::DurabilityPending) => {
                self.pending.get_mut(&key).expect("matched action").phase = Phase::Queued
            }
            Err(error) => self.reject(key, error),
            Ok(_) => {
                self.pending.remove(&key);
            }
        }
        Ok(())
    }
    fn accept_skill(&mut self, outcome: SkillOutcome) -> Result<(), Box<SkillOutcome>> {
        let outcome = match self.service.accept_skill(outcome) {
            Ok(()) => return Ok(()),
            Err(outcome) => *outcome,
        };
        let Some(key) = self.matched(outcome.context, 2) else {
            return Err(Box::new(outcome));
        };
        match outcome.result {
            Ok(SkillStage::Proposed(ticket)) if ticket.context == outcome.context => {
                self.pending.get_mut(&key).expect("matched action").phase = Phase::Proposed(ticket)
            }
            Err(SkillActionError::Busy) => {
                self.pending.get_mut(&key).expect("matched action").phase = Phase::Queued
            }
            Err(error) => self.reject(key, error),
            _ => return Err(Box::new(outcome)),
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgressionIngress {
    Accepted,
    Blocked,
    Unsupported,
}
impl GameRuntime {
    pub fn progression_failure(&self, key: SessionKey) -> Option<&str> {
        self.progression.failures.get(&key).map(String::as_str)
    }
    pub(super) fn progression_ingress_blocked(&self, key: SessionKey) -> bool {
        self.progression.pending.contains_key(&key)
    }
    pub(super) fn retry_progression(&mut self, key: SessionKey) -> bool {
        if self
            .progression
            .pending
            .get(&key)
            .is_some_and(|p| matches!(p.phase, Phase::Saving))
            && self.progression.service.blocked().is_some()
        {
            self.progression.service.retry();
            true
        } else {
            false
        }
    }
    pub(super) fn forget_progression_session(&mut self, key: SessionKey) -> Result<(), String> {
        if self.progression_ingress_blocked(key) {
            return Err("progression action remains pending during logout".into());
        }
        self.progression.cursors.remove(&key);
        self.progression.failures.remove(&key);
        Ok(())
    }
    pub(super) fn handle_progression_message(
        &mut self,
        key: SessionKey,
        message: &ReceivedMessage,
    ) -> Result<ProgressionIngress, String> {
        if self.progression_ingress_blocked(key) {
            return Ok(ProgressionIngress::Blocked);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(ProgressionIngress::Blocked);
        };
        let Some(loading) = &session.loading else {
            return Ok(ProgressionIngress::Unsupported);
        };
        if !self.players.entered(loading.loaded.binding.actor)
            || session.terminated
            || session.disconnected
        {
            return Ok(ProgressionIngress::Blocked);
        }
        let binding = loading.loaded.binding;
        if binding.account != session.account.id || binding.session.0 != key.generation {
            return Err("progression authenticated binding mismatch".into());
        }
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: message.sequence,
        };
        let Some(action) = decode_action(
            context,
            &message.bytes,
            self.limits.message_bytes,
            self.progression.training_busy(),
        )?
        else {
            return Ok(ProgressionIngress::Unsupported);
        };
        let Some(action) = action else {
            return Ok(ProgressionIngress::Blocked);
        };
        if self.progression.pending.len() >= self.limits.sessions {
            return Ok(ProgressionIngress::Blocked);
        }
        self.progression
            .cursors
            .entry(key)
            .or_insert_with(|| ProgressionCursor::new(binding, 0));
        self.progression.failures.remove(&key);
        self.progression.pending.insert(
            key,
            Pending {
                context,
                action,
                phase: Phase::Queued,
            },
        );
        Ok(ProgressionIngress::Accepted)
    }
    pub(super) fn poll_progression(&mut self) -> Result<(), String> {
        if let Some(unexpected) = &self.progression.unexpected {
            let context = match unexpected {
                Unexpected::Raise(o) => o.context,
                Unexpected::Ui(o) => o.context,
                Unexpected::Skill(o) => o.context,
            };
            return Err(format!(
                "uncorrelated progression outcome retained: {context:?}"
            ));
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.progression.accept_raise(outcome) {
                self.progression.unexpected = Some(Unexpected::Raise(*outcome));
                return Err("uncorrelated progression outcome retained".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.ui_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.progression.accept_ui(outcome) {
                self.progression.unexpected = Some(Unexpected::Ui(outcome));
                return Err("uncorrelated UI outcome retained".into());
            }
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.skill_outcomes().try_recv() else {
                break;
            };
            if let Err(outcome) = self.progression.accept_skill(outcome) {
                self.progression.unexpected = Some(Unexpected::Skill(*outcome));
                return Err("uncorrelated skill outcome retained".into());
            }
        }
        let input = self.simulation.input();
        let keys = select_pending(
            &self.progression.pending,
            self.progression.ingress_cursor,
            self.limits.work_per_poll,
            |p| matches!(p, Phase::Queued | Phase::Proposed(_)),
        );
        for key in keys {
            self.progression.ingress_cursor = Some(key);
            let pending = self
                .progression
                .pending
                .get_mut(&key)
                .expect("selected pending action");
            match pending.phase {
                Phase::Queued => match input.try_submit(pending.action.command(pending.context)) {
                    Ok(()) => pending.phase = Phase::Submitted,
                    Err(TrySendError::Full(_)) => break,
                    Err(TrySendError::Disconnected(_)) => {
                        return Err("progression owner ingress closed".into());
                    }
                },
                Phase::Proposed(ticket) => {
                    let Action::Train(_, identity) = pending.action else {
                        return Err("skill proposal action mismatch".into());
                    };
                    self.progression
                        .service
                        .stage(identity, SkillSaveOwner::Plain(ticket))
                        .map_err(|_| "skill service already reserved")?;
                    pending.phase = Phase::Saving;
                }
                _ => {}
            }
        }
        let token = self.token()?;
        // Even a degraded durable skill lane must not starve unrelated UI/XP output.
        let durable = self.progression.service.poll(
            &input,
            &mut self.online_saves,
            &self.saves.handle,
            token,
        );
        if self.progression.completion.is_none() {
            self.progression.completion = self.progression.service.take_completion();
        }
        if let Some(completion) = &self.progression.completion {
            let SkillSaveOwner::Plain(ticket) = completion.owner else {
                return Err("unexpected skill device completion".into());
            };
            let key = self
                .progression
                .pending
                .iter()
                .find_map(|(key, p)| {
                    (p.context == ticket.context && matches!(p.phase, Phase::Saving))
                        .then_some(*key)
                })
                .ok_or("skill completion missing session owner")?;
            if completion.committed {
                self.progression
                    .pending
                    .get_mut(&key)
                    .expect("matched skill")
                    .phase = Phase::Trained(ticket);
            } else {
                self.progression.reject(key, "skill transaction rejected");
            }
            self.progression.completion = None;
        }
        self.project_progression()?;
        durable
    }
    fn project_progression(&mut self) -> Result<(), String> {
        let keys = select_pending(
            &self.progression.pending,
            self.progression.output_cursor,
            self.limits.work_per_poll,
            |p| matches!(p, Phase::Raised(_) | Phase::Trained(_)),
        );
        for key in keys {
            if self.network_output.len() >= self.limits.messages {
                break;
            }
            self.progression.output_cursor = Some(key);
            let pending = &self.progression.pending[&key];
            let rank = if let Phase::Raised(change) = pending.phase {
                if change.before.ranks != change.after.ranks && change.rank_effect.is_none() {
                    return Err("accepted rank effect has no authoritative base".into());
                }
                bace_replication::project_rank_effect(
                    pending.context.actor.0,
                    change,
                    bace_replication::BatchLimits {
                        max_messages: 8,
                        max_bytes: 65536,
                        max_message_bytes: self.limits.message_bytes,
                        max_string_bytes: 4096,
                    },
                )
                .map_err(|e| format!("rank effect projection: {e:?}"))?
            } else {
                None
            };
            if rank.as_ref().is_some_and(|r| !r.observers.is_empty())
                && !self.observer_room(1, 4096)
            {
                break;
            }
            let replica = self
                .players
                .replication(pending.context.actor)
                .ok_or("progression sequence owner missing")?;
            if replica.key != key {
                return Err("progression sequence owner mismatch".into());
            }
            let cursor = self
                .progression
                .cursors
                .get_mut(&key)
                .ok_or("progression cursor missing")?;
            let (queue, messages) = match pending.phase {
                Phase::Raised(change) => {
                    let packets = cursor
                        .project(&mut replica.properties, pending.context, change)
                        .map_err(|e| format!("progression projection: {e:?}"))?;
                    (packets.queue, packets.messages.into())
                }
                Phase::Trained(ticket) => {
                    let change = ticket.change;
                    let bace_gameplay_api::ProgressionTarget::Skill(skill) = change.after.target
                    else {
                        return Err("training target mismatch".into());
                    };
                    let notice = bace_replication::project_training_notice(
                        skill,
                        change.available_skill_credits,
                        true,
                    )
                    .map_err(|e| format!("training notice: {e:?}"))?;
                    let mut packets = cursor
                        .project_training(
                            &mut replica.properties,
                            pending.context,
                            bace_gameplay_api::SkillTrainingChange {
                                before: change.before,
                                after: change.after,
                                available_skill_credits: change.available_skill_credits,
                                revision: change.revision,
                            },
                            None,
                        )
                        .map_err(|e| format!("training projection: {e:?}"))?;
                    packets.messages.push(notice);
                    (packets.queue, packets.messages)
                }
                _ => unreachable!("selected ready output"),
            };
            if let Some(rank) = rank {
                let mut ordered = messages.into_iter().map(|m| (queue, m)).collect::<Vec<_>>();
                ordered.extend(rank.owner.into_iter().map(|m| (m.queue, m.bytes)));
                self.network_output
                    .push_back(NetworkCommand::SendOrderedBatch {
                        key,
                        messages: ordered,
                    });
                if !rank.observers.is_empty() {
                    self.retain_observer_messages(vec![(pending.context.actor, rank.observers)])?;
                }
            } else {
                self.network_output.push_back(NetworkCommand::SendBatch {
                    key,
                    queue,
                    messages,
                });
            }
            self.progression.pending.remove(&key);
        }
        Ok(())
    }
}
/// Outer None is unsupported; inner None is a recognized training action waiting
/// for the bounded durable lane. Envelope sequence never overrides the reliable
/// message sequence shared with other live action adapters.
fn decode_action(
    context: ActionContext,
    bytes: &[u8],
    max_bytes: usize,
    training_busy: bool,
) -> Result<Option<Option<Action>>, String> {
    use bace_wire::opcode::GameActionType as Op;
    let opcode = bytes.get(..4).ok_or("truncated gameplay opcode")?;
    if u32::from_le_bytes(opcode.try_into().expect("four bytes"))
        != bace_wire::opcode::GameMessageOpcode::GameAction.0
    {
        return Ok(None);
    }
    let envelope = bace_wire::GameActionEnvelope::decode(bytes, max_bytes)
        .map_err(|e| format!("malformed game action: {e:?}"))?;
    let action = match envelope.action {
        Op::RaiseAttribute | Op::RaiseVital | Op::RaiseSkill => Action::Raise(
            bace_session::decode_progression(
                bace_session::SessionState::WorldConnected,
                context,
                bytes,
                max_bytes,
            )
            .map_err(|e| format!("malformed progression: {e:?}"))?
            .request,
        ),
        Op::TrainSkill => {
            let (_, request) = bace_session::decode_training(bytes, context, max_bytes)
                .map_err(|e| format!("malformed training: {e:?}"))?;
            if training_busy {
                return Ok(Some(None));
            }
            let mut identity = [0; 16];
            OsRng
                .try_fill_bytes(&mut identity)
                .map_err(|e| format!("skill operation entropy unavailable: {e}"))?;
            Action::Train(
                request,
                SkillOperationId::new(identity).map_err(|e| e.to_string())?,
            )
        }
        Op::SetCharacterOptions
        | Op::SetSingleCharacterOption
        | Op::AddShortCut
        | Op::RemoveShortCut
        | Op::AddSpellFavorite
        | Op::RemoveSpellFavorite
        | Op::SpellbookFilter
        | Op::SetDesiredComponentLevel => Action::Ui(
            bace_session::decode_ui(bytes, context, max_bytes)
                .map_err(|e| format!("malformed UI input: {e:?}"))?
                .1,
        ),
        _ => return Ok(None),
    };
    Ok(Some(Some(action)))
}

#[cfg(test)]
mod tests;

/// A hot low-key session must not monopolize a bounded adapter turn. Waiting
/// captures/receipts are excluded and the next turn resumes after the last key.
fn select_pending(
    pending: &BTreeMap<SessionKey, Pending>,
    after: Option<SessionKey>,
    budget: usize,
    eligible: impl Fn(&Phase) -> bool,
) -> Vec<SessionKey> {
    use std::ops::Bound::{Excluded, Included, Unbounded};
    let tail = pending.range((after.map_or(Unbounded, Excluded), Unbounded));
    let head = pending
        .range((Unbounded, after.map_or(Unbounded, Included)))
        .take(if after.is_some() { pending.len() } else { 0 });
    tail.chain(head)
        .filter(|(_, p)| eligible(&p.phase))
        .take(budget)
        .map(|(key, _)| *key)
        .collect()
}
