//! Source corpse permissions and the single live container viewer.
//! Decisions are proposals; durable permit consumption and IsLooted changes
//! must commit before `adopt_corpse_access` publishes success.
use super::*;
use crate::player_death::{
    CorpseAccessCommand, CorpseAccessDecision as D, CorpseAccessDenial as Denial,
    CorpseAccessError as E, CorpseAccessInspection, CorpseAccessOutcome, CorpseAccessProfile,
    CorpseAccessState,
};

impl Kernel {
    pub(crate) fn apply_corpse_access_command(&mut self, command: CorpseAccessCommand) {
        match command {
            CorpseAccessCommand::Inspect {
                correlation,
                context,
                corpse,
                unix_seconds,
            } => {
                let has_loot_permit = self
                    .player_deaths
                    .corpse_access
                    .get(&corpse)
                    .and_then(|state| state.profile.victim)
                    .and_then(|victim| {
                        self.player_deaths
                            .consent_grants
                            .get(&context.actor)?
                            .get(&victim)
                    })
                    .is_some_and(|grant| grant.expires_at >= unix_seconds);
                let shares_killer_fellowship = self.corpse_fellowship_right(corpse, context.actor);
                let result = self.authorize_corpse_use(context, corpse).and_then(|()| {
                    let decision = self.inspect_corpse_access(
                        corpse,
                        context.actor,
                        has_loot_permit,
                        shares_killer_fellowship,
                    )?;
                    let state = self
                        .player_deaths
                        .corpse_access
                        .get(&corpse)
                        .ok_or(E::Missing)?;
                    Ok(CorpseAccessInspection {
                        operation: state.operation,
                        profile: state.profile.clone(),
                        decision,
                        has_loot_permit,
                    })
                });
                self.player_deaths.corpse_access_outcomes.push_back(
                    CorpseAccessOutcome::Inspected {
                        correlation,
                        actor: context.actor,
                        corpse,
                        result,
                    },
                );
            }
            CorpseAccessCommand::Adopt {
                correlation,
                context,
                corpse,
                has_loot_permit,
                decision,
            } => {
                let shares_killer_fellowship = self.corpse_fellowship_right(corpse, context.actor);
                let result = (if matches!(decision, D::Close { .. }) {
                    Ok(())
                } else {
                    self.authorize_corpse_use(context, corpse)
                })
                .and_then(|()| {
                    self.adopt_corpse_access(
                        corpse,
                        context.actor,
                        has_loot_permit,
                        shares_killer_fellowship,
                        decision,
                    )
                })
                .map(|()| decision);
                self.player_deaths
                    .corpse_access_outcomes
                    .push_back(CorpseAccessOutcome::Adopted {
                        correlation,
                        actor: context.actor,
                        corpse,
                        result,
                    });
            }
        }
    }

    fn corpse_fellowship_right(&self, corpse: EntityId, actor: EntityId) -> bool {
        let killer = self
            .player_deaths
            .corpse_access
            .get(&corpse)
            .and_then(|state| state.profile.killer);
        let Some(killer_group) = killer.and_then(|killer| self.fellowships.membership(killer))
        else {
            return false;
        };
        killer_group.share_loot()
            && self
                .fellowships
                .membership(actor)
                .is_some_and(|group| group.id == killer_group.id)
    }

