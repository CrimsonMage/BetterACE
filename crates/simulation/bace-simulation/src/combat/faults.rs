//! A reported physical fault is terminal unless a valuable receipt or accepted
//! spell side effect is still outstanding. Cleanup never cancels a newer owner.
use super::*;
pub(super) fn end_if_owned(
    world: &mut World,
    actor: EntityId,
    owner: u64,
) -> Result<(), bace_world::WorldError> {
    if world.source_motion_token(actor).is_some_and(|token| {
        token.domain == bace_motion::MotionDomain::Physical && token.owner == owner
    }) {
        world.end_physical_motion(actor, owner)
    } else {
        Ok(())
    }
}
impl Combat {
    pub(super) fn cleanup_physical_faults(&mut self, world: &mut World) {
        let actor = self.physical_attacks.iter().find_map(|(&actor, attack)| {
            (attack.faulted
                && !super::procs::has_pending_request(&self.physical_hits, actor, attack.operation))
            .then_some((
                actor,
                attack.operation,
                attack.motion.as_ref().map(|m| m.token.owner),
            ))
        });
        if let Some((actor, operation, owner)) = actor
            && owner.is_none_or(|owner| end_if_owned(world, actor, owner).is_ok())
        {
            self.physical_hits
                .retain(|key, _| key.attacker != actor || key.operation != operation);
            self.physical_attacks.remove(&actor);
            self.stop_physical_approach(world, actor);
            if let Some(driver) = self.physical_drivers.get_mut(&actor) {
                driver.cancel_operation(operation);
            }
        }
        let missile = self.missiles.iter().find_map(|(&operation, state)| {
            let actor = EntityId(state.proposal.actor);
            (state.faulted
                && (!state.submitted || state.launched)
                && !super::procs::has_pending_request(&self.physical_hits, actor, operation))
            .then_some((
                actor,
                operation,
                EntityId(state.proposal.projectile),
                state.launched,
                state.motion.as_ref().map(|m| m.token.owner),
            ))
        });
        if let Some((actor, operation, projectile, launched, owner)) = missile
            && (!launched || self.physical_events.len() < self.capacity.max(2))
            && owner.is_none_or(|owner| end_if_owned(world, actor, owner).is_ok())
        {
            self.physical_hits
                .retain(|key, _| key.attacker != actor || key.operation != operation);
            self.missiles.remove(&operation);
            self.launches.retain(|p| p.operation != operation);
            if launched {
                world.remove_projectile(projectile);
                self.physical_events
                    .push_back(PhysicalCombatEvent::ProjectileRemoved {
                        actor: projectile,
                        tick: self.simulation_tick,
                    });
            }
            if let Some(driver) = self.physical_drivers.get_mut(&actor) {
                driver.cancel_operation(operation);
            }
        }
    }
}
