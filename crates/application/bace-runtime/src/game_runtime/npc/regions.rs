//! NPC destinations use the same bounded simulation residency owner as players.
use super::*;
use std::sync::mpsc::TrySendError;
impl GameRuntime {
    pub(in crate::game_runtime) fn npc_region_outcome(
        &mut self,
        token: u64,
        result: Result<(), bace_simulation::GeneratorServiceError>,
    ) -> bool {
        let Some((source, _)) = self.npc.regions.remove(&token) else {
            return false;
        };
        if let Err(error) = result {
            self.npc.failure = Some(format!("NPC destination region {}: {error:?}", source.0));
        }
        true
    }
    pub(super) fn ensure_npc_region(
        &mut self,
        source: EntityId,
        cell: u32,
    ) -> Result<bool, String> {
        if cell == 0 {
            return Err("NPC destination cell missing".into());
        }
        let block = (cell >> 16) as u16;
        if self
            .world
            .as_ref()
            .is_some_and(|w| w.regions.prepared_region(block).is_some())
        {
            return Ok(true);
        }
        if self.npc.regions.values().any(|(actor, _)| *actor == source) {
            return Ok(false);
        }
        if self.npc.regions.len() >= 4096 {
            return Err("NPC destination request capacity".into());
        }
        let token = self.token()?;
        match self
            .simulation
            .input()
            .try_submit(bace_simulation::Command::Generator(
                bace_simulation::GeneratorCommand {
                    correlation: token,
                    action: bace_simulation::GeneratorAction::RequestRegion {
                        landblock: block,
                        permanent: false,
                    },
                },
            )) {
            Ok(()) => {
                self.npc.regions.insert(token, (source, block));
            }
            Err(TrySendError::Full(_)) => (),
            Err(TrySendError::Disconnected(_)) => return Err("NPC destination owner closed".into()),
        }
        Ok(false)
    }
}
