//! Verified recall actions share the world motion owner. The source recall timer
//! remains independent of the animation callback (notably Marketplace's 14 s).
use super::*;
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken, PreparedMotionChain};

impl Kernel {
    pub(super) fn begin_binding_motion(
        &mut self,
        actor: EntityId,
        owner: u64,
        style: Option<Arc<PreparedMotionChain>>,
        chain: Arc<PreparedMotionChain>,
    ) -> Result<(), RecallError> {
        let source = chain
            .source_transition()
            .ok_or(RecallError::MissingAssets)?;
        if chain.motion != 0x1000_0057
            || chain.speed != 1.
            || source.before.style != 0x8000_003d
            || source.after.style != 0x8000_003d
            || chain.stop_chain().is_none()
            || self
                .world
                .combatant(actor)
                .is_none_or(|c| c.mode() != 1 && style.is_none())
        {
            return Err(RecallError::MissingAssets);
        }
        let token = MotionToken {
            domain: MotionDomain::Recall,
            owner,
            sequence: if style.is_some() { 2 } else { 1 },
        };
        if let Some(style) = style {
            if style.motion != 0x8000_003d
                || style.speed != 1.
                || style
                    .source_transition()
                    .is_none_or(|s| s.after != source.before)
            {
                return Err(RecallError::MissingAssets);
            }
            self.world
                .begin_style_action(
                    actor,
                    MotionToken {
                        sequence: 1,
                        ..token
                    },
                    style,
                    token,
                    chain,
                )
                .map_err(|_| RecallError::Busy)?;
            self.world
                .combatant_mut(actor)
                .expect("preflighted combatant")
                .set_mode(1);
        } else {
            self.world
                .begin_motion(actor, token, chain)
                .map_err(|_| RecallError::Busy)?;
        }
        Ok(())
    }

    pub(super) fn stop_binding_motion(&mut self, actor: EntityId, owner: u64) -> bool {
        if self
            .world
            .source_motion_token(actor)
            .is_some_and(|token| token.domain == MotionDomain::Recall && token.owner == owner)
        {
            self.world.end_recall_motion(actor, owner).is_ok()
        } else {
            true
        }
    }

    pub(super) fn begin_recall_motion(
        &mut self,
        actor: EntityId,
        ordinal: u64,
        kind: RecallKind,
        style: Option<Arc<PreparedMotionChain>>,
        chain: Arc<PreparedMotionChain>,
    ) -> Result<crate::recalls::RecallMotion, RecallError> {
        let source = chain
            .source_transition()
            .ok_or(RecallError::MissingAssets)?;
        if chain.motion != kind.motion()
            || chain.speed != 1.
            || source.before.style != 0x8000003d
            || source.after.style != 0x8000003d
            || chain.stop_chain().is_none()
            || self
                .world
                .combatant(actor)
                .is_none_or(|c| c.mode() != 1 && style.is_none())
        {
            return Err(RecallError::MissingAssets);
        }
        let token = MotionToken {
            domain: MotionDomain::Recall,
            owner: ordinal,
            sequence: if style.is_some() { 2 } else { 1 },
        };
        if let Some(style) = style {
            if style.motion != 0x8000003d
                || style.speed != 1.
                || style
                    .source_transition()
                    .is_none_or(|s| s.after != source.before)
            {
                return Err(RecallError::MissingAssets);
            }
            self.world
                .begin_style_action(
                    actor,
                    MotionToken {
                        sequence: 1,
                        ..token
                    },
                    style,
                    token,
                    chain,
                )
                .map_err(|_| RecallError::Busy)?;
            // No owner can interleave between the checked atomic motion admission
            // and this infallible switch to the source NonCombat mode.
            self.world
                .combatant_mut(actor)
                .expect("preflighted combatant")
                .set_mode(1);
        } else {
            self.world
                .begin_motion(actor, token, chain)
                .map_err(|_| RecallError::Busy)?;
        }
        Ok(crate::recalls::RecallMotion {
            token,
            completed: None,
        })
    }

    pub(super) fn stop_recall_motion(&mut self, actor: EntityId) -> bool {
        let Some(motion) = self
            .recalls
            .pending
            .get(&actor)
            .and_then(|p| p.motion.as_ref())
        else {
            return true;
        };
        if self.world.source_motion_token(actor).is_some_and(|token| {
            token.domain == MotionDomain::Recall && token.owner == motion.token.owner
        }) {
            self.world
                .end_recall_motion(actor, motion.token.owner)
                .is_ok()
        } else {
            // Retirement, teleport or a later accepted owner has already removed
            // this action. Never end a different owner's sequence.
            true
        }
    }

    pub(super) fn step_recall_motions(&mut self) {
        // Also drain stop/retirement callbacks after their logical recall has
        // staged, so a long authored animation cannot strand the shared queue.
        for _ in 0..128 {
            let Some(event) = self.world.take_motion_event_for(MotionDomain::Recall) else {
                break;
            };
            if let Some(binding) = self.recalls.pending_bindings.get_mut(&event.actor)
                && event.token.owner == binding.motion_owner
            {
                match event.event {
                    MotionExecutionEvent::Completed
                        if event.token.sequence == binding.action_sequence =>
                    {
                        binding.completed = Some(true)
                    }
                    MotionExecutionEvent::Completed if event.token.sequence == 1 => {
                        binding.action_started = true
                    }
                    MotionExecutionEvent::Cancelled => binding.completed = Some(false),
                    MotionExecutionEvent::Hook { .. } | MotionExecutionEvent::Completed => {}
                }
                continue;
            }
            let Some(motion) = self
                .recalls
                .pending
                .get_mut(&event.actor)
                .and_then(|p| p.motion.as_mut())
            else {
                continue;
            };
            if motion.token != event.token {
                continue;
            }
            match event.event {
                MotionExecutionEvent::Completed => motion.completed = Some(true),
                MotionExecutionEvent::Cancelled => motion.completed = Some(false),
                MotionExecutionEvent::Hook { .. } => {}
            }
        }
        self.recalls.scratch.clear();
        self.recalls
            .scratch
            .extend(self.recalls.pending.keys().copied());
        for index in 0..self.recalls.scratch.len() {
            let actor = self.recalls.scratch[index];
            let pending = &self.recalls.pending[&actor];
            let cancelled = pending
                .motion
                .as_ref()
                .is_some_and(|m| m.completed == Some(false))
                || self.world.combatant(actor).is_none_or(|c| c.health() == 0)
                || !self
                    .world
                    .actor_state(actor)
                    .is_ok_and(|(_, p)| p.epoch() == pending.start_epoch);
            if cancelled {
                if self.recalls.events.len() >= self.recalls.capacity
                    || !self.stop_recall_motion(actor)
                {
                    continue;
                }
                let pending = self
                    .recalls
                    .pending
                    .remove(&actor)
                    .expect("retained recall");
                self.recalls.events.push_back(RecallEvent::Cancelled {
                    context: pending.context,
                });
            } else if pending
                .motion
                .as_ref()
                .is_some_and(|m| m.completed == Some(true))
                && self.stop_recall_motion(actor)
            {
                self.recalls
                    .pending
                    .get_mut(&actor)
                    .expect("retained recall")
                    .motion = None;
            }
        }
    }
}
