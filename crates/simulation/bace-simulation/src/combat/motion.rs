//! Physical callbacks are emitted only by the World's authoritative DAT cursor.
use super::*;
use bace_motion::{
    MotionDomain, MotionExecutionEvent, MotionHookPayload, MotionToken, PreparedMotionChain,
};
use std::sync::Arc;
pub(super) type PhysicalMotionKey = (EntityId, u32, u32, u32, u32, u32);
pub(super) fn key(actor: EntityId, chain: &PreparedMotionChain) -> Option<PhysicalMotionKey> {
    let before = chain.source_transition()?.before;
    Some((
        actor,
        chain.motion,
        chain.speed.to_bits(),
        before.style,
        before.substate,
        before.speed.to_bits(),
    ))
}

pub(crate) struct PhysicalMotion {
    pub token: MotionToken,
    pub epoch: u16,
    pub hooks: VecDeque<u32>,
    pub complete: Option<bool>,
}
impl Combat {
    pub(crate) fn validate_physical_motions(
        &self,
        actor: EntityId,
        registrations: &[(u32, f32, Arc<PreparedMotionChain>)],
    ) -> Result<(), CombatRejection> {
        if registrations.is_empty()
            || registrations.len() > 128
            || self
                .physical_motions
                .keys()
                .filter(|key| key.0 == actor)
                .count()
                .saturating_add(registrations.len())
                > 128
            || self
                .physical_motions
                .len()
                .saturating_add(registrations.len())
                > self.capacity.saturating_mul(128)
        {
            return Err(CombatRejection::Capacity);
        }
        for (index, (motion, speed, chain)) in registrations.iter().enumerate() {
            let k = key(actor, chain).ok_or(CombatRejection::InvalidRequest)?;
            if chain.motion != *motion
                || chain.speed != *speed
                || !speed.is_finite()
                || *speed <= 0.0
                || chain.stop_chain().is_none()
                || chain.nominal_duration_seconds() > 150.0
                || registrations[..index]
                    .iter()
                    .any(|(_, _, c)| key(actor, c) == Some(k))
                || self.physical_motions.contains_key(&k)
            {
                return Err(CombatRejection::InvalidRequest);
            }
        }
        Ok(())
    }
    pub(crate) fn register_physical_motion(
        &mut self,
        actor: EntityId,
        motion: u32,
        speed: f32,
        chain: Arc<PreparedMotionChain>,
    ) -> Result<(), CombatRejection> {
        if !self.physical.contains_key(&actor)
            || chain.motion != motion
            || chain.speed != speed
            || !speed.is_finite()
            || speed <= 0.0
            || chain.stop_chain().is_none()
            || chain.nominal_duration_seconds() > 150.0
        {
            return Err(CombatRejection::InvalidRequest);
        }
        if self.active(actor) {
            return Err(CombatRejection::Busy);
        }
        let key = key(actor, &chain).ok_or(CombatRejection::InvalidRequest)?;
        if !self.physical_motions.contains_key(&key)
            && (self.physical_motions.len() >= self.capacity.saturating_mul(128)
                || self
                    .physical_motions
                    .keys()
                    .filter(|key| key.0 == actor)
                    .count()
                    >= 128)
        {
            return Err(CombatRejection::Capacity);
        }
        self.physical_motions.insert(key, chain);
        self.enable_physical_driver(actor);
        Ok(())
    }
    pub(super) fn physical_chain(
        &self,
        world: &World,
        actor: EntityId,
        motion: u32,
        speed: f32,
    ) -> Option<Arc<PreparedMotionChain>> {
        let before = world
            .source_motion_state(actor)
            .unwrap_or(bace_motion::SourceMotionState {
                style: self.physical.get(&actor)?.style,
                substate: 0x41000003,
                speed: 1.0,
            });
        self.physical_motions
            .get(&(
                actor,
                motion,
                speed.to_bits(),
                before.style,
                before.substate,
                before.speed.to_bits(),
            ))
            .cloned()
            .or_else(|| {
                self.physical_motions.iter().find_map(|(key, chain)| {
                    (key.0 == actor
                        && key.1 == motion
                        && key.3 == before.style
                        && key.4 == before.substate
                        && f32::from_bits(key.5).is_sign_negative()
                            == before.speed.is_sign_negative())
                    .then(|| {
                        chain
                            .retime_current_action(before.speed, speed)
                            .ok()
                            .map(Arc::new)
                    })
                    .flatten()
                })
            })
    }
    pub(crate) fn physical_style_chain(
        &self,
        world: &World,
        actor: EntityId,
        style: u32,
    ) -> Option<Arc<PreparedMotionChain>> {
        let action = if world
            .source_motion_state(actor)
            .is_some_and(|s| s.style == style)
        {
            0x41000003
        } else {
            style
        };
        self.physical_chain(world, actor, action, 1.0)
    }
    pub(super) fn begin_physical_motion(
        &mut self,
        world: &mut World,
        actor: EntityId,
        operation: u64,
        motion: u32,
        speed: f32,
    ) -> Result<Option<PhysicalMotion>, CombatRejection> {
        self.begin_physical_motion_sequence(
            world,
            actor,
            motion,
            speed,
            MotionToken {
                domain: MotionDomain::Physical,
                owner: operation,
                sequence: 1,
            },
        )
    }
    pub(super) fn begin_physical_motion_sequence(
        &mut self,
        world: &mut World,
        actor: EntityId,
        motion: u32,
        speed: f32,
        token: MotionToken,
    ) -> Result<Option<PhysicalMotion>, CombatRejection> {
        let Some(chain) = self.physical_chain(world, actor, motion, speed) else {
            // Profiles with no DAT registration are the existing explicit synthetic
            // harness. Once any chain is registered, missing variants fail closed.
            return if self.physical_drivers.contains_key(&actor)
                || world
                    .body(actor)
                    .is_ok_and(|b| b.collision_shape().is_some())
            {
                Err(CombatRejection::MissingCombatProfile)
            } else {
                Ok(None)
            };
        };
        let cache_key = key(actor, &chain).ok_or(CombatRejection::InvalidRequest)?;
        if !self.physical_motions.contains_key(&cache_key) {
            // One positive-rate family per source state; successive Quickness
            // updates replace derived rates instead of accumulating variants.
            self.physical_motions.retain(|k, _| {
                !(k.0 == cache_key.0
                    && k.1 == cache_key.1
                    && k.3 == cache_key.3
                    && k.4 == cache_key.4
                    && k.5 == cache_key.5)
            });
            self.physical_motions.insert(cache_key, chain.clone());
        }
        let epoch = world
            .actor_state(actor)
            .map_err(|_| CombatRejection::MissingActor)?
            .1
            .epoch();
        world
            .begin_motion(actor, token, chain)
            .map_err(|_| CombatRejection::Busy)?;
        Ok(Some(PhysicalMotion {
            token,
            epoch,
            hooks: VecDeque::with_capacity(32),
            complete: None,
        }))
    }
    pub(super) fn drain_physical_motion(&mut self, world: &mut World) {
        for _ in 0..self.capacity {
            let Some(event) = self
                .physical_motion_pending
                .take()
                .or_else(|| world.take_motion_event_for(MotionDomain::Physical))
            else {
                break;
            };
            if !world
                .actor_state(event.actor)
                .is_ok_and(|(_, s)| s.epoch() == event.epoch)
            {
                continue;
            }
            let motion = self
                .physical_attacks
                .get_mut(&event.actor)
                .and_then(|a| a.motion.as_mut())
                .filter(|m| m.token == event.token)
                .or_else(|| {
                    self.missiles
                        .get_mut(&event.token.owner)
                        .filter(|a| a.proposal.actor == event.actor.0)
                        .and_then(|a| a.motion.as_mut())
                        .filter(|m| m.token == event.token)
                })
                .or_else(|| {
                    self.physical_drivers.get_mut(&event.actor).and_then(|d| {
                        d.reload
                            .as_mut()
                            .filter(|m| m.token == event.token)
                            .or_else(|| d.transition.as_mut().filter(|m| m.token == event.token))
                    })
                });
            let Some(motion) = motion.filter(|m| m.token == event.token && m.epoch == event.epoch)
            else {
                continue;
            };
            match event.event {
                MotionExecutionEvent::Hook {
                    payload: MotionHookPayload::Attack { part, .. },
                    ..
                } => {
                    if motion.hooks.len() == 32 {
                        self.physical_motion_pending = Some(event);
                        break;
                    }
                    motion.hooks.push_back(part);
                }
                MotionExecutionEvent::Completed => motion.complete = Some(true),
                MotionExecutionEvent::Cancelled => motion.complete = Some(false),
                MotionExecutionEvent::Hook { .. } => {
                    if self.physical_events.len() == self.capacity.max(2) {
                        self.physical_motion_pending = Some(event);
                        break;
                    }
                    self.physical_events
                        .push_back(PhysicalCombatEvent::MotionHook {
                            actor: event.actor,
                            event: event.event,
                        });
                }
            }
        }
    }
}
