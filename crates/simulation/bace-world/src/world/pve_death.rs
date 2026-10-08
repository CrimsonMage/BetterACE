//! Durable PVE death replaces one dead source with exact prepared world roots.
//! Root placement uses authored static geometry; loot roots intentionally share
//! the copied death pose and are ethereal anchors rather than dynamic colliders.
use super::*;

pub type PveWorldRoots = Vec<(Actor, Option<CorpseState>)>;
pub type PveWorldReplaceError = (WorldError, PveWorldRoots);

impl World {
    pub fn preflight_pve_death_roots(
        &self,
        source: EntityId,
        roots: &[(Actor, Option<CorpseState>)],
    ) -> Result<(), WorldError> {
        if roots.len() > 257 || self.anchors.len() + roots.len() > 4096 {
            return Err(PhysicsError::InvalidState.into());
        }
        let victim = self.actors.get(&source).ok_or(WorldError::MissingActor)?;
        if self.combatants.get(&source).is_none_or(|c| c.health() != 0) {
            return Err(WorldError::LivingActor);
        }
        if self.health_observation_pending_for(source)
            || self.has_reserved_vitals(source)
            || self.retirement_holds.contains_key(&source)
        {
            return Err(WorldError::VitalReserved);
        }
        let region = self.geometry.as_ref().ok_or(WorldError::MissingGeometry)?;
        let pose = victim.body.accepted();
        let mut ids = BTreeSet::new();
        let corpse_count = roots.iter().filter(|(_, corpse)| corpse.is_some()).count();
        if corpse_count > 1 || corpse_count == 1 && roots.len() != 1 {
            return Err(WorldError::DuplicateActor);
        }
        for (actor, corpse) in roots {
            if actor.id.0 == 0
                || actor.id.0 == u32::MAX
                || actor.id == source
                || !ids.insert(actor.id)
                || self.contains_identity(actor.id)
                || actor.cell != victim.cell
                || actor.body.accepted().position() != pose.position()
                || (actor.body.accepted().heading_radians() - pose.heading_radians()).abs() > 0.0001
                || corpse.as_ref().is_some_and(|state| {
                    state.operation == 0 || state.source != source || state.items.len() > 256
                })
            {
                return Err(WorldError::DuplicateActor);
            }
            let shape = actor
                .body
                .collision_shape()
                .ok_or(WorldError::MissingGeometry)?;
            region
                .validate_placement(
                    actor.cell.0,
                    actor.body.accepted().position(),
                    shape,
                    &[],
                    actor.id.0,
                )
                .map_err(PhysicsError::from)?;
        }
        Ok(())
    }

    pub fn replace_dead_pve_with_roots(
        &mut self,
        source: EntityId,
        roots: PveWorldRoots,
    ) -> Result<(), PveWorldReplaceError> {
        if let Err(error) = self.preflight_pve_death_roots(source, &roots) {
            return Err((error, roots));
        }
        self.remove(source)
            .expect("preflighted dead source retirement");
        for (actor, corpse) in roots {
            self.anchors.insert(actor.id);
            if let Some(state) = corpse {
                self.corpses.insert(actor.id, state);
            }
            self.actors.insert(actor.id, actor);
        }
        self.visibility.invalidate();
        Ok(())
    }
}
