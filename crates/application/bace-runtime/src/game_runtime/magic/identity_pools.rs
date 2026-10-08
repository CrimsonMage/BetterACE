//! Durable identities are allocated off the simulation thread and supplied in
//! exact retained batches. A rejected supply never triggers replacement IDs.
use super::*;
use bace_simulation::{
    MagicResourceAction, MagicResourceCommand, MagicResourceOutcome, MagicResourceResult,
};
use bace_types::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Projectile,
    Portal,
}
struct Pool {
    kind: Kind,
    next_query: u64,
    remaining: usize,
    capacity: Option<usize>,
    ids: Vec<EntityId>,
    command: Option<MagicResourceCommand>,
    submitted: bool,
    failure: Option<String>,
}
impl Pool {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            next_query: 0,
            remaining: 0,
            capacity: None,
            ids: Vec::new(),
            command: None,
            submitted: false,
            failure: None,
        }
    }
}
struct Allocation {
    kind: Kind,
    result: Result<Vec<u32>, String>,
}
pub(super) struct IdentityPools {
    pools: [Pool; 2],
    allocation: Option<Job<Allocation>>,
}
impl IdentityPools {
    pub(super) fn new() -> Self {
        Self {
            pools: [Pool::new(Kind::Projectile), Pool::new(Kind::Portal)],
            allocation: None,
        }
    }
    pub(super) fn pending(&self) -> bool {
        self.allocation.is_some()
            || self
                .pools
                .iter()
                .any(|p| p.command.is_some() || !p.ids.is_empty())
    }
}
impl GameRuntime {
    pub(super) fn poll_magic_identity_pools(&mut self, unix: u64) -> Result<(), String> {
        if let Some(done) = ready(&mut self.magic.identity_pools.allocation) {
            let pool = self
                .magic
                .identity_pools
                .pools
                .iter_mut()
                .find(|p| p.kind == done.kind)
                .ok_or("magic identity pool correlation")?;
            match done.result {
                Ok(ids) => {
                    if !pool.ids.is_empty()
                        || ids.is_empty()
                        || ids.len() > 64
                        || ids.windows(2).any(|p| p[0] >= p[1])
                        || ids.iter().any(|id| !(0x80000000..=0xfffffffe).contains(id))
                    {
                        return Err("invalid durable magic identity allocation".into());
                    }
                    pool.ids = ids.into_iter().map(EntityId).collect();
                    pool.failure = None;
                }
                Err(error) => {
                    pool.failure = Some(error);
                    pool.next_query = unix.saturating_add(1000);
                }
            }
        }
        for index in 0..2 {
            let pool = &self.magic.identity_pools.pools[index];
            if pool.submitted || pool.next_query > unix {
                continue;
            }
            if pool.command.is_none() {
                let fill = pool
                    .capacity
                    .map(|capacity| capacity.saturating_sub(pool.remaining).min(64))
                    .unwrap_or(0);
                if pool.ids.is_empty() && fill > 0 && !self.draining {
                    if self.magic.identity_pools.allocation.is_none() {
                        let kind = pool.kind;
                        let store = self.bootstrap.store.clone();
                        self.magic.identity_pools.allocation = Some(Box::pin(async move {
                            Allocation {
                                kind,
                                result: store
                                    .allocate_dynamic_ids(fill as u16)
                                    .await
                                    .map_err(|e| e.to_string()),
                            }
                        }));
                    }
                    continue;
                }
                if self.draining && pool.ids.is_empty() {
                    continue;
                }
                let correlation = self.token()?;
                let pool = &mut self.magic.identity_pools.pools[index];
                let action = match pool.kind {
                    Kind::Projectile => MagicResourceAction::SupplyProjectileIds(pool.ids.clone()),
                    Kind::Portal => MagicResourceAction::SupplyPortalIds(pool.ids.clone()),
                };
                pool.command = Some(MagicResourceCommand {
                    correlation,
                    binding: None,
                    action,
                });
            }
            let pool = &mut self.magic.identity_pools.pools[index];
            match self.simulation.input().try_submit(Command::MagicResource(
                pool.command.as_ref().ok_or("magic pool command")?.clone(),
            )) {
                Ok(()) => pool.submitted = true,
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err("magic identity owner closed".into());
                }
            }
        }
        Ok(())
    }
    pub(super) fn accept_magic_identity_pool(
        &mut self,
        done: &MagicResourceOutcome,
        unix: u64,
    ) -> Result<bool, String> {
        let Some(pool) = self.magic.identity_pools.pools.iter_mut().find(|p| {
            p.submitted
                && p.command
                    .as_ref()
                    .is_some_and(|c| c.correlation == done.correlation)
        }) else {
            return Ok(false);
        };
        if done.binding.is_some() {
            return Err("magic identity binding mismatch".into());
        }
        match &done.result {
            Ok(MagicResourceResult::ProjectileIds {
                remaining,
                capacity,
            }) if pool.kind == Kind::Projectile => {
                pool.remaining = *remaining;
                pool.capacity = Some(*capacity);
            }
            Ok(MagicResourceResult::PortalIds {
                remaining,
                capacity,
            }) if pool.kind == Kind::Portal => {
                pool.remaining = *remaining;
                pool.capacity = Some(*capacity);
            }
            Err(error) => {
                // Owner rejected the entire batch. Keep these allocated IDs and
                // exact command for retry; no uncertainty permits replacement.
                pool.failure = Some(format!("magic identity supply: {error:?}"));
                pool.submitted = false;
                pool.next_query = unix.saturating_add(1000);
                return Ok(true);
            }
            _ => return Err("magic identity result kind mismatch".into()),
        }
        pool.ids.clear();
        pool.command = None;
        pool.submitted = false;
        pool.failure = None;
        pool.next_query = unix.saturating_add(100);
        Ok(true)
    }
}
impl GameRuntime {
    /// Recoverable allocation/supply pressure remains visible while other magic
    /// durability and output lanes continue to drain on every poll.
    pub fn magic_background_failure(&self) -> Option<&str> {
        self.magic
            .identity_pools
            .pools
            .iter()
            .find_map(|pool| pool.failure.as_deref())
    }
}
