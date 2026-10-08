//! Ordered group output. Encoding failure preserves the authoritative event counter.
use crate::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_gameplay_api::{CharacterBinding, social::SocialEvent};
use bace_wire::{GroupEvent, SocialCodecLimits};
impl EventSequencer {
    pub fn project_group_event(
        &mut self,
        binding: CharacterBinding,
        event: &SocialEvent,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        let wire_limits = SocialCodecLimits {
            max_message_bytes: limits.max_message_bytes,
            max_entries: 4096,
            max_filters: 4,
            max_string_bytes: limits.max_string_bytes,
        };
        let fellowship;
        let allegiance;
        let event = match event {
            SocialEvent::Fellowship {
                recipients,
                snapshot,
            } => {
                if !recipients.contains(&binding.actor) {
                    return Err(SessionProjectionError::WrongBinding);
                }
                fellowship = crate::group_profile::fellowship(snapshot)?;
                GroupEvent::Fellowship(&fellowship)
            }
            SocialEvent::FellowshipLeft {
                recipients,
                actor,
                dismissed,
                disbanded,
            } => {
                if !recipients.contains(&binding.actor) {
                    return Err(SessionProjectionError::WrongBinding);
                }
                if *disbanded {
                    GroupEvent::FellowDisband
                } else if *dismissed {
                    GroupEvent::FellowDismiss(actor.0)
                } else {
                    GroupEvent::FellowQuit(actor.0)
                }
            }
            SocialEvent::Allegiance {
                recipient,
                profile,
                info_response,
            } => {
                if *recipient != binding.actor {
                    return Err(SessionProjectionError::WrongBinding);
                }
                allegiance = crate::group_profile::allegiance(profile)?;
                if *info_response {
                    GroupEvent::AllegianceInfo {
                        subject: profile.subject.0,
                        profile: &allegiance,
                    }
                } else {
                    GroupEvent::Allegiance {
                        rank: profile.rank,
                        profile: &allegiance,
                    }
                }
            }
            SocialEvent::Confirmation {
                recipient,
                kind,
                token,
                text,
            } => {
                if *recipient != binding.actor {
                    return Err(SessionProjectionError::WrongBinding);
                }
                GroupEvent::Confirmation {
                    kind: *kind,
                    token: *token,
                    text,
                }
            }
            _ => return Err(SessionProjectionError::InvalidProjection),
        };
        let bytes = event.encode(binding.actor.0, self.next, wire_limits)?;
        let mut messages = Vec::new();
        let mut total = 0;
        crate::session_output::push(&mut messages, &mut total, 9, bytes, limits)?;
        self.next = self.next.wrapping_add(1);
        Ok(SessionBatch { binding, messages })
    }
}
