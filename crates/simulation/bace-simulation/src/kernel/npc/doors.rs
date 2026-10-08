//! Native door requests enter DoorAuthority; authored requests never set
//! physical ethereality directly or pretend an animation has completed.
use super::Kernel;
use crate::{NpcEffect, NpcProposal};
use bace_gameplay_api::{DoorChange, NpcCompletion, NpcFailure as E, NpcOperation};
impl Kernel {
    pub fn apply_npc_door(&mut self, expected: &NpcProposal) -> Result<DoorChange, E> {
        self.npcs.validate_service(expected)?;
        let NpcEffect::Service(NpcOperation::OpenSelf(open)) = expected.effect else {
            return Err(E::Unsupported);
        };
        let result = self
            .doors
            .scripted_state(expected.context.source, open, self.tick)
            .map_err(|_| E::Conflict)?;
        if matches!(result, DoorChange::Busy | DoorChange::Locked) {
            return Err(E::Conflict);
        }
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.confirm_npc_committed(expected)?;
        Ok(result)
    }
}
