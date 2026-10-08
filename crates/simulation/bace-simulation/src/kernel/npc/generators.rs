//! Generate delegates selection to the existing incarnation-fenced generator.
//! Spawn work remains in its bounded asset/world admission queue.
use super::Kernel;
use crate::{GeneratorControl, NpcEffect, NpcProposal};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
impl Kernel {
    pub fn apply_npc_generate(&mut self, expected: &NpcProposal) -> Result<(), E> {
        self.npcs.validate_service(expected)?;
        if !matches!(expected.effect, NpcEffect::Service(NpcOperation::Generate)) {
            return Err(E::Unsupported);
        }
        let identity = self
            .generators
            .machines
            .get(&expected.context.source)
            .ok_or(E::MissingContent)?
            .definition()
            .identity;
        self.generator_control(identity, GeneratorControl::Generate)
            .map_err(|_| E::Conflict)?;
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.confirm_npc_committed(expected)
    }
}
