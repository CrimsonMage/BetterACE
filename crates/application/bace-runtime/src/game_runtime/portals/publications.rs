//! Canonical portal state is not visible to generic replication until the
//! corresponding private batch has entered its generation's reliable peer.
use super::*;

const PREFIX: u64 = 0x5000_0000_0000_0000;
const MASK: u64 = 0xff00_0000_0000_0000;
const CAPACITY: usize = 64;

struct Publication {
    actor: EntityId,
    key: SessionKey,
    materialization: Option<(u64, u16)>,
    // Keep the exact bytes until reliable admission, not merely adapter admission.
    _messages: Vec<(u16, Vec<u8>)>,
}
#[derive(Default)]
pub(super) struct PortalPublications {
    next: u64,
    pending: BTreeMap<u64, Publication>,
}
impl PortalPublications {
    fn has_room(&self) -> bool {
        self.pending.len() < CAPACITY && self.next < !MASK
    }
    pub(in crate::game_runtime) fn retain(
        &mut self,
        actor: EntityId,
        key: SessionKey,
        messages: Vec<(u16, Vec<u8>)>,
        materialization: Option<(u64, u16)>,
    ) -> Result<NetworkCommand, String> {
        if !self.has_room() || messages.is_empty() {
            return Err("portal reliable publication capacity/empty batch".into());
        }
        self.next += 1;
        let correlation = PREFIX | self.next;
        self.pending.insert(
            correlation,
            Publication {
                actor,
                key,
                materialization,
                _messages: messages.clone(),
            },
        );
        Ok(NetworkCommand::SendReliableBatch {
            key,
            correlation,
            messages,
        })
    }
    fn resolve(
        &mut self,
        key: SessionKey,
        correlation: u64,
    ) -> Result<Option<(EntityId, u64, u16)>, String> {
        if self.pending.get(&correlation).is_none_or(|p| p.key != key) {
            return Err("portal reliable publication receipt mismatch".into());
        }
        let accepted = self
            .pending
            .remove(&correlation)
            .expect("matched portal publication");
        Ok(accepted
            .materialization
            .map(|(operation, epoch)| (accepted.actor, operation, epoch)))
    }
}
impl PortalRuntime {
    pub(in crate::game_runtime) fn publication_pending(&self) -> bool {
        !self.publications.pending.is_empty()
    }
    pub(in crate::game_runtime) fn publication_holds(&self, actor: EntityId) -> bool {
        self.publications.pending.values().any(|p| p.actor == actor)
    }
    pub(super) fn publication_room(&self) -> bool {
        self.publications.has_room()
    }
}
impl GameRuntime {
    pub(in crate::game_runtime) fn poll_portal_publications(&mut self) -> Result<(), String> {
        for _ in 0..self.limits.work_per_poll {
            let Some(index) = self
                .reliable_admissions
                .iter()
                .position(|(_, id, _)| id & MASK == PREFIX)
            else {
                break;
            };
            let (key, correlation, accepted) = self.reliable_admissions[index];
            let materialized = self.portals.publications.resolve(key, correlation)?;
            self.reliable_admissions.remove(index);
            if accepted
                && let Some((actor, operation, epoch)) = materialized
                && self.portals.death_materialization_watches.get(&actor)
                    == Some(&(operation, epoch))
            {
                self.portals
                    .materialized_receipts
                    .insert(actor, (operation, epoch));
            }
            if !accepted && let Some(session) = self.sessions.get_mut(&key) {
                // A definite rejected peer admission ends that generation. It
                // cannot be retried as if the peer had received the old epoch.
                session.terminated = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
