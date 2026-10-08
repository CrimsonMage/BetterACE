//! Bounded authoritative fellowship identity and emote-visible membership.
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fellowship {
    pub id: u64,
    pub(crate) name: String,
    pub(crate) leader: EntityId,
    pub(crate) members: Vec<EntityId>,
    pub(crate) locked: bool,
    pub(crate) lock_name: Option<String>,
    pub(crate) share_xp: bool,
    pub(crate) share_loot: bool,
    pub(crate) revision: u64,
    pub(crate) open: bool,
    pub(crate) departed: std::collections::BTreeMap<EntityId, u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FellowshipError {
    Invalid,
    Capacity,
    Duplicate,
    NotLeader,
    NotMember,
    Locked,
    Overflow,
}
impl Fellowship {
    pub fn new(
        id: u64,
        name: String,
        leader: EntityId,
        share_xp: bool,
    ) -> Result<Self, FellowshipError> {
        if id == 0 || leader.0 == 0 || name.is_empty() || name.len() > 255 {
            return Err(FellowshipError::Invalid);
        }
        Ok(Self {
            id,
            name,
            leader,
            members: vec![leader],
            locked: false,
            lock_name: None,
            share_xp,
            share_loot: false,
            revision: 0,
            open: false,
            departed: Default::default(),
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn leader(&self) -> EntityId {
        self.leader
    }
    pub fn members(&self) -> &[EntityId] {
        &self.members
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn share_xp(&self) -> bool {
        self.share_xp
    }
    pub fn share_loot(&self) -> bool {
        self.share_loot
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn join(&mut self, actor: EntityId) -> Result<(), FellowshipError> {
        if actor.0 == 0 {
            return Err(FellowshipError::Invalid);
        }
        if self.members.contains(&actor) {
            return Err(FellowshipError::Duplicate);
        }
        if self.locked {
            return Err(FellowshipError::Locked);
        }
        if self.members.len() >= 9 {
            return Err(FellowshipError::Capacity);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(FellowshipError::Overflow)?;
        self.members.push(actor);
        self.revision = revision;
        Ok(())
    }
    pub fn lock_from_emote(
        &mut self,
        actor: EntityId,
        locked: bool,
        name: Option<String>,
    ) -> Result<(), FellowshipError> {
        if !self.members.contains(&actor) {
            return Err(FellowshipError::NotMember);
        }
        if name.as_ref().is_some_and(|n| n.len() > 256) {
            return Err(FellowshipError::Invalid);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(FellowshipError::Overflow)?;
        self.locked = locked;
        self.lock_name = name;
        self.revision = revision;
        Ok(())
    }
    pub fn lock(
        &mut self,
        actor: EntityId,
        locked: bool,
        name: Option<String>,
    ) -> Result<(), FellowshipError> {
        if actor != self.leader {
            return Err(FellowshipError::NotLeader);
        }
        if name.as_ref().is_some_and(|n| n.len() > 256) {
            return Err(FellowshipError::Invalid);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(FellowshipError::Overflow)?;
        self.locked = locked;
        self.lock_name = name;
        self.revision = revision;
        Ok(())
    }
    /// Pinned SplitLuminance divides by ALL fellows before range/eligibility
    /// filtering. Quest luminance is never fellowship-shared.
    pub fn split_luminance(
        &self,
        amount: u64,
        source: EntityId,
        quest: bool,
        eligible_in_range: &[EntityId],
    ) -> Result<Vec<(EntityId, u64)>, FellowshipError> {
        if !self.members.contains(&source) {
            return Err(FellowshipError::NotMember);
        }
        if amount > i64::MAX as u64 {
            return Err(FellowshipError::Overflow);
        }
        if quest || !self.share_xp {
            return Ok(vec![(source, amount)]);
        }
        let share = amount / self.members.len() as u64;
        Ok(self
            .members
            .iter()
            .filter(|m| eligible_in_range.contains(m))
            .map(|id| (*id, share))
            .collect())
    }
}
