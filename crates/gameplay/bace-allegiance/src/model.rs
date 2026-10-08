//! Allegiance state is ID-indexed. Only explicit proposals may change accepted state.
use bace_gameplay_api::social::{AllegianceSanctuary, SocialError};
use bace_types::{AccountId, EntityId};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllegianceNode {
    pub character: EntityId,
    pub account: AccountId,
    pub name: String,
    pub gender: u8,
    pub heritage: u8,
    pub patron: Option<EntityId>,
    pub monarch: EntityId,
    pub vassals: Vec<EntityId>,
    pub rank: u32,
    pub followers: u32,
    pub level: u32,
    pub leadership: u32,
    pub loyalty: u32,
    pub sworn_at: u64,
    pub online_seconds: u64,
    pub may_pass_up: bool,
    pub received_total: u64,
    pub tithed_total: u64,
    pub unclaimed: u64,
}
impl AllegianceNode {
    pub fn validate(&self) -> Result<(), SocialError> {
        if self.character.0 == 0
            || self.account.0 == 0
            || self.monarch.0 == 0
            || self.name.is_empty()
            || self.name.len() > 100
            || self.vassals.len() > 11
            || self.rank == 0
            || self.rank > 10
            || self.level == 0
            || self.level > 275
            || self.unclaimed > u64::from(u32::MAX)
            || self.patron == Some(self.character)
            || self
                .vassals
                .iter()
                .any(|v| *v == self.character || v.0 == 0)
            || self.vassals.iter().copied().collect::<BTreeSet<_>>().len() != self.vassals.len()
        {
            return Err(SocialError::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct AllegianceMetadata {
    pub monarch: EntityId,
    pub chat_room: u32,
    pub name: Option<String>,
    pub motd: Option<String>,
    pub motd_set_by: Option<String>,
    pub officer_titles: [Option<String>; 3],
    pub officers: BTreeMap<EntityId, u32>,
    pub locked: bool,
    pub approved: BTreeSet<EntityId>,
    pub banned_characters: BTreeMap<EntityId, String>,
    pub chat_gags: BTreeMap<EntityId, i64>,
    pub sanctuary: Option<AllegianceSanctuary>,
}
impl AllegianceMetadata {
    pub fn new(monarch: EntityId, chat_room: u32) -> Self {
        Self {
            monarch,
            chat_room,
            name: None,
            motd: None,
            motd_set_by: None,
            officer_titles: Default::default(),
            officers: BTreeMap::new(),
            locked: false,
            approved: BTreeSet::new(),
            banned_characters: BTreeMap::new(),
            chat_gags: BTreeMap::new(),
            sanctuary: None,
        }
    }
    pub fn permission(&self, actor: EntityId) -> u32 {
        if actor == self.monarch {
            4
        } else {
            self.officers.get(&actor).copied().unwrap_or(0)
        }
    }
    pub fn chat_allowed(&self, actor: EntityId, now: i64) -> bool {
        self.chat_gags.get(&actor).is_none_or(|until| *until <= now)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct AllegiancePatch {
    pub before_revision: u64,
    pub after_revision: u64,
    pub nodes: Vec<(Option<AllegianceNode>, Option<AllegianceNode>)>,
    pub metadata: Vec<(Option<AllegianceMetadata>, Option<AllegianceMetadata>)>,
}
#[derive(Clone, Debug)]
pub struct AllegianceRegistry {
    pub(crate) nodes: BTreeMap<EntityId, AllegianceNode>,
    pub(crate) groups: BTreeMap<EntityId, AllegianceMetadata>,
    pub(crate) revision: u64,
    pub(crate) capacity: usize,
}
impl AllegianceRegistry {
    pub fn new(capacity: usize) -> Result<Self, SocialError> {
        if capacity == 0 || capacity > 1_000_000 {
            return Err(SocialError::Capacity);
        }
        Ok(Self {
            nodes: BTreeMap::new(),
            groups: BTreeMap::new(),
            revision: 0,
            capacity,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn node(&self, id: EntityId) -> Option<&AllegianceNode> {
        self.nodes.get(&id)
    }
    pub fn metadata(&self, monarch: EntityId) -> Option<&AllegianceMetadata> {
        self.groups.get(&monarch)
    }
    pub fn nodes(&self) -> impl Iterator<Item = &AllegianceNode> {
        self.nodes.values()
    }
    pub fn groups(&self) -> impl Iterator<Item = &AllegianceMetadata> {
        self.groups.values()
    }
    pub fn permission(&self, actor: EntityId) -> u32 {
        self.node(actor)
            .and_then(|n| self.metadata(n.monarch))
            .map_or(0, |m| m.permission(actor))
    }
    pub fn patch(&self) -> Result<AllegiancePatch, SocialError> {
        Ok(AllegiancePatch {
            before_revision: self.revision,
            after_revision: self.revision.checked_add(1).ok_or(SocialError::Overflow)?,
            nodes: vec![],
            metadata: vec![],
        })
    }
    pub fn validate_patch(&self, patch: &AllegiancePatch) -> Result<(), SocialError> {
        if patch.before_revision != self.revision
            || patch.after_revision != self.revision.checked_add(1).ok_or(SocialError::Overflow)?
        {
            return Err(SocialError::Stale);
        }
        let mut ids = BTreeSet::new();
        let mut count = self.nodes.len();
        for (before, after) in &patch.nodes {
            let id = before
                .as_ref()
                .or(after.as_ref())
                .ok_or(SocialError::Invalid)?
                .character;
            if !ids.insert(id)
                || self.node(id) != before.as_ref()
                || after.as_ref().is_some_and(|n| n.character != id)
            {
                return Err(SocialError::Stale);
            }
            if let Some(n) = after {
                n.validate()?;
            }
            count = count
                .checked_sub(usize::from(before.is_some()))
                .ok_or(SocialError::Invalid)?
                .checked_add(usize::from(after.is_some()))
                .ok_or(SocialError::Overflow)?;
        }
        if count > self.capacity {
            return Err(SocialError::Capacity);
        }
        ids.clear();
        for (before, after) in &patch.metadata {
            let id = before
                .as_ref()
                .or(after.as_ref())
                .ok_or(SocialError::Invalid)?
                .monarch;
            if !ids.insert(id)
                || self.metadata(id) != before.as_ref()
                || after.as_ref().is_some_and(|m| m.monarch != id)
            {
                return Err(SocialError::Stale);
            }
        }
        self.validate_patch_links(patch)
    }
    pub fn adopt(&mut self, patch: AllegiancePatch) -> Result<(), SocialError> {
        self.validate_patch(&patch)?;
        for (before, after) in patch.nodes {
            let id = before
                .as_ref()
                .or(after.as_ref())
                .expect("validated node")
                .character;
            if let Some(after) = after {
                self.nodes.insert(id, after);
            } else {
                self.nodes.remove(&id);
            }
        }
        for (before, after) in patch.metadata {
            let id = before
                .as_ref()
                .or(after.as_ref())
                .expect("validated metadata")
                .monarch;
            if let Some(after) = after {
                self.groups.insert(id, after);
            } else {
                self.groups.remove(&id);
            }
        }
        self.revision = patch.after_revision;
        Ok(())
    }
}
