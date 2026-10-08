//! ResetHomePosition changes only the population owner's target home. Current
//! location and heading always come from accepted authoritative physics.
use super::*;
impl Population {
    pub(crate) fn script_home(&self, actor: EntityId) -> Option<bace_gameplay_api::NpcDestination> {
        let npc = self.npcs.get(&actor)?;
        let (cell, position) = npc.home?;
        let heading = npc.home_heading?;
        Some(bace_gameplay_api::NpcDestination {
            cell: Some(cell),
            position,
            rotation: [(heading * 0.5).cos(), 0.0, 0.0, (heading * 0.5).sin()],
            relative: false,
        })
    }
    pub(crate) fn reset_script_home(
        &mut self,
        actor: EntityId,
        world: &World,
    ) -> Result<bace_gameplay_api::NpcDestination, PveError> {
        let (cell, state) = world
            .actor_state(actor)
            .map_err(|_| PveError::MissingActor)?;
        let npc = self.npcs.get_mut(&actor).ok_or(PveError::MissingActor)?;
        npc.home = Some((cell, state.position()));
        npc.home_heading = Some(state.heading_radians());
        npc.returning_since = None;
        self.script_home(actor).ok_or(PveError::MissingActor)
    }
}
