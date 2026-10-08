//! Correlated trusted death inputs preserve every rejected prepared owner.
use crate::{Kernel, PlayerDeathCommand, PlayerDeathError};

pub struct PlayerDeathServiceCommand {
    pub correlation: u64,
    pub command: PlayerDeathCommand,
}
impl PlayerDeathServiceCommand {
    pub fn valid_bounds(&self) -> bool {
        self.correlation != 0 && self.command.valid_bounds()
    }
}
pub struct PlayerDeathServiceOutcome {
    pub correlation: u64,
    /// Failure returns the exact command, including its sole prepared corpse body.
    pub result: Result<(), (PlayerDeathError, Box<PlayerDeathCommand>)>,
}
impl Kernel {
    pub fn apply_player_death_service(
        &mut self,
        request: PlayerDeathServiceCommand,
    ) -> PlayerDeathServiceOutcome {
        let result = if request.valid_bounds() {
            self.apply_player_death_command(request.command)
                .map_err(|(error, command)| (error, Box::new(command)))
        } else {
            Err((PlayerDeathError::Invalid, Box::new(request.command)))
        };
        PlayerDeathServiceOutcome {
            correlation: request.correlation,
            result,
        }
    }
}
