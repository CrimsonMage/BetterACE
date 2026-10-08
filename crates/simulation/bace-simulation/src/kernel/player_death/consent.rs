//! Source recipient-owned, one-hour corpse consent. Wall time enters only as
//! an explicit adapter input; logout clears the recipient's transient grants.
use super::*;
use crate::player_death::ConsentGrant;
use crate::{CorpseConsentCommand, CorpseConsentError, CorpseConsentOutcome};
use bace_gameplay_api::corpse_consent::CorpseConsentRequest as R;

const PERMIT_SECONDS: u64 = 3600;
const MAX_RECIPIENTS: usize = 4096;
// Player_Death.cs documents a 20-person consent list; this also fits one
// bounded source SystemChat even at the maximum authored name length.
const MAX_GRANTS: usize = 20;

impl Kernel {
    pub(crate) fn apply_corpse_consent_command(&mut self, command: CorpseConsentCommand) {
        let result = self.corpse_consent(&command);
        self.player_deaths
            .consent_grants
            .retain(|_, grants| !grants.is_empty());
        self.player_deaths
            .consent_outcomes
            .push_back(CorpseConsentOutcome {
                correlation: command.correlation,
                actor: command.context.actor,
                result,
            });
    }

    pub fn take_corpse_consent_outcome(&mut self) -> Option<CorpseConsentOutcome> {
        self.player_deaths.consent_outcomes.pop_front()
    }

    pub fn restore_corpse_consent_outcome(
        &mut self,
        outcome: CorpseConsentOutcome,
    ) -> Result<(), CorpseConsentOutcome> {
        if self.player_deaths.consent_outcomes.len() >= self.player_deaths.capacity {
            return Err(outcome);
        }
        self.player_deaths.consent_outcomes.push_front(outcome);
        Ok(())
    }

