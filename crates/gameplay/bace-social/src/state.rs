//! Persistent social preferences and bounded online directory. Identity is trusted input.
use bace_gameplay_api::social::{SocialError as E, SocialIdentity, SocialSquelch};
use bace_types::{AccountId, EntityId};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialPreferences {
    pub friends: BTreeSet<EntityId>,
    pub squelches: Vec<SocialSquelch>,
    pub global_mask: u32,
    pub afk_message: String,
    pub channels: BTreeSet<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SocialPresence {
    pub identity: SocialIdentity,
    pub access: u8,
    pub online: bool,
    pub appear_offline: bool,
    pub afk: bool,
    pub gagged: bool,
    pub olthoi: bool,
    pub no_olthoi_talk: bool,
    pub ignore_fellowship_requests: bool,
    pub auto_accept_fellowship: bool,
    pub share_fellowship_loot: bool,
    pub society: u32,
    pub listen_allegiance: bool,
    pub listen_general: bool,
    pub listen_trade: bool,
    pub listen_lfg: bool,
    pub listen_roleplay: bool,
    pub listen_society: bool,
}
#[derive(Clone, Debug)]
pub struct SocialDirectory {
    pub(crate) identities: BTreeMap<EntityId, SocialPresence>,
    pub(crate) names: BTreeMap<String, EntityId>,
    pub(crate) settings: BTreeMap<EntityId, SocialPreferences>,
    pub(crate) inverse_friends: BTreeMap<EntityId, BTreeSet<EntityId>>,
    pub(crate) online: BTreeSet<EntityId>,
    pub(crate) capacity: usize,
}
impl SocialDirectory {
    pub fn new(capacity: usize) -> Result<Self, E> {
        if capacity == 0 || capacity > 1_000_000 {
            return Err(E::Capacity);
        }
        Ok(Self {
            identities: BTreeMap::new(),
            names: BTreeMap::new(),
            settings: BTreeMap::new(),
            inverse_friends: BTreeMap::new(),
            online: BTreeSet::new(),
            capacity,
        })
    }
    pub fn presence(&self, actor: EntityId) -> Option<&SocialPresence> {
        self.identities.get(&actor)
    }
    pub fn by_name(&self, name: &str) -> Option<&SocialPresence> {
        self.names
            .get(&name.trim_start_matches('+').to_lowercase())
            .and_then(|id| self.presence(*id))
    }
    pub fn preferences(&self, actor: EntityId) -> Option<&SocialPreferences> {
        self.settings.get(&actor)
    }
    pub fn online(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.online.iter().copied()
    }
    pub fn watchers(&self, actor: EntityId) -> impl Iterator<Item = EntityId> + '_ {
        self.inverse_friends
            .get(&actor)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| self.online.contains(id))
    }
    pub fn validate_registration(
        &self,
        presence: &SocialPresence,
        settings: &SocialPreferences,
    ) -> Result<(), E> {
        let actor = presence.identity.character;
        let name = presence.identity.name.to_lowercase();
        if actor.0 == 0
            || presence.identity.account.0 == 0
            || name.is_empty()
            || name.len() > 100
            || presence.access > 5
        {
            return Err(E::Invalid);
        }
        settings.validate()?;
        if settings.friends.contains(&actor) {
            return Err(E::Invalid);
        }
        if self.settings.contains_key(&actor)
            || self
                .identities
                .get(&actor)
                .is_some_and(|old| old.identity != presence.identity)
            || self.names.get(&name).is_some_and(|id| *id != actor)
        {
            return Err(E::Duplicate);
        }
        if !self.identities.contains_key(&actor) && self.identities.len() == self.capacity {
            return Err(E::Capacity);
        }
        Ok(())
    }
    pub fn register(
        &mut self,
        presence: SocialPresence,
        settings: SocialPreferences,
    ) -> Result<(), E> {
        self.validate_registration(&presence, &settings)?;
        let actor = presence.identity.character;
        let name = presence.identity.name.to_lowercase();
        for friend in &settings.friends {
            self.inverse_friends
                .entry(*friend)
                .or_default()
                .insert(actor);
        }
        if presence.online {
            self.online.insert(actor);
        }
        self.names.insert(name, actor);
        self.identities.insert(actor, presence);
        self.settings.insert(actor, settings);
        Ok(())
    }
    pub fn update_presence(&mut self, presence: SocialPresence) -> Result<(), E> {
        let actor = presence.identity.character;
        let old = self.presence(actor).ok_or(E::Missing)?;
        if old.identity != presence.identity {
            return Err(E::Stale);
        }
        if presence.online {
            self.online.insert(actor);
        } else {
            self.online.remove(&actor);
        }
        self.identities.insert(actor, presence);
        Ok(())
    }
    pub fn adopt_preferences(
        &mut self,
        actor: EntityId,
        before: &SocialPreferences,
        after: SocialPreferences,
    ) -> Result<(), E> {
        after.validate()?;
        if self.preferences(actor) != Some(before) {
            return Err(E::Stale);
        }
        for friend in before.friends.difference(&after.friends) {
            if let Some(watchers) = self.inverse_friends.get_mut(friend) {
                watchers.remove(&actor);
                if watchers.is_empty() {
                    self.inverse_friends.remove(friend);
                }
            }
        }
        for friend in after.friends.difference(&before.friends) {
            self.inverse_friends
                .entry(*friend)
                .or_default()
                .insert(actor);
        }
        self.settings.insert(actor, after);
        Ok(())
    }
    pub fn unregister(&mut self, actor: EntityId) -> Result<SocialPreferences, E> {
        if !self.settings.contains_key(&actor) {
            return Err(E::Missing);
        }
        let presence = self.identities.get_mut(&actor).ok_or(E::Missing)?;
        presence.online = false;
        presence.afk = false;
        self.online.remove(&actor);
        let preferences = self.settings.remove(&actor).ok_or(E::Missing)?;
        for friend in &preferences.friends {
            if let Some(watchers) = self.inverse_friends.get_mut(friend) {
                watchers.remove(&actor);
                if watchers.is_empty() {
                    self.inverse_friends.remove(friend);
                }
            }
        }
        Ok(preferences)
    }
    pub fn squelched(&self, recipient: EntityId, sender: EntityId, message_type: u32) -> bool {
        let Ok(mask) = crate::squelch_mask(message_type) else {
            return false;
        };
        let Some(source) = self.presence(sender) else {
            return true;
        };
        let Some(settings) = self.preferences(recipient) else {
            return true;
        };
        settings.global_mask & mask == mask
            || settings.squelches.iter().any(|entry| {
                if let Some(account) = entry.account {
                    account == source.identity.account
                } else {
                    entry.character == sender && entry.mask & mask == mask
                }
            })
    }
}
impl SocialPreferences {
    pub fn validate(&self) -> Result<(), E> {
        if self.friends.len() > 1024
            || self.squelches.len() > 1024
            || self.channels.len() > 32
            || self.afk_message.len() > 4096
            || self.friends.iter().any(|id| id.0 == 0)
            || self.squelches.iter().any(|s| {
                s.character.0 == 0 || s.name.len() > 100 || s.account == Some(AccountId(0))
            })
        {
            return Err(E::Capacity);
        }
        Ok(())
    }
}
impl SocialDirectory {
    /// Trusted relational identity only. Offline cache rows carry no live privileges.
    pub fn cache_identity(&mut self, identity: SocialIdentity) -> Result<(), E> {
        self.cache_offline_identity(SocialPresence {
            identity,
            access: 0,
            online: false,
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
        })
    }
    /// Cold identity lookup for friends/squelches; no second preference owner.
    pub fn cache_offline_identity(&mut self, presence: SocialPresence) -> Result<(), E> {
        if presence.online
            || presence.identity.character.0 == 0
            || presence.identity.account.0 == 0
            || presence.identity.name.is_empty()
            || presence.identity.name.len() > 100
        {
            return Err(E::Invalid);
        }
        let actor = presence.identity.character;
        let name = presence.identity.name.to_lowercase();
        if self
            .identities
            .get(&actor)
            .is_some_and(|p| p.identity != presence.identity)
            || self.names.get(&name).is_some_and(|id| *id != actor)
        {
            return Err(E::Stale);
        }
        if self.identities.contains_key(&actor) {
            return Ok(());
        }
        if self.identities.len() >= self.capacity {
            return Err(E::Capacity);
        }
        self.names.insert(name, actor);
        self.identities.insert(actor, presence);
        Ok(())
    }
}
