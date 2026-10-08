//! Single authoritative fellowship index, shared by networking, NPCs and rewards.
use bace_fellowship::Fellowship;
use bace_gameplay_api::social::SocialError;
use bace_quests::QuestRegistry;
use bace_types::EntityId;
use std::collections::BTreeMap;
pub(crate) struct Fellowships {
    pub(crate) groups: BTreeMap<u64, Fellowship>,
    pub(crate) members: BTreeMap<EntityId, u64>,
    pub(crate) quests: BTreeMap<u64, QuestRegistry>,
    pub(crate) panels: std::collections::BTreeSet<EntityId>,
    pub(crate) capacity: usize,
    pub(crate) next: u64,
}
impl Fellowships {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            groups: BTreeMap::new(),
            members: BTreeMap::new(),
            quests: BTreeMap::new(),
            panels: Default::default(),
            capacity: capacity.min(4096),
            next: 0,
        }
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.groups.is_empty()
    }
    pub(crate) fn register(&mut self, fellow: Fellowship) -> Result<(), SocialError> {
        if self.groups.len() >= self.capacity {
            return Err(SocialError::Capacity);
        }
        if self.groups.contains_key(&fellow.id)
            || fellow
                .members()
                .iter()
                .any(|id| self.members.contains_key(id))
        {
            return Err(SocialError::Duplicate);
        }
        let quests = QuestRegistry::new(4096).map_err(|_| SocialError::Capacity)?;
        for member in fellow.members() {
            self.members.insert(*member, fellow.id);
        }
        self.next = self.next.max(fellow.id);
        self.quests.insert(fellow.id, quests);
        self.groups.insert(fellow.id, fellow);
        Ok(())
    }
    pub(crate) fn membership(&self, actor: EntityId) -> Option<&Fellowship> {
        self.members.get(&actor).and_then(|id| self.groups.get(id))
    }
    pub(crate) fn roster(&self, actor: EntityId) -> Option<&[EntityId]> {
        self.membership(actor).map(Fellowship::members)
    }
    pub(crate) fn replace(
        &mut self,
        before: &Fellowship,
        after: Fellowship,
    ) -> Result<(), SocialError> {
        if self.groups.get(&before.id) != Some(before) || after.id != before.id {
            return Err(SocialError::Stale);
        }
        if after.members().iter().any(|id| {
            self.members
                .get(id)
                .is_some_and(|group| *group != before.id)
        }) {
            return Err(SocialError::Duplicate);
        }
        for id in before.members() {
            self.members.remove(id);
        }
        for id in after.members() {
            self.members.insert(*id, after.id);
        }
        if after.members().is_empty() {
            self.groups.remove(&after.id);
            self.quests.remove(&after.id);
        } else {
            self.groups.insert(after.id, after);
        }
        Ok(())
    }
    pub(crate) fn disband(&mut self, id: u64) -> Result<Vec<EntityId>, SocialError> {
        let fellow = self.groups.remove(&id).ok_or(SocialError::Missing)?;
        let members = fellow.members().to_vec();
        for actor in &members {
            self.members.remove(actor);
            self.panels.remove(actor);
        }
        self.quests.remove(&id);
        Ok(members)
    }
}
