//! Source player corpses spill roots with their descendant trees before decay.
use super::*;
use bace_inventory::ItemPlace;
impl Kernel {
    pub(super) fn prepare_corpse_spill_intent(
        &self,
        corpse: EntityId,
        proposal: &mut bace_inventory::InventoryProposal,
    ) -> Result<Option<CorpseSpillIntent>, E> {
        let state = self.world.corpse(corpse).ok_or(E::Stale)?;
        if !(0x5000_0001..=0x5fff_ffff).contains(&state.source.0) {
            return Ok(None);
        }
        let roots: Vec<_> = proposal.changes.iter().filter_map(|c| c.before.as_ref())
            .filter(|b| matches!(b.place, ItemPlace::Contained { container, .. } if container == corpse))
            .map(|b| b.id).collect();
        if roots.is_empty() {
            return Ok(None);
        }
        let (cell, accepted) = self
            .world
            .actor_state(corpse)
            .map_err(|_| E::MissingAssets)?;
        let p = accepted.position();
        let half = accepted.heading_radians() * 0.5;
        for change in &mut proposal.changes {
            if change.after.id == corpse {
                continue;
            }
            change.after.place = if roots.contains(&change.after.id) {
                ItemPlace::World
            } else {
                change.before.as_ref().ok_or(E::Invalid)?.place
            };
        }
        Ok(Some(CorpseSpillIntent {
            roots,
            position: bace_gameplay_api::GeneratorLocation {
                cell: cell.0,
                origin: [p.x, p.y, p.z],
                rotation: [0., 0., half.sin(), half.cos()],
            },
        }))
    }
    pub(in crate::kernel) fn prepare_corpse_spill(
        &mut self,
        prepared: PreparedCorpseSpill,
    ) -> Result<(), (E, PreparedCorpseSpill)> {
        let check = (|| {
            let pending = self
                .corpse_expiry
                .pending
                .get(&prepared.operation)
                .ok_or(E::Stale)?;
            let intent = pending.ticket.spill.as_ref().ok_or(E::Invalid)?;
            if !pending.submitted
                || pending.spill.is_some()
                || prepared.actors.len() != intent.roots.len()
            {
                return Err(E::Stale);
            }
            let ids: std::collections::BTreeSet<_> = prepared.actors.iter().map(|a| a.id).collect();
            if ids.len() != prepared.actors.len() || intent.roots.iter().any(|id| !ids.contains(id))
            {
                return Err(E::Invalid);
            }
            for actor in &prepared.actors {
                let p = actor.body.accepted().position();
                if actor.cell.0 != intent.position.cell
                    || p.x != intent.position.origin[0]
                    || p.y != intent.position.origin[1]
                    || p.z < intent.position.origin[2]
                {
                    return Err(E::Invalid);
                }
            }
            self.world
                .validate_corpse_spills(pending.ticket.corpse, &prepared.actors)
                .map_err(|_| E::MissingAssets)
        })();
        if let Err(error) = check {
            return Err((error, prepared));
        }
        self.corpse_expiry
            .pending
            .get_mut(&prepared.operation)
            .expect("checked expiry")
            .spill = Some(prepared.actors);
        Ok(())
    }
    pub(super) fn step_corpse_removals(&mut self) {
        let mut due = [(EntityId(0), 0); 32];
        let mut count = 0;
        for (&corpse, &(operation, at)) in &self.corpse_expiry.retiring {
            if at <= self.tick
                && self.corpse_expiry.events.len() + count < self.corpse_expiry.capacity
            {
                due[count] = (corpse, operation);
                count += 1;
                if count == due.len() {
                    break;
                }
            }
        }
        for (corpse, operation) in due.into_iter().take(count) {
            if self.world.remove(corpse).is_none() {
                continue;
            }
            self.corpse_expiry.retiring.remove(&corpse);
            self.corpse_expiry.events.push_back(CorpseExpiryEvent {
                corpse,
                death_operation: operation,
                phase: CorpseExpiryPhase::Removed,
            });
        }
    }
}
