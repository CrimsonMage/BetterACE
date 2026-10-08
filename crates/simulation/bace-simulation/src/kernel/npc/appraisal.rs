//! NPC appraisal wake owner. Live Identify must still supply the complete
//! source-filtered profile and retained private/emote output before adoption.
use super::*;

impl Kernel {
    pub fn preview_npc_appraisal_wake(
        &self,
        context: ActionContext,
        creature: EntityId,
    ) -> Result<Option<crate::AppraisalWake>, PveError> {
        // Read-only preparation does not consume ActionContext.sequence. The
        // single authoritative authorization occurs during adoption.
        self.population
            .preview_appraisal_wake(creature, context.actor, &self.world)
    }

    pub fn adopt_npc_appraisal_wake(
        &mut self,
        context: ActionContext,
        expected: crate::AppraisalWake,
    ) -> Result<(), PveError> {
        if expected.examiner != context.actor {
            return Err(PveError::InvalidReceipt);
        }
        self.characters
            .authorize(context, self.world.body(context.actor).is_ok())
            .map_err(|_| PveError::MissingActor)?;
        self.population.adopt_appraisal_wake(expected, &self.world)
    }
}
