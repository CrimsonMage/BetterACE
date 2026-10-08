//! Authenticated ACE corpse consent actions. Mutation belongs to the simulation
//! recipient grant table; this adapter retains each correlated chat transcript.
use super::*;
use bace_gameplay_api::{ActionContext, corpse_consent::CorpseConsentRequest};
use bace_replication::{BatchLimits, InventoryProjection as P};
use bace_simulation::{CorpseConsentCommand, CorpseConsentOutcome};
use std::sync::mpsc::TrySendError;

pub(super) struct Pending {
    pub(super) key: SessionKey,
    actor: EntityId,
    correlation: u64,
    outcome: Option<CorpseConsentOutcome>,
}

impl GameRuntime {
    pub(in crate::game_runtime) fn handle_corpse_consent_dispatch(
        &mut self,
        key: SessionKey,
        context: ActionContext,
        request: CorpseConsentRequest,
    ) -> Result<super::super::social::SocialIngress, String> {
        use super::super::social::SocialIngress;
        // Corpse Open freezes and commits a one-shot grant before adoption.
        // Serialize consent mutations with that in-flight durable decision.
        if self.deaths.consent.is_some() || self.deaths.access.is_some() {
            return Ok(SocialIngress::Blocked);
        }
        let Some(session) = self.sessions.get(&key) else {
            return Ok(SocialIngress::Blocked);
        };
        let Some(loading) = session.loading.as_ref() else {
            return Ok(SocialIngress::Blocked);
        };
        let binding = loading.loaded.binding;
        if !self.players.entered(binding.actor)
            || session.disconnected
            || session.terminated
            || binding.actor != context.actor
            || binding.account != context.account
            || binding.session != context.session
            || binding.account != session.account.id
            || binding.session.0 != key.generation
        {
            return Ok(SocialIngress::Blocked);
        }
        let elapsed = u64::try_from(self.last_elapsed.as_millis())
            .map_err(|_| "corpse consent clock overflow")?;
        let unix_seconds = self
            .clock
            .unix_millis
            .checked_add(elapsed)
            .ok_or("corpse consent clock overflow")?
            / 1000;
        let correlation = self.token()?;
        let command = Command::CorpseConsent(CorpseConsentCommand {
            correlation,
            context,
            request,
            unix_seconds,
        });
        match self.simulation.input().try_submit(command) {
            Ok(()) => {
                self.deaths.consent = Some(Pending {
                    key,
                    actor: context.actor,
                    correlation,
                    outcome: None,
                });
                Ok(SocialIngress::Accepted)
            }
            Err(TrySendError::Full(_)) => Ok(SocialIngress::Blocked),
            Err(TrySendError::Disconnected(_)) => Err("corpse consent owner closed".into()),
        }
    }

    pub(super) fn poll_corpse_consent(&mut self) -> Result<(), String> {
        if self.deaths.unexpected_consent.is_some() {
            return Err("unmatched corpse consent outcome retained".into());
        }
        let Some(mut pending) = self.deaths.consent.take() else {
            return if let Ok(outcome) = self.simulation.corpse_consent_outcomes().try_recv() {
                self.deaths.unexpected_consent = Some(outcome);
                Err("unmatched corpse consent outcome retained".into())
            } else {
                Ok(())
            };
        };
        if pending.outcome.is_none()
            && let Ok(outcome) = self.simulation.corpse_consent_outcomes().try_recv()
        {
            if outcome.correlation != pending.correlation || outcome.actor != pending.actor {
                self.deaths.unexpected_consent = Some(outcome);
                self.deaths.consent = Some(pending);
                return Err("corpse consent outcome identity mismatch".into());
            }
            pending.outcome = Some(outcome);
        }
        let result = self.present_corpse_consent(&mut pending);
        if result.is_err() || !result.as_ref().is_ok_and(|complete| *complete) {
            self.deaths.consent = Some(pending);
        }
        result.map(|_| ())
    }

    fn present_corpse_consent(&mut self, pending: &mut Pending) -> Result<bool, String> {
        let Some(outcome) = pending.outcome.as_ref() else {
            return Ok(false);
        };
        let Ok(messages) = outcome.result.as_ref() else {
            // Rejected authenticated attempts consume their owner sequence and
            // carry no source success chat. They must not pin every later
            // consent action behind a failed result.
            return Ok(true);
        };
        if messages.len() > 2 || messages.iter().any(|(_, text)| text.len() > 4096) {
            return Err("corpse consent output bound".into());
        }
        let connected = messages
            .iter()
            .filter_map(|(actor, text)| {
                let replica = self.players.replication(*actor)?;
                self.sessions
                    .get(&replica.key)
                    .filter(|session| !session.disconnected && !session.terminated)
                    .map(|_| (*actor, text.as_str()))
            })
            .collect::<Vec<_>>();
        if self
            .network_output
            .len()
            .checked_add(connected.len())
            .is_none_or(|count| count > self.limits.messages)
        {
            return Ok(false);
        }
        let mut batches = Vec::with_capacity(connected.len());
        for (actor, text) in connected {
            let replica = self
                .players
                .replication(actor)
                .ok_or("corpse consent recipient vanished")?;
            let binding = replica.binding;
            let key = replica.key;
            let limit = self.limits.message_bytes;
            let batch = replica
                .events
                .project_inventory(
                    binding,
                    &[P::System { text, chat_type: 0 }],
                    &mut replica.item_properties,
                    bace_wire::ObjectCodecLimits {
                        max_message_bytes: limit,
                        max_model_entries: 255,
                        max_children: 128,
                        max_restrictions: 1024,
                        max_motion_commands: 32,
                        max_string_bytes: 4096,
                    },
                    BatchLimits {
                        max_messages: 1,
                        max_bytes: limit,
                        max_message_bytes: limit,
                        max_string_bytes: 4096,
                    },
                )
                .map_err(|error| format!("corpse consent projection: {error:?}"))?;
            batches.push(
                crate::game_messages::session_batch_command(key, batch)
                    .map_err(|error| error.to_string())?,
            );
        }
        self.network_output.extend(batches);
        Ok(true)
    }
}
