//! Preserve an uncommitted release while the accepted health-output lane is full.
use super::*;
impl Magic {
    pub(super) fn health_output_ready(
        &self,
        attempt: &Attempt,
        world: &World,
        fellowships: &crate::fellowships::Fellowships,
    ) -> bool {
        let actor = attempt.origin.actor();
        let target = attempt.target.unwrap_or(actor);
        let mut targets = [None, None];
        match attempt.prepared.spell.effect {
            SpellEffect::Boost {
                vital: Vital::Health,
                ..
            } => targets[0] = Some(target),
            SpellEffect::Transfer {
                source,
                destination,
                source_is_caster,
                destination_is_caster,
                ..
            } => {
                if source == Vital::Health {
                    targets[0] = Some(if source_is_caster { actor } else { target });
                }
                if destination == Vital::Health {
                    targets[1] = Some(if destination_is_caster { actor } else { target });
                }
                if targets[0] == targets[1] {
                    targets[1] = None;
                }
            }
            SpellEffect::LifeProjectile {
                source: Vital::Health,
                ..
            } => targets[0] = Some(actor),
            SpellEffect::FellowshipBoost {
                vital: Vital::Health,
                ..
            } => {
                return fellowships.roster(target).is_none_or(|members| {
                    world
                        .validate_health_observations(members.iter().copied())
                        .is_ok()
                });
            }
            _ => {}
        }
        world
            .validate_health_observations(targets.into_iter().flatten())
            .is_ok()
    }
}
