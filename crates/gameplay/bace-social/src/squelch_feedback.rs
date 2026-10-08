//! User-visible SquelchManager responses from the pinned official source.
use crate::{SocialDirectory, squelch_mask};
use bace_gameplay_api::social::{SocialError, SocialRequest};
use bace_types::EntityId;
pub fn channel_name(channel: u32) -> String {
    match channel {
        0 => "Broadcast",
        1 => "AllChannels",
        2 => "Speech",
        3 => "Tell",
        4 => "OutgoingTell",
        5 => "System",
        6 => "Combat",
        7 => "Magic",
        8 => "Channel",
        9 => "ChannelSend",
        10 => "Social",
        11 => "SocialSend",
        12 => "Emote",
        13 => "Advancement",
        14 => "Abuse",
        15 => "Help",
        16 => "Appraisal",
        17 => "Spellcasting",
        18 => "Allegiance",
        19 => "Fellowship",
        20 => "WorldBroadcast",
        21 => "CombatEnemy",
        22 => "CombatSelf",
        23 => "Recall",
        24 => "Craft",
        25 => "Salvaging",
        27 => "x1B",
        28 => "x1C",
        29 => "x1D",
        30 => "x1E",
        31 => "AdminTell",
        _ => return channel.to_string(),
    }
    .into()
}
impl SocialDirectory {
    pub fn squelch_feedback(
        &self,
        actor: EntityId,
        request: &SocialRequest,
        result: Result<(), SocialError>,
    ) -> Option<String> {
        match request {
            SocialRequest::GlobalSquelch {
                enabled,
                message_type,
            } => {
                let verb = if *enabled { "squelch" } else { "unsquelch" };
                let unchanged = result.is_err()
                    || self.propose_preferences(actor, request).ok().as_ref()
                        == self.preferences(actor);
                Some(format!(
                    "The {} channel {} {verb}ed.",
                    channel_name(*message_type),
                    if unchanged { "is already" } else { "has been" }
                ))
            }
            SocialRequest::AccountSquelch { enabled, name } => {
                if name.trim().is_empty() {
                    return None;
                }
                let Some(target) = self.by_name(name) else {
                    return Some(format!("{name} not found."));
                };
                if self.presence(actor)?.identity.account == target.identity.account {
                    return Some("You can't squelch yourself!".into());
                }
                let state = match (*enabled, result) {
                    (true, Ok(())) => "has been squelched",
                    (true, Err(_)) => "is already squelched",
                    (false, Ok(())) => "has been unsquelched",
                    (false, Err(_)) => "is not squelched",
                };
                Some(format!("{}'s account {state}.", target.identity.name))
            }
            SocialRequest::CharacterSquelch {
                enabled,
                target,
                name,
                message_type,
            } => {
                if squelch_mask(*message_type).is_err() {
                    return Some(format!(
                        "{} is not a legal squelch channel",
                        channel_name(*message_type)
                    ));
                }
                let target = if target.0 != 0 {
                    match self.presence(*target) {
                        Some(v) => v,
                        None => return Some("Couldn't find player to squelch.".into()),
                    }
                } else {
                    if name.trim().is_empty() {
                        return None;
                    }
                    match self.by_name(name) {
                        Some(v) => v,
                        None => return Some(format!("{name} not found.")),
                    }
                };
                if target.identity.character == actor {
                    return Some("You can't squelch yourself!".into());
                }
                let suffix = if *message_type == 1 {
                    String::new()
                } else {
                    format!(" on the {} channel", channel_name(*message_type))
                };
                let state = match (*enabled, result) {
                    (true, Ok(())) => "has been squelched",
                    (true, Err(_)) => "is already squelched",
                    (false, Ok(())) => "has been unsquelched",
                    (false, Err(_)) => "is not squelched",
                };
                let suffix = if !*enabled && result == Err(SocialError::Missing) {
                    String::new()
                } else {
                    suffix
                };
                Some(format!("{} {state}{suffix}.", target.identity.name))
            }
            _ => None,
        }
    }
}
