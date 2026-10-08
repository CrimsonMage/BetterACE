//! Source staff Audit publication through the single social owner.
use super::*;
use bace_gameplay_api::{
    social::{AcceptedChat, ChatChannel, ChatDelivery, SocialEvent},
    staff::StaffError,
};

impl Kernel {
    pub(super) fn staff_audit(
        &mut self,
        context: ActionContext,
        texts: Vec<String>,
        sudo: bool,
    ) -> Result<(), StaffError> {
        if texts.is_empty()
            || texts.len() > 2
            || texts
                .iter()
                .any(|text| text.is_empty() || text.len() > 4096 || text.as_bytes().contains(&0))
        {
            return Err(StaffError::Invalid);
        }
        if self.social.events.len().saturating_add(texts.len()) > self.social.capacity {
            return Err(StaffError::Capacity);
        }
        self.social
            .sequence
            .checked_add(texts.len() as u64)
            .ok_or(StaffError::Overflow)?;
        let unix_seconds = self.social_now().map_err(|_| StaffError::Overflow)?;
        self.authorize_staff(context, 2, sudo)?;
        let sender_name = self
            .social
            .directory
            .presence(context.actor)
            .filter(|presence| presence.online && presence.identity.account == context.account)
            .ok_or(StaffError::NotBound)?
            .identity
            .name
            .clone();
        let recipients: Vec<_> = self
            .social
            .directory
            .online()
            .filter(|id| {
                self.social
                    .directory
                    .preferences(*id)
                    .is_some_and(|preferences| preferences.channels.contains(&4))
            })
            .take(4097)
            .collect();
        if recipients.len() > 4096 {
            return Err(StaffError::Capacity);
        }
        for text in texts {
            self.social.sequence += 1;
            self.social.events.push_back(SocialEvent::Chat {
                accepted: AcceptedChat {
                    sequence: self.social.sequence,
                    unix_seconds,
                    sender: context.actor,
                    sender_name: sender_name.clone(),
                    channel: ChatChannel::Audit,
                    text,
                },
                wire: ChatDelivery::LegacyChannel(4),
                recipients: recipients.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_character::{CharacterProgression, ProgressionTables, RankTable};
    use bace_gameplay_api::{
        CharacterBinding, SessionId,
        staff::{StaffAction, StaffCommand, StaffEvent, StaffPrivileges, StaffRegistration},
    };
    use bace_social::{SocialPreferences, SocialPresence};
    use std::{collections::BTreeSet, sync::Arc};

    #[test]
    fn committed_audit_enters_two_source_lines_under_one_authorization() {
        let mut kernel = crate::synthetic_scenario(1, 0).unwrap();
        let binding = CharacterBinding {
            actor: bace_types::EntityId(1),
            account: bace_types::AccountId(1),
            session: SessionId(7),
        };
        let rank = RankTable::new(&[0, 10]).unwrap();
        kernel
            .register_character(
                binding,
                CharacterProgression::new(
                    &[],
                    Arc::new(ProgressionTables {
                        attributes: rank.clone(),
                        vitals: rank.clone(),
                        trained_skills: rank.clone(),
                        specialized_skills: rank,
                    }),
                    0,
                    0,
                )
                .unwrap(),
            )
            .unwrap();
        kernel
            .register_staff(StaffRegistration {
                binding,
                privileges: StaffPrivileges {
                    account_access: 2,
                    sentinel: true,
                    ..Default::default()
                },
            })
            .unwrap();
        kernel
            .register_social_presence(
                SocialPresence {
                    identity: bace_gameplay_api::social::SocialIdentity {
                        character: binding.actor,
                        account: binding.account,
                        name: "Sentinel".into(),
                    },
                    access: 2,
                    online: true,
                    appear_offline: false,
                    afk: false,
                    gagged: false,
                    olthoi: false,
                    no_olthoi_talk: false,
                    ignore_fellowship_requests: false,
                    auto_accept_fellowship: false,
                    share_fellowship_loot: false,
                    society: 0,
                    listen_allegiance: false,
                    listen_general: false,
                    listen_trade: false,
                    listen_lfg: false,
                    listen_roleplay: false,
                    listen_society: false,
                },
                SocialPreferences {
                    channels: BTreeSet::from([4]),
                    ..Default::default()
                },
            )
            .unwrap();
        kernel.social.events.clear();
        let context = ActionContext {
            actor: binding.actor,
            account: binding.account,
            session: binding.session,
            sequence: 1,
        };
        kernel
            .apply_staff_command(StaffCommand {
                token: 42,
                action: StaffAction::Audit {
                    context,
                    texts: vec![
                        "Booting account Target.".into(),
                        "Banned account Target for 0 days, 0 hours and 5 minutes.".into(),
                    ],
                    sudo: false,
                },
            })
            .unwrap();
        let Some(SocialEvent::Chat {
            accepted,
            wire,
            recipients,
        }) = kernel.take_social_event()
        else {
            panic!("Audit event")
        };
        assert_eq!(accepted.channel, ChatChannel::Audit);
        assert_eq!(accepted.sender, binding.actor);
        assert_eq!(accepted.sender_name, "Sentinel");
        assert_eq!(accepted.text, "Booting account Target.");
        assert_eq!(accepted.sequence, 1);
        assert_eq!(wire, ChatDelivery::LegacyChannel(4));
        assert_eq!(recipients, vec![binding.actor]);
        let Some(SocialEvent::Chat { accepted, .. }) = kernel.take_social_event() else {
            panic!("second Audit event")
        };
        assert_eq!(
            accepted.text,
            "Banned account Target for 0 days, 0 hours and 5 minutes."
        );
        assert_eq!(accepted.sequence, 2);
        assert!(
            matches!(kernel.take_staff_event(), Some(StaffEvent::Outcome { token: 42, actor: Some(actor), result: Ok(()) }) if actor == binding.actor)
        );
        assert!(kernel.take_social_event().is_none());
    }
}
