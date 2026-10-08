//! Atomic registry projection for death; does not duplicate the live registry.
use super::*;
impl Magic {
    pub(crate) fn registry_time(&self) -> f64 {
        self.current_time
    }

    pub(crate) fn prepare_death_registry(
        &self,
        actor: EntityId,
        vitae: Option<f32>,
        kind: bace_interactions::PlayerDeathKind,
        retain: bool,
    ) -> Result<EnchantmentRegistry, CastRejection> {
        let before = self
            .registries
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        let mut after = EnchantmentRegistry::restore(
            before.capacity(),
            before.revision(),
            before.entries().to_vec(),
        )
        .map_err(|_| CastRejection::InvalidState)?;
        if let Some(value) = vitae {
            let patch = after
                .propose_vitae(actor.0, self.vitae_template.as_ref(), Some(value), None)
                .map_err(|_| CastRejection::InvalidState)?;
            after
                .adopt_vitae(patch)
                .map_err(|_| CastRejection::InvalidState)?;
        }
        let remove: Vec<_> = after
            .entries()
            .iter()
            .filter(|e| {
                !bace_interactions::keep_death_enchantment(
                    e.spec.duration,
                    e.spell,
                    e.spec.beneficial,
                    kind,
                    retain,
                )
            })
            .map(|e| (e.spell, e.spec.layer))
            .collect();
        if !remove.is_empty() {
            after
                .remove(&remove)
                .map_err(|_| CastRejection::InvalidState)?;
        }
        Ok(after)
    }
    /// The committed death operation publishes its own ordered vitae/purge
    /// transcript. Adopt the exact registry before the death animation without
    /// also queuing ordinary magic notifications after that transcript.
    pub(crate) fn adopt_death_registry_silent(
        &mut self,
        actor: EntityId,
        before: u64,
        after: EnchantmentRegistry,
    ) -> Result<(), CastRejection> {
        self.validate_death_registry_owner(actor, before)?;
        self.registries.insert(actor, after);
        self.vitae_removals.remove(&actor);
        Ok(())
    }

    pub(crate) fn validate_death_registry_silent(
        &self,
        actor: EntityId,
        before: u64,
    ) -> Result<(), CastRejection> {
        self.validate_death_registry_owner(actor, before)
    }
}

impl Magic {
    fn validate_death_registry_owner(
        &self,
        actor: EntityId,
        before: u64,
    ) -> Result<(), CastRejection> {
        let current = self
            .registries
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if !self.registry_reserved(actor) || current.revision() != before {
            return Err(CastRejection::Busy);
        }
        Ok(())
    }
}

impl Magic {
    /// ACE Player.Die calls FailCast(false): terminate without a fizzle, retaining
    /// paid/valuable continuations until their exact acknowledgement can drain.
    pub(crate) fn cancel_for_player_death(
        &mut self,
        actor: EntityId,
        world: &mut World,
    ) -> Result<(), CastRejection> {
        if world
            .combatant(actor)
            .is_none_or(|c| !c.profile().player || c.health() != 0)
        {
            return Err(CastRejection::InvalidState);
        }
        let Some(attempt) = self.attempts.get(&actor) else {
            return Ok(());
        };
        if !self.can_accept() {
            return Err(CastRejection::Capacity);
        }
        if (attempt.component_request_sent && !attempt.components_confirmed)
            || attempt.portal_operation.is_some()
            || attempt.pending_direct.is_some()
            || attempt.terminal.is_some()
            || attempt.peace_fizzle.is_some()
        {
            return Err(CastRejection::Busy);
        }
        let cast = attempt.cast;
        if let Some(token) = world.source_motion_token(actor)
            && token.domain == bace_motion::MotionDomain::Casting
            && token.owner == cast
        {
            world
                .cancel_motion(actor, token)
                .map_err(|_| CastRejection::Capacity)?;
        }
        let mut attempt = self.attempts.remove(&actor).expect("checked dead cast");
        if let Err(error) = self.retain_recovery(&attempt, None, self.current_time) {
            self.attempts.insert(actor, attempt);
            return Err(error);
        }
        attempt.driver.cancel();
        self.publish_outcome(attempt.origin, Ok(CastChange::Cancelled { cast }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_death_registry_adopts_without_replaying_magic_events() {
        let actor = EntityId(1);
        let mut magic = Magic::new(1);
        magic
            .register_registry(actor, EnchantmentRegistry::new(8).unwrap(), true, 0.0)
            .unwrap();
        magic.reserve_registry(actor, true, 0.0).unwrap();
        let vitae = EnchantmentEntry {
            spell: 666,
            caster: actor.0,
            school: bace_magic::MagicSchool::Life,
            spec: bace_magic::EnchantmentSpec {
                category: 204,
                power: 0,
                duration: -1.0,
                layer: 1,
                stat_type: 0,
                stat_key: 0,
                value: 0.95,
                beneficial: false,
                set_id: None,
            },
            start_time: 0.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata::default(),
        };
        let after = EnchantmentRegistry::restore(8, 1, vec![vitae]).unwrap();
        magic.events.push_back(MagicEvent::ProjectileRemoved {
            tick: 0,
            actor: EntityId(2),
        });
        assert_eq!(magic.validate_death_registry_silent(actor, 0), Ok(()));
        magic.adopt_death_registry_silent(actor, 0, after).unwrap();
        assert_eq!(magic.registry(actor).unwrap().revision(), 1);
        assert_eq!(magic.registry(actor).unwrap().entries()[0].spell, 666);
        assert_eq!(magic.pending_events(), 1);
        assert!(matches!(
            magic.take_event(),
            Some(MagicEvent::ProjectileRemoved { .. })
        ));
        assert!(magic.take_event().is_none());
        assert!(magic.registry_reserved(actor));
    }
}