    fn authorize_corpse_use(
        &self,
        context: bace_gameplay_api::ActionContext,
        corpse: EntityId,
    ) -> Result<(), E> {
        self.characters
            .can_take_complete(CharacterBinding {
                actor: context.actor,
                account: context.account,
                session: context.session,
            })
            .map_err(|_| E::Ownership)?;
        let (cell, actor) = self
            .world
            .actor_state(context.actor)
            .map_err(|_| E::Missing)?;
        let (corpse_cell, target) = self.world.actor_state(corpse).map_err(|_| E::Missing)?;
        let actor_body = self.world.body(context.actor).map_err(|_| E::Missing)?;
        let corpse_body = self.world.body(corpse).map_err(|_| E::Missing)?;
        let actor_shape = actor_body.collision_shape().ok_or(E::Missing)?;
        let corpse_shape = corpse_body.collision_shape().ok_or(E::Missing)?;
        let mut target_position = target.position();
        if cell != corpse_cell {
            target_position = target_position
                + self
                    .world
                    .geometry()
                    .ok_or(E::Missing)?
                    .frame_offset(cell.0, corpse_cell.0)
                    .map_err(|_| E::Missing)?;
        }
        let cylinder = |position: bace_geometry::Vec3, shape: &bace_physics::CollisionShape| {
            bace_inventory::InventoryCylinder {
                position: [position.x, position.y, position.z],
                radius: shape.nominal_radius().unwrap_or(shape.horizontal_radius()),
                height: shape.nominal_height().unwrap_or(shape.height()),
            }
        };
        let distance = bace_inventory::inventory_use_distance(
            cylinder(actor.position(), actor_shape),
            cylinder(target_position, corpse_shape),
        )
        .ok_or(E::Invalid)?;
        let use_radius = match self
            .world
            .properties(corpse)
            .and_then(|p| p.get(bace_entity::PropertyFamily::Float, 54))
        {
            Some(bace_entity::PropertyValue::Float(value))
                if value.is_finite() && *value > 0. && *value <= 20. =>
            {
                *value
            }
            Some(_) => return Err(E::Invalid),
            // ACE Container.SetEphemeralValues supplies 0.5 when unauthored.
            None => 0.5,
        };
        if distance > use_radius {
            return Err(E::OutOfRange);
        }
        let from = actor.position() + bace_geometry::Vec3::new(0., 0., actor_shape.height() * 0.5);
        let to = target_position + bace_geometry::Vec3::new(0., 0., corpse_shape.height() * 0.5);
        if !self
            .world
            .segment_clear(cell, from, to, Some(corpse.0))
            .map_err(|_| E::Missing)?
        {
            return Err(E::Obstructed);
        }
        Ok(())
    }

    pub fn take_corpse_access_outcome(&mut self) -> Option<CorpseAccessOutcome> {
        self.player_deaths.corpse_access_outcomes.pop_front()
    }
    pub fn restore_corpse_access_outcome(
        &mut self,
        outcome: CorpseAccessOutcome,
    ) -> Result<(), CorpseAccessOutcome> {
        if self.player_deaths.corpse_access_outcomes.len() >= self.player_deaths.capacity {
            return Err(outcome);
        }
        self.player_deaths
            .corpse_access_outcomes
            .push_front(outcome);
        Ok(())
    }

    pub fn register_corpse_access(
        &mut self,
        corpse: EntityId,
        death_operation: u64,
        profile: CorpseAccessProfile,
    ) -> Result<(), E> {
        profile.validate()?;
        if corpse.0 == 0 || death_operation == 0 {
            return Err(E::Invalid);
        }
        if self
            .world
            .corpse(corpse)
            .is_none_or(|state| state.operation != death_operation)
        {
            return Err(E::Stale);
        }
        if let Some(existing) = self.player_deaths.corpse_access.get(&corpse) {
            return if existing.operation == death_operation && existing.profile == profile {
                Ok(())
            } else {
                Err(E::Stale)
            };
        }
        if self.player_deaths.corpse_access.len() >= 4096 {
            return Err(E::Capacity);
        }
        self.player_deaths.corpse_access.insert(
            corpse,
            CorpseAccessState {
                operation: death_operation,
                profile,
                viewer: None,
            },
        );
        Ok(())
    }

    /// The caller supplies fellowship and permit rights from their sole owners.
    /// No client-supplied flag grants access. ACE's monster public threshold is
    /// strictly less than 180 seconds remaining, except for rare corpses.
    pub fn inspect_corpse_access(
        &self,
        corpse: EntityId,
        actor: EntityId,
        has_loot_permit: bool,
        shares_killer_fellowship: bool,
    ) -> Result<D, E> {
        if actor.0 == 0 {
            return Err(E::Invalid);
        }
        let state = self
            .player_deaths
            .corpse_access
            .get(&corpse)
            .ok_or(E::Missing)?;
        if self
            .world
            .corpse(corpse)
            .is_none_or(|world| world.operation != state.operation)
        {
            return Err(E::Stale);
        }
        let remaining = self
            .corpse_expiry
            .deadlines
            .get(&corpse)
            .filter(|deadline| deadline.operation == state.operation)
            .map(|deadline| deadline.tick.saturating_sub(self.tick));
        let decision = decide(
            &state.profile,
            state.viewer,
            actor,
            has_loot_permit,
            shares_killer_fellowship,
            remaining,
        );
        if matches!(
            decision,
            D::Open {
                consume_permit: true
            }
        ) && state.profile.permittees.len() >= 1024
        {
            return Err(E::Capacity);
        }
        Ok(decision)
    }

