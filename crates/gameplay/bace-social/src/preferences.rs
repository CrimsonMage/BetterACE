//! ACE SquelchManager masks; changes are proposals until their aggregate owner adopts.
use crate::{SocialDirectory, SocialPreferences};
use bace_gameplay_api::social::{SocialError as E, SocialRequest, SocialSquelch};
use bace_types::EntityId;
const COMBINED: u32 = (1 << 2)
    | (1 << 3)
    | (1 << 6)
    | (1 << 7)
    | (1 << 12)
    | (1 << 16)
    | (1 << 17)
    | (1 << 18)
    | (1 << 19)
    | (1 << 21)
    | (1 << 22)
    | (1 << 23)
    | (1 << 24)
    | (1 << 25);
pub fn squelch_mask(message_type: u32) -> Result<u32, E> {
    match message_type {
        1 => Ok(u32::MAX),
        2 | 3 | 6 | 7 | 12 | 16 | 17 | 18 | 19 | 21 | 22 | 23 | 24 | 25 => Ok(1 << message_type),
        _ => Err(E::Invalid),
    }
}
fn add(mask: u32, value: u32) -> u32 {
    let result = mask | value;
    if result == COMBINED { u32::MAX } else { result }
}
fn remove(mask: u32, value: u32) -> u32 {
    if value == u32::MAX {
        return 0;
    }
    let result = if mask == u32::MAX { COMBINED } else { mask } & !value;
    if result == COMBINED { u32::MAX } else { result }
}
impl SocialDirectory {
    pub fn propose_preferences(
        &self,
        actor: EntityId,
        request: &SocialRequest,
    ) -> Result<SocialPreferences, E> {
        let source = self.presence(actor).ok_or(E::Missing)?;
        let mut result = self.preferences(actor).ok_or(E::Missing)?.clone();
        match request {
            SocialRequest::AddFriend(name) => {
                let target = self.by_name(name).ok_or(E::Missing)?.identity.character;
                if target == actor {
                    return Err(E::Invalid);
                }
                if !result.friends.insert(target) {
                    return Err(E::Duplicate);
                }
            }
            SocialRequest::RemoveFriend(target) => {
                if !result.friends.remove(target) {
                    return Err(E::Missing);
                }
            }
            SocialRequest::RemoveAllFriends => result.friends.clear(),
            SocialRequest::SetAfkMessage(message) => result.afk_message = message.clone(),
            SocialRequest::AddChannel(channel) => {
                if !legal_legacy_channel(source.access, *channel) {
                    return Err(E::Forbidden);
                }
                result.channels.insert(*channel);
            }
            SocialRequest::RemoveChannel(channel) => {
                result.channels.remove(channel);
            }
            SocialRequest::GlobalSquelch {
                enabled,
                message_type,
            } => {
                let mask = squelch_mask(*message_type)?;
                result.global_mask = if *enabled {
                    add(result.global_mask, mask)
                } else {
                    remove(result.global_mask, mask)
                };
            }
            SocialRequest::CharacterSquelch {
                enabled,
                target,
                name,
                message_type,
            } => {
                let target = if target.0 != 0 {
                    self.presence(*target)
                } else {
                    self.by_name(name)
                }
                .ok_or(E::Missing)?;
                if target.identity.character == actor {
                    return Err(E::Invalid);
                }
                let mask = squelch_mask(*message_type)?;
                update(
                    &mut result,
                    SocialSquelch {
                        character: target.identity.character,
                        account: None,
                        name: target.identity.name.clone(),
                        mask,
                    },
                    *enabled,
                )?;
            }
            SocialRequest::AccountSquelch { enabled, name } => {
                let target = self.by_name(name).ok_or(E::Missing)?;
                if target.identity.account == source.identity.account {
                    return Err(E::Invalid);
                }
                update(
                    &mut result,
                    SocialSquelch {
                        character: target.identity.character,
                        account: Some(target.identity.account),
                        name: target.identity.name.clone(),
                        mask: u32::MAX,
                    },
                    *enabled,
                )?;
            }
            _ => return Err(E::Invalid),
        }
        result.validate()?;
        Ok(result)
    }
}
fn update(state: &mut SocialPreferences, entry: SocialSquelch, enabled: bool) -> Result<(), E> {
    let index = state.squelches.iter().position(|s| {
        if let Some(account) = entry.account {
            s.account == Some(account)
        } else {
            s.account.is_none() && s.character == entry.character
        }
    });
    match (index, enabled) {
        (None, true) => state.squelches.push(entry),
        (None, false) => return Err(E::Missing),
        (Some(i), _) => {
            let old = state.squelches[i].mask;
            let mask = if enabled {
                add(old, entry.mask)
            } else {
                remove(old, entry.mask)
            };
            if old == mask {
                return Err(E::Duplicate);
            }
            if mask == 0 {
                state.squelches.remove(i);
            } else {
                state.squelches[i].mask = mask;
            }
        }
    }
    Ok(())
}
pub fn legal_legacy_channel(access: u8, channel: u32) -> bool {
    match channel {
        1 | 8 | 16 | 32 => access >= 1,
        2 => access >= 5,
        4 | 512 => access >= 2,
        1024 => true,
        _ => false,
    }
}
