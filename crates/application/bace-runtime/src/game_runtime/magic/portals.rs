//! Destination regions are admitted before submitting a prepared player program.
//! Portal transaction completion remains exclusively with the portal owner.
use super::*;
impl GameRuntime {
    pub(in crate::game_runtime) fn magic_region_outcome(
        &mut self,
        token: u64,
        result: Result<(), bace_simulation::GeneratorServiceError>,
    ) -> bool {
        let Some(key) = self.magic.regions.remove(&token) else {
            return false;
        };
        if let Err(error) = result {
            self.magic
                .failures
                .insert(key, format!("spell destination region: {error:?}"));
            if let Some(p) = self.magic.pending.get_mut(&key) {
                p.phase = Phase::Terminal(CastOutcome {
                    context: p.context,
                    result: Err(CastRejection::MissingAssets),
                });
            }
        }
        true
    }
    pub(super) fn prepare_magic_regions(&mut self, key: SessionKey) -> Result<bool, String> {
        let Some(p) = self.magic.pending.get(&key) else {
            return Ok(false);
        };
        let Phase::Program(Some(program)) = &p.phase else {
            return Ok(true);
        };
        if self.magic.regions.values().any(|owner| *owner == key) {
            return Ok(false);
        }
        let Some(world) = self.world.as_ref() else {
            return Ok(program.destination_cells.is_empty());
        };
        let missing = program
            .destination_cells
            .iter()
            .map(|cell| (cell.0 >> 16) as u16)
            .find(|block| world.regions.prepared_region(*block).is_none());
        let Some(block) = missing else {
            return Ok(true);
        };
        let token = self.token()?;
        match self.simulation.input().try_submit(Command::Generator(
            bace_simulation::GeneratorCommand {
                correlation: token,
                action: bace_simulation::GeneratorAction::RequestRegion {
                    landblock: block,
                    permanent: false,
                },
            },
        )) {
            Ok(()) => {
                self.request_regions.insert(token, (key, block));
                self.magic.regions.insert(token, key);
            }
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                return Err("spell destination owner closed".into());
            }
        }
        Ok(false)
    }
}

impl GameRuntime {
    /// Only the exact portal transaction owner calls this after adopting its
    /// terminal completion. Public portal observations do not prove completion.
    pub(in crate::game_runtime) fn complete_magic_portal(
        &mut self,
        actor: bace_types::EntityId,
        cast: u64,
    ) {
        self.magic.portals.retain(|event| !matches!(event, bace_simulation::MagicEvent::PortalRequired { actor: owner, cast: operation, .. } if *owner == actor && *operation == cast));
    }
}
