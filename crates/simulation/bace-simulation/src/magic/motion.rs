//! Cast completion comes only from the same World's GDLE sequence owner.
use super::*;
use bace_motion::{MotionDomain, MotionExecutionEvent};
impl Magic {
    pub(crate) fn enable_synthetic_cast_timing(&mut self) {
        self.synthetic_cast_timing = true;
    }
    pub(super) fn drain_motion_events(&mut self, world: &mut World) {
        self.drain_motion_events_for(world, None);
    }
    pub(super) fn drain_motion_events_for(
        &mut self,
        world: &mut World,
        mut current: Option<&mut Attempt>,
    ) {
        for _ in 0..4096 {
            if self.events.len() >= self.capacity {
                break;
            }
            let Some(event) = world.take_motion_event_for(MotionDomain::Casting) else {
                break;
            };
            // EndCast may already have published its result while the source
            // action and Ready suffix still execute. Preserve their trusted
            // hooks; they are projections, never permission to release a spell.
            if let MotionExecutionEvent::Hook {
                animation,
                frame,
                kind,
                payload,
            } = event.event
            {
                if world
                    .actor_state(event.actor)
                    .is_ok_and(|(_, state)| state.epoch() == event.epoch)
                {
                    self.events.push_back(MagicEvent::MotionHook {
                        actor: event.actor,
                        cast: event.token.owner,
                        sequence: event.token.sequence,
                        animation,
                        frame,
                        kind,
                        payload,
                    });
                }
                continue;
            }
            let local_matches = current
                .as_ref()
                .is_some_and(|a| a.origin.actor() == event.actor && a.cast == event.token.owner);
            let attempt = if local_matches {
                current.as_deref_mut()
            } else {
                self.attempts
                    .get_mut(&event.actor)
                    .filter(|a| a.cast == event.token.owner)
            };
            let Some(attempt) = attempt else {
                continue;
            };
            if let Some(entry) = attempt
                .style_entry
                .as_mut()
                .filter(|e| event.token.sequence == 1 && e.epoch == event.epoch)
            {
                match event.event {
                    MotionExecutionEvent::Completed => entry.completion = Some(true),
                    MotionExecutionEvent::Cancelled => entry.completion = Some(false),
                    MotionExecutionEvent::Hook { .. } => {}
                }
                continue;
            }
            let Some(motion) = attempt.motion.as_mut().filter(|m| {
                m.sequence == event.token.sequence && m.epoch == event.epoch && m.due.is_none()
            }) else {
                continue;
            };
            match event.event {
                MotionExecutionEvent::Completed => motion.completion = Some(true),
                MotionExecutionEvent::Cancelled => motion.completion = Some(false),
                MotionExecutionEvent::Hook { .. } => unreachable!("hooks handled above"),
            }
        }
    }
}
