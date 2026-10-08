//! Portal effect release waits for a typed owning-service transaction; booleans
//! from adapters never establish that a link, summon or teleport happened.
use super::*;
#[derive(Clone)]
pub(crate) struct PendingPortalCast {
    pub actor: EntityId,
    pub epoch: u16,
    pub cast: u64,
    pub target: EntityId,
    pub effect: bace_magic::PortalEffect,
    pub fellowship: bool,
    pub mana_cost: u32,
}
impl Magic {
    pub(crate) fn pending_portal_casts(&self) -> Vec<PendingPortalCast> {
        self.attempts
            .values()
            .chain(self.instant_continuations.values())
            .filter(|a| a.portal_request_sent && a.portal_operation.is_none())
            .filter_map(|a| {
                let (effect, fellowship) = match &a.prepared.spell.effect {
                    SpellEffect::Portal(effect) => (effect.clone(), false),
                    SpellEffect::FellowshipPortal(effect) => (effect.clone(), true),
                    _ => return None,
                };
                Some(PendingPortalCast {
                    actor: a.origin.actor(),
                    epoch: a.origin_epoch,
                    cast: a.cast,
                    target: a.target.unwrap_or(a.origin.actor()),
                    effect,
                    fellowship,
                    mana_cost: if a.mana_applied {
                        0
                    } else {
                        a.resources.as_ref()?.cost
                    },
                })
            })
            .take(32)
            .collect()
    }
    pub(crate) fn portal_origin(&self, actor: EntityId, cast: u64) -> Option<CastOrigin> {
        self.attempts
            .get(&actor)
            .filter(|a| a.cast == cast)
            .or_else(|| {
                self.instant_continuations
                    .get(&cast)
                    .filter(|a| a.origin.actor() == actor)
            })
            .map(|a| a.origin)
    }
    pub(crate) fn bind_portal(
        &mut self,
        actor: EntityId,
        cast: u64,
        operation: u64,
    ) -> Result<(), CastRejection> {
        let attempt = self
            .attempts
            .get_mut(&actor)
            .filter(|a| a.cast == cast)
            .or_else(|| {
                self.instant_continuations
                    .get_mut(&cast)
                    .filter(|a| a.origin.actor() == actor)
            })
            .filter(|a| a.portal_request_sent && a.portal_operation.is_none())
            .ok_or(CastRejection::InvalidState)?;
        attempt.portal_operation = Some(operation);
        Ok(())
    }
    pub(crate) fn portal_finish_ready(&self, actor: EntityId, cast: u64) -> bool {
        self.can_accept()
            && self
                .attempts
                .get(&actor)
                .filter(|a| a.cast == cast)
                .or_else(|| {
                    self.instant_continuations
                        .get(&cast)
                        .filter(|a| a.origin.actor() == actor)
                })
                .is_some_and(|a| a.portal_request_sent)
            && self.recovery.get(&actor).is_some_and(|r| r.can_mutate())
    }
    pub(crate) fn finish_portal_service(
        &mut self,
        actor: EntityId,
        cast: u64,
        result: Result<(), CastRejection>,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        if !self.portal_finish_ready(actor, cast) {
            return Err(CastRejection::Capacity);
        }
        let independent = self
            .instant_continuations
            .get(&cast)
            .is_some_and(|a| a.origin.actor() == actor);
        let mut attempt = if independent {
            self.instant_continuations.remove(&cast)
        } else {
            self.attempts.remove(&actor)
        }
        .ok_or(CastRejection::InvalidState)?;
        // Release is one irreversible driver transition. Keep its accepted result
        // if recording recovery fails, so the exact owner acknowledgment can retry.
        let completion = (|| {
            if let Some(previous) = attempt.portal_completion {
                if previous != result {
                    return Err(CastRejection::InvalidState);
                }
            } else {
                attempt
                    .driver
                    .resolve_release(cast, now, result.map_err(|_| CastError::InvalidTarget))
                    .map_err(rejection)?;
                attempt.portal_completion = Some(result);
            }
            if independent {
                self.publish_outcome(
                    attempt.origin,
                    result.map(|()| CastChange::Completed { cast }),
                );
                Ok(())
            } else {
                self.finish_attempt(
                    &mut attempt,
                    result.map(|()| CastChange::Completed { cast }),
                    world,
                    now,
                )
            }
        })();
        if completion.is_err() || attempt.terminal.is_some() {
            if independent {
                self.instant_continuations.insert(cast, attempt);
            } else {
                self.attempts.insert(actor, attempt);
            }
        }
        completion?;
        Ok(())
    }
}

impl Magic {
    pub(crate) fn apply_portal_mana(
        &mut self,
        world: &mut World,
        mutation: VitalMutation,
        operation: u64,
    ) -> Result<(), CastRejection> {
        if self.events.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        if mutation.vital != EntityVital::Mana || mutation.after > mutation.before {
            return Err(CastRejection::InvalidState);
        }
        let result = world
            .apply_vital_batch_reserved(
                &[mutation],
                None,
                bace_world::VitalReservationToken {
                    domain: bace_world::VitalReservationDomain::Portal,
                    operation,
                },
            )
            .map_err(|_| CastRejection::InvalidState)?;
        if mutation.before != mutation.after {
            let result = &result[0];
            self.events.push_back(MagicEvent::Vital {
                incarnation: world
                    .combatant(mutation.actor)
                    .map_or(0, |c| c.incarnation()),
                actor: mutation.actor,
                vital: mutation.vital,
                before: mutation.before,
                after: mutation.after,
                revision: result.revision,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
