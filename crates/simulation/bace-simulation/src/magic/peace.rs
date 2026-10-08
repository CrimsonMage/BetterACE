//! GDLE Update peace-style fizzle. Resource/output admission is atomic; no normal
//! component burn, spell effect, school-success timestamp or streak charge runs.
use super::*;
pub(super) fn accepted_peace_style(world: &World, actor: EntityId) -> bool {
    world.source_motion_state(actor).map_or_else(
        || {
            world
                .body(actor)
                .is_ok_and(|b| b.collision_shape().is_none())
                && world.combatant(actor).is_some_and(|c| c.mode() == 1)
        },
        |state| state.style == 0x8000003d,
    )
}
impl Magic {
    pub(super) fn flush_peace_fizzle(
        &mut self,
        attempt: &mut Attempt,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        let Some((cost, intensity, movement)) = attempt.peace_fizzle else {
            return Ok(());
        };
        if self.events.len() + 2 > self.capacity
            || !self.can_accept()
            || world.vital_reserved(attempt.origin.actor(), EntityVital::Mana)
        {
            return Ok(());
        }
        let actor = attempt.origin.actor();
        let before = world
            .vital(actor, EntityVital::Mana)
            .map_err(|_| CastRejection::MissingActor)?
            .current;
        let change = VitalMutation {
            actor,
            vital: EntityVital::Mana,
            before,
            after: before.saturating_sub(cost),
        };
        let results = world
            .apply_vital_batch(&[change], None)
            .map_err(|_| CastRejection::InvalidState)?;
        self.events.push_back(MagicEvent::Fizzle {
            actor,
            intensity,
            movement_incarnation: movement
                .then(|| world.combatant(actor).map_or(0, |c| c.incarnation())),
        });
        if change.before != change.after {
            self.events.push_back(MagicEvent::Vital {
                incarnation: world.combatant(actor).map_or(0, |c| c.incarnation()),
                actor,
                vital: EntityVital::Mana,
                before,
                after: change.after,
                revision: results[0].revision,
            });
        }
        attempt.peace_fizzle = None;
        // LaunchSpellEffect(TRUE) returns WERROR_NONE before any target/effect
        // checks. This successful UseDone does not establish a successful school cast.
        self.finish_fizzled_attempt(attempt, world, now)
    }
}