    /// Called only after any one-shot permit or close mutation has a confirmed
    /// durable receipt. Recheck the exact decision under the simulation owner.
    pub fn adopt_corpse_access(
        &mut self,
        corpse: EntityId,
        actor: EntityId,
        has_loot_permit: bool,
        shares_killer_fellowship: bool,
        decision: D,
    ) -> Result<(), E> {
        if self.inspect_corpse_access(corpse, actor, has_loot_permit, shares_killer_fellowship)?
            != decision
        {
            return Err(E::Stale);
        }
        let state = self
            .player_deaths
            .corpse_access
            .get_mut(&corpse)
            .ok_or(E::Missing)?;
        let consumed_victim = match decision {
            D::Open {
                consume_permit: true,
            } => Some(state.profile.victim.ok_or(E::Invalid)?),
            _ => None,
        };
        match decision {
            D::Open { consume_permit } => {
                if consume_permit {
                    if state.profile.permittees.len() >= 1024 {
                        return Err(E::Capacity);
                    }
                    let index = state.profile.permittees.binary_search(&actor).unwrap_err();
                    state.profile.permittees.insert(index, actor);
                }
                state.viewer = Some(actor);
            }
            D::Close { mark_looted } => {
                state.viewer = None;
                if mark_looted {
                    state.profile.looted = true;
                }
            }
            D::Denied(_) => return Err(E::Invalid),
        }
        if let Some(victim) = consumed_victim
            && let Some(grants) = self.player_deaths.consent_grants.get_mut(&actor)
        {
            grants.remove(&victim);
            if grants.is_empty() {
                self.player_deaths.consent_grants.remove(&actor);
            }
        }
        Ok(())
    }

    /// The disconnected viewer remains attached until its close checkpoint
    /// commits. This keeps expiry deferred through uncertain database outcomes.
    pub fn pending_corpse_viewer_detach(&self, actor: EntityId) -> Vec<(EntityId, bool)> {
        let mut pending = Vec::new();
        for (&corpse, state) in &self.player_deaths.corpse_access {
            if state.viewer == Some(actor) {
                pending.push((corpse, !state.profile.looted));
            }
        }
        pending
    }

    pub fn forget_corpse_access(&mut self, corpse: EntityId, operation: u64) -> Result<(), E> {
        if self
            .player_deaths
            .corpse_access
            .get(&corpse)
            .is_none_or(|state| state.operation != operation || state.viewer.is_some())
        {
            return Err(E::Stale);
        }
        self.player_deaths.corpse_access.remove(&corpse);
        Ok(())
    }

    pub fn corpse_is_open(&self, corpse: EntityId) -> bool {
        self.player_deaths
            .corpse_access
            .get(&corpse)
            .is_some_and(|state| state.viewer.is_some())
    }
}

fn decide(
    profile: &CorpseAccessProfile,
    viewer: Option<EntityId>,
    actor: EntityId,
    has_loot_permit: bool,
    shares_killer_fellowship: bool,
    remaining_ticks: Option<u64>,
) -> D {
    if let Some(viewer) = viewer {
        return if viewer == actor {
            D::Close {
                mark_looted: !profile.looted,
            }
        } else {
            D::Denied(Denial::InUse)
        };
    }
    let direct = profile.victim.is_none_or(|victim| victim == actor)
        || profile.killer == Some(actor)
        || profile.looted;
    if direct {
        return D::Open {
            consume_permit: false,
        };
    }
    if profile.permittees.binary_search(&actor).is_ok() {
        return D::Open {
            consume_permit: false,
        };
    }
    if has_loot_permit && !profile.pk_death {
        return D::Open {
            consume_permit: true,
        };
    }
    let public = profile.is_monster
        && !profile.generated_rare
        && remaining_ticks.is_some_and(|remaining| remaining < 180 * 30);
    if public || shares_killer_fellowship && !profile.generated_rare && !profile.pk_death {
        return D::Open {
            consume_permit: false,
        };
    }
    D::Denied(if profile.generated_rare {
        Denial::Rare
    } else if profile.pk_death {
        Denial::PlayerKiller
    } else {
        Denial::Locked
    })
}

#[cfg(test)]
#[path = "corpse_access/tests.rs"]
mod tests;
