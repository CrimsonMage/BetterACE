//! ACE 47edade3 Entity/Fellowship.cs lifecycle. Fixed member bound is nine.
use crate::{Fellowship, FellowshipError as E};
use bace_types::EntityId;
impl Fellowship {
    pub fn open(&self) -> bool {
        self.open
    }
    pub fn lock_name(&self) -> Option<&str> {
        self.lock_name.as_deref()
    }
    pub fn departed(&self) -> impl Iterator<Item = (EntityId, u64)> + '_ {
        self.departed.iter().map(|(a, t)| (*a, *t))
    }
    pub fn set_share_loot_at_creation(&mut self, enabled: bool) -> Result<(), E> {
        if self.revision != 0 || self.members.len() != 1 {
            return Err(E::Invalid);
        }
        self.share_loot = enabled;
        Ok(())
    }
    pub fn can_recruit(&self, inviter: EntityId, recipient: EntityId, now: u64) -> Result<(), E> {
        if !self.members.contains(&inviter) {
            return Err(E::NotMember);
        }
        if inviter != self.leader && !self.open {
            return Err(E::NotLeader);
        }
        if self.members.contains(&recipient) {
            return Err(E::Duplicate);
        }
        if recipient.0 == 0 {
            return Err(E::Invalid);
        }
        if self.members.len() >= 9 {
            return Err(E::Capacity);
        }
        if self.locked
            && self
                .departed
                .get(&recipient)
                .is_none_or(|time| now > time.saturating_add(600))
        {
            return Err(E::Locked);
        }
        Ok(())
    }
    pub fn recruit_confirmed(
        &mut self,
        inviter: EntityId,
        recipient: EntityId,
        now: u64,
    ) -> Result<(), E> {
        self.can_recruit(inviter, recipient, now)?;
        let revision = self.next_revision()?;
        self.members.push(recipient);
        self.departed.remove(&recipient);
        self.revision = revision;
        Ok(())
    }
    pub fn change_openness(&mut self, actor: EntityId, open: bool) -> Result<(), E> {
        if actor != self.leader {
            return Err(E::NotLeader);
        }
        if self.locked {
            return Err(E::Locked);
        }
        let revision = self.next_revision()?;
        self.open = open;
        self.revision = revision;
        Ok(())
    }
    pub fn assign_leader(&mut self, actor: EntityId, successor: EntityId) -> Result<(), E> {
        if actor != self.leader {
            return Err(E::NotLeader);
        }
        if !self.members.contains(&successor) {
            return Err(E::NotMember);
        }
        let revision = self.next_revision()?;
        self.leader = successor;
        self.revision = revision;
        Ok(())
    }
    /// The owner resolves source successor policy from current member levels.
    pub fn remove(
        &mut self,
        actor: EntityId,
        dismissed: bool,
        now: u64,
        successor: Option<EntityId>,
    ) -> Result<(), E> {
        if !self.members.contains(&actor) {
            return Err(E::NotMember);
        }
        if actor == self.leader
            && self.members.len() > 1
            && successor.is_none_or(|s| s == actor || !self.members.contains(&s))
        {
            return Err(E::Invalid);
        }
        let revision = self.next_revision()?;
        self.members.retain(|id| *id != actor);
        if self.leader == actor
            && let Some(successor) = successor
        {
            self.leader = successor;
        }
        if self.locked && !dismissed {
            self.departed
                .retain(|_, time| now <= time.saturating_add(600));
            if self.departed.len() >= 9 {
                let oldest = self
                    .departed
                    .iter()
                    .min_by_key(|(_, t)| **t)
                    .map(|(id, _)| *id);
                if let Some(id) = oldest {
                    self.departed.remove(&id);
                }
            }
            self.departed.insert(actor, now);
        }
        self.revision = revision;
        Ok(())
    }
    fn next_revision(&self) -> Result<u64, E> {
        self.revision.checked_add(1).ok_or(E::Overflow)
    }
}
