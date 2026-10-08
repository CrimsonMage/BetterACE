//! Explicit-clock ACE status staging, regeneration and lifecycle directives.
use crate::generator::{GeneratorError, GeneratorMachine, delay_tick};
use bace_gameplay_api::*;
use bace_random::RandomRoot;
use bace_types::EntityId;
impl GeneratorMachine {
    pub fn advance(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        if clock.tick < self.next_update && self.next_regeneration.is_none_or(|t| clock.tick < t) {
            self.last_tick = clock.tick;
            return Ok(GeneratorTransition::default());
        }
        let mut draft = self.clone();
        let mut out = GeneratorTransition::default();
        if clock.tick >= draft.next_update {
            draft.last_tick = clock.tick;
            draft.status(clock, root, &mut out)?;
            if !draft.entered {
                draft.status(clock, root, &mut out)?;
                if !draft.disabled {
                    draft.start(clock, root, &mut out)?;
                }
                draft.entered = true;
            }
            draft.next_update = clock.tick.checked_add(150).ok_or(GeneratorError::Time)?;
        }
        if draft.next_regeneration.is_some_and(|t| clock.tick >= t) {
            draft.generate(clock, root, &mut out)?;
            draft.has_regenerated = true;
            draft.next_regeneration = if draft.definition.regeneration_interval > 0.0 {
                Some(delay_tick(
                    clock.tick,
                    draft.definition.regeneration_interval,
                )?)
            } else {
                None
            };
        }
        draft.last_tick = clock.tick;
        *self = draft;
        Ok(out)
    }
    pub fn activate(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        let mut draft = self.clone();
        let mut out = GeneratorTransition::default();
        draft.generate(clock, root, &mut out)?;
        draft.last_tick = clock.tick;
        *self = draft;
        Ok(out)
    }
    fn start(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
        out: &mut GeneratorTransition,
    ) -> Result<(), GeneratorError> {
        if self.powering {
            return Ok(());
        }
        self.powering = true;
        if self.definition.initial_delay > 0.0 {
            self.next_regeneration = Some(if !self.has_regenerated {
                clock.tick
            } else {
                delay_tick(clock.tick, self.definition.initial_delay)?
            });
        } else {
            if matches!(
                self.definition.kind,
                GeneratorKind::Container | GeneratorKind::Chest | GeneratorKind::Vendor
            ) || (self
                .definition
                .event
                .as_ref()
                .is_some_and(|s| !s.is_empty())
                && self.definition.regeneration_interval == 0.0)
            {
                self.generate(clock, root, out)?;
            }
            if self.definition.initial_count == 0 {
                self.powering = false;
            }
        }
        Ok(())
    }
    fn status(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
        out: &mut GeneratorTransition,
    ) -> Result<(), GeneratorError> {
        let disabled = match self.definition.time_type {
            GeneratorTimeType::Undefined | GeneratorTimeType::Defined => return Ok(()),
            GeneratorTimeType::RealTime => {
                (self.definition.start_time > 0
                    && clock.unix_seconds < i64::from(self.definition.start_time))
                    || (self.definition.end_time > 0
                        && clock.unix_seconds > i64::from(self.definition.end_time))
            }
            GeneratorTimeType::Night => clock.is_day,
            GeneratorTimeType::Day => !clock.is_day,
            GeneratorTimeType::Event => {
                if self.definition.event.as_ref().is_none_or(|s| s.is_empty()) {
                    return Ok(());
                }
                match clock.event {
                    GeneratorEventState::Missing => return Ok(()),
                    GeneratorEventState::Available { enabled, started } => !enabled || !started,
                }
            }
        };
        if self.staged {
            self.disabled = disabled;
            if disabled {
                out.effects.extend(self.cleanup(
                    self.definition.end_destruction,
                    false,
                    false,
                    false,
                ));
            } else {
                self.start(clock, root, out)?;
            }
            self.staged = false;
        } else if self.disabled != disabled {
            self.staged = true;
        }
        Ok(())
    }
    pub fn notify(
        &mut self,
        entity: EntityId,
        event: GeneratorNotification,
        clock: GeneratorClock,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        let mut out = GeneratorTransition::default();
        if let Some(i) = self
            .profiles
            .iter()
            .position(|p| p.members.contains_key(&entity))
        {
            let member_identity = self
                .member_identities
                .get(&entity)
                .copied()
                .unwrap_or(self.definition.identity);
            let stored = self.profiles[i].profile.when_create;
            let mut observed = match event {
                GeneratorNotification::Destruction => 1,
                GeneratorNotification::PickUp => 2,
                GeneratorNotification::Death => 4,
            };
            if (stored == 1 && observed == 2) || (stored == 2 && observed == 1) {
                observed = stored;
            }
            let expected = if stored == 0 && (observed == 1 || observed == 2) {
                observed
            } else {
                stored
            };
            if expected == observed {
                let delay = if self.definition.kind == GeneratorKind::Chest {
                    0.0
                } else {
                    self.profiles[i]
                        .profile
                        .delay
                        .or_else(|| self.profiles.first().and_then(|p| p.profile.delay))
                        .unwrap_or(0.0)
                };
                let after = delay_tick(clock.tick, f64::from(delay))?;
                self.profiles[i].members.remove(&entity);
                self.member_identities.remove(&entity);
                self.profiles[i].available_after = Some(after);
            }
            let container = matches!(
                self.definition.kind,
                GeneratorKind::Container
                    | GeneratorKind::Creature
                    | GeneratorKind::Chest
                    | GeneratorKind::Vendor
            );
            if self.definition.initial_count > 0
                && self.current_create() == 0
                && (self.definition.automatic_destruction
                    || (self.definition.parent.is_some() && !container))
            {
                out.effects.push(GeneratorLifecycleEffect::DestroySelf(
                    self.definition.identity,
                ));
            }
            out.effects.push(GeneratorLifecycleEffect::DetachMember {
                generator: member_identity,
                entity,
            });
        }
        self.last_tick = clock.tick;
        Ok(out)
    }
    fn cleanup(
        &mut self,
        directive: GeneratorDestruction,
        recursive: bool,
        include_dead: bool,
        from_unload: bool,
    ) -> Vec<GeneratorLifecycleEffect> {
        if directive == GeneratorDestruction::Nothing {
            return vec![];
        }
        let mut effects = Vec::new();
        for p in &mut self.profiles {
            for (&entity, &contribution) in &p.members {
                let identity = self
                    .member_identities
                    .get(&entity)
                    .copied()
                    .unwrap_or(self.definition.identity);
                let member = GeneratorSpawnMember {
                    entity,
                    contribution,
                };
                effects.push(match directive {
                    GeneratorDestruction::Kill => GeneratorLifecycleEffect::KillMember {
                        generator: identity,
                        member,
                    },
                    _ => GeneratorLifecycleEffect::DestroyMember {
                        generator: identity,
                        member,
                        recursive,
                        include_dead,
                        from_unload,
                    },
                });
            }
            p.members.clear();
            p.queued = 0;
            p.suppressed = 0;
            p.treasure = false;
            p.available_after = Some(self.last_tick);
        }
        self.queue.clear();
        self.member_identities.clear();
        effects
    }
    pub fn reset(&mut self, clock: GeneratorClock) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        self.last_tick = clock.tick;
        let effects = self.cleanup(GeneratorDestruction::Destroy, true, true, false);
        if self.definition.kind == GeneratorKind::Chest {
            self.powering = true;
        }
        Ok(GeneratorTransition {
            effects,
            ..Default::default()
        })
    }
    /// A staff regeneration is one draft transition. A failed selection cannot
    /// publish reset cleanup or discard the accepted generator membership.
    pub fn regenerate(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        let mut draft = self.clone();
        let mut transition = draft.reset(clock)?;
        draft.entered = false;
        for profile in &mut draft.profiles {
            profile.available_after = None;
        }
        draft.generate(clock, root, &mut transition)?;
        draft.last_tick = clock.tick;
        *self = draft;
        Ok(transition)
    }
    pub fn death(&mut self, clock: GeneratorClock) -> Result<GeneratorTransition, GeneratorError> {
        self.directive(clock, false)
    }
    pub fn destroy(
        &mut self,
        clock: GeneratorClock,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.directive(clock, false)
    }
    pub fn unload(&mut self, clock: GeneratorClock) -> Result<GeneratorTransition, GeneratorError> {
        self.directive(clock, true)
    }
    fn directive(
        &mut self,
        clock: GeneratorClock,
        unload: bool,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        self.last_tick = clock.tick;
        let effects = self.cleanup(self.definition.destruction, false, false, unload);
        Ok(GeneratorTransition {
            effects,
            ..Default::default()
        })
    }
}