    fn corpse_consent(
        &mut self,
        command: &CorpseConsentCommand,
    ) -> Result<Vec<(EntityId, String)>, CorpseConsentError> {
        if !command.valid_bounds() {
            return Err(CorpseConsentError::Invalid);
        }
        let actor = command.context.actor;
        self.characters
            .can_take_complete(CharacterBinding {
                actor,
                account: command.context.account,
                session: command.context.session,
            })
            .map_err(|_| CorpseConsentError::Ownership)?;
        self.characters
            .authorize(command.context, self.world.body(actor).is_ok())
            .map_err(|_| CorpseConsentError::Ownership)?;
        let actor_name = self
            .social
            .directory
            .presence(actor)
            .filter(|presence| presence.online)
            .map(|presence| presence.identity.name.clone())
            .ok_or(CorpseConsentError::Ownership)?;
        let target = match &command.request {
            R::Clear | R::Display => None,
            R::RemoveFrom(name) | R::Add(name) | R::Remove(name) => self
                .social
                .directory
                .by_name(name)
                .filter(|presence| matches!(&command.request, R::RemoveFrom(_)) || presence.online)
                .map(|presence| {
                    (
                        presence.identity.character,
                        presence.identity.name.clone(),
                        self.character_ui(presence.identity.character)
                            .is_some_and(|ui| ui.options1 & 0x0008_0000 != 0),
                    )
                }),
        };
        let message = |target: EntityId, text: String| {
            if text.len() > 4096 {
                Err(CorpseConsentError::Capacity)
            } else {
                Ok(vec![(target, text)])
            }
        };
        match &command.request {
            R::Clear => {
                let grants = self.player_deaths.consent_grants.entry(actor).or_default();
                grants.retain(|_, grant| grant.expires_at >= command.unix_seconds);
                if grants.is_empty() {
                    message(
                        actor,
                        "You do not have permission to loot anyone's corpse.".into(),
                    )
                } else {
                    grants.clear();
                    message(actor, "You have cleared your consent list. Players will have to permit you again to allow you access to their corpse.".into())
                }
            }
            R::Display => {
                let grants = self.player_deaths.consent_grants.entry(actor).or_default();
                grants.retain(|_, grant| grant.expires_at >= command.unix_seconds);
                if grants.is_empty() {
                    message(
                        actor,
                        "You do not have permission to loot anyone's corpse.".into(),
                    )
                } else {
                    let names = grants
                        .values()
                        .map(|grant| grant.granter_name.as_str())
                        .collect::<Vec<_>>();
                    message(
                        actor,
                        format!(
                            "You have permissions to loot a corpse from these players:\n{}",
                            names.join("\n")
                        ),
                    )
                }
            }
            R::RemoveFrom(raw) => {
                let known = self
                    .player_deaths
                    .consent_grants
                    .get(&actor)
                    .and_then(|grants| {
                        grants
                            .iter()
                            .find(|(_, grant)| {
                                grant.expires_at >= command.unix_seconds
                                    && grant.granter_name.eq_ignore_ascii_case(raw)
                            })
                            .map(|(id, grant)| (*id, grant.granter_name.clone(), false))
                    });
                let Some((granter, name, _)) = target.or(known) else {
                    return message(actor, format!("{raw} is not online."));
                };
                if granter == actor {
                    return message(
                        actor,
                        "You always have permission to loot your corpse.".into(),
                    );
                }
                let grants = self.player_deaths.consent_grants.entry(actor).or_default();
                grants.retain(|_, grant| grant.expires_at >= command.unix_seconds);
                if grants.remove(&granter).is_some() {
                    message(
                        actor,
                        format!("You have removed your permissions to loot {name}'s corpse."),
                    )
                } else {
                    message(
                        actor,
                        format!("You don't have permission to loot {name}'s corpse."),
                    )
                }
            }
            R::Add(raw) => {
                let Some((recipient, name, accepts)) = target else {
                    return message(actor, format!("{raw} is not online."));
                };
                if recipient == actor {
                    return message(
                        actor,
                        "You already have permission to loot your corpse.".into(),
                    );
                }
                if !accepts {
                    return message(
                        actor,
                        format!(
                            "{name} is not accepting corpse looting permissions from other players."
                        ),
                    );
                }
                if !self.player_deaths.consent_grants.contains_key(&recipient)
                    && self.player_deaths.consent_grants.len() >= MAX_RECIPIENTS
                {
                    return Err(CorpseConsentError::Capacity);
                }
                let grants = self
                    .player_deaths
                    .consent_grants
                    .entry(recipient)
                    .or_default();
                grants.retain(|_, grant| grant.expires_at >= command.unix_seconds);
                if grants.contains_key(&actor) {
                    return message(
                        actor,
                        format!("{name} already has permission to loot your corpse."),
                    );
                }
                if grants.len() >= MAX_GRANTS {
                    return Err(CorpseConsentError::Capacity);
                }
                let expires = command
                    .unix_seconds
                    .checked_add(PERMIT_SECONDS)
                    .ok_or(CorpseConsentError::Invalid)?;
                grants.insert(
                    actor,
                    ConsentGrant {
                        expires_at: expires,
                        granter_name: actor_name.clone(),
                    },
                );
                Ok(vec![
                    (
                        recipient,
                        format!(
                            "{actor_name} has given you permission to loot one of his or her corpses. This permission will last one hour."
                        ),
                    ),
                    (
                        actor,
                        format!(
                            "You have given permission to {name} to loot one of your corpses. This permission will last one hour."
                        ),
                    ),
                ])
            }
            R::Remove(raw) => {
                let Some((recipient, name, _)) = target else {
                    // ACE dereferences a null player in this error branch; the
                    // correction preserves its intended denial without a crash.
                    return message(
                        actor,
                        format!("{raw} doesn't have permission to loot your corpse."),
                    );
                };
                if recipient == actor {
                    return message(
                        actor,
                        "You always have permission to loot your corpse.".into(),
                    );
                }
                let grants = self
                    .player_deaths
                    .consent_grants
                    .entry(recipient)
                    .or_default();
                grants.retain(|_, grant| grant.expires_at >= command.unix_seconds);
                if grants.remove(&actor).is_none() {
                    return message(
                        actor,
                        format!("{name} doesn't have permission to loot your corpse."),
                    );
                }
                Ok(vec![
                    (
                        recipient,
                        format!(
                            "{actor_name} has revoked permission to loot one of his or her corpses."
                        ),
                    ),
                    (
                        actor,
                        format!("{name}'s permission to loot your corpse has been revoked."),
                    ),
                ])
            }
        }
    }
}

#[cfg(test)]
mod tests;
