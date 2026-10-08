//! One simulation-owner action sequence per actor. Prepared programs are immutable;
//! packet acknowledgements and client timestamps never complete these motions.
use super::*;
use bace_motion::{
    MotionDomain, MotionExecutionEvent, MotionPlayback, MotionToken, PreparedMotionChain,
    RootFrame, TimelineError,
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldMotionEvent {
    pub actor: EntityId,
    pub epoch: u16,
    pub token: MotionToken,
    pub event: MotionExecutionEvent,
}
pub(super) struct OwnedMotion {
    pub(super) playback: MotionPlayback,
    pub(super) epoch: u16,
    pub(super) ending: bool,
}
pub(super) struct CompletedCallback {
    token: MotionToken,
    epoch: u16,
    phase: u64,
}
fn queue_cancellation(
    events: &mut std::collections::VecDeque<WorldMotionEvent>,
    actor: EntityId,
    owned: &OwnedMotion,
) -> bool {
    let count = owned.playback.pending_tokens().count();
    if events.len() + count.max(1) > 4096 {
        return false;
    }
    let event = |token| WorldMotionEvent {
        actor,
        epoch: owned.epoch,
        token,
        event: MotionExecutionEvent::Cancelled,
    };
    if count == 0 {
        events.push_back(event(owned.playback.token));
    } else {
        events.extend(owned.playback.pending_tokens().map(event));
    }
    true
}
impl World {
    pub(super) fn reserve_motion_buffers(&mut self) {
        if self.motion_events.capacity() < 4096 {
            self.motion_events.reserve(4096 - self.motion_events.len());
        }
        if self.motion_retired.capacity() < 4096 {
            self.motion_retired
                .reserve(4096 - self.motion_retired.len());
        }
        if self.motion_scratch.capacity() < 4096 {
            self.motion_scratch
                .reserve(4096 - self.motion_scratch.len());
        }
    }
    pub fn begin_motion(
        &mut self,
        actor: EntityId,
        token: MotionToken,
        chain: std::sync::Arc<PreparedMotionChain>,
    ) -> Result<(), WorldError> {
        self.begin_motion_with_suffix(actor, token, chain, None)
    }
    /// Atomically queue an authored style transition and its immediate action.
    /// The existing playback owns both callbacks; no timer or client acknowledgment
    /// is needed between these source commands. Rejected batches change no state.
    pub fn begin_style_action(
        &mut self,
        actor: EntityId,
        style_token: MotionToken,
        style: std::sync::Arc<PreparedMotionChain>,
        action_token: MotionToken,
        action: std::sync::Arc<PreparedMotionChain>,
    ) -> Result<(), WorldError> {
        let before = style.source_transition().ok_or(WorldError::InvalidMotion)?;
        let after = action
            .source_transition()
            .ok_or(WorldError::InvalidMotion)?;
        if style.motion & 0x80000000 == 0
            || style.motion != before.after.style
            || action.motion & 0x10000000 == 0
            || action.motion & 0x80000000 != 0
            || before.after != after.before
            || after.before.style != after.after.style
            || style.continues_cycle()
            || action.continues_cycle()
            || style_token.domain != action_token.domain
            || style_token.owner != action_token.owner
            || style_token.sequence >= action_token.sequence
        {
            return Err(WorldError::InvalidMotion);
        }
        self.begin_motion_with_suffix(actor, style_token, style, Some((action_token, action)))
    }
    fn begin_motion_with_suffix(
        &mut self,
        actor: EntityId,
        token: MotionToken,
        chain: std::sync::Arc<PreparedMotionChain>,
        suffix: Option<(MotionToken, std::sync::Arc<PreparedMotionChain>)>,
    ) -> Result<(), WorldError> {
        let body = self.body(actor)?;
        let changes_forward = chain.motion & 0x80000000 != 0 || chain.motion == 0x41000003;
        let target_style = chain
            .source_transition()
            .filter(|_| changes_forward)
            .map(|s| s.after.style);
        let style_asset = target_style
            .and_then(|style| self.locomotion_styles.get(&(actor, style)))
            .cloned();
        if target_style.is_some()
            && !self.locomotion_styles.is_empty()
            && body.collision_shape().is_some()
            && style_asset.is_none()
        {
            return Err(WorldError::InvalidMotion);
        }
        let chain = if let Some(style) = &style_asset {
            let drive = body
                .validate_animated_style(style, chain.clears_style_modifiers())
                .map_err(|_| WorldError::InvalidMotion)?;
            std::sync::Arc::new(
                chain
                    .with_locomotion_modifiers(&style.profile, drive)
                    .map_err(|_| WorldError::InvalidMotion)?,
            )
        } else {
            chain
        };
        if token.domain == MotionDomain::Casting
            && self
                .combatants
                .get(&actor)
                .is_some_and(|c| c.profile().player)
            && !chain.is_rootless()
            && !changes_forward
            && body.accepted().grounded()
            && body.has_locomotion_intent()
        {
            return Err(WorldError::InvalidMotion);
        }
        if self.anchors.contains(&actor)
            || self.retirement_holds.contains_key(&actor)
            || self.portal_transit.contains_key(&actor)
            || self.combatants.get(&actor).is_some_and(|c| c.health() == 0)
        {
            return Err(WorldError::InvalidMotion);
        }
        if let Some(current) = self.motions.get(&actor) {
            if suffix.is_none()
                && current.playback.token == token
                && current.playback.requested_chain() == &chain
            {
                return Ok(());
            }
            if !current.playback.cursor().completed
                && (!current.ending
                    || current.playback.has_pending_action() && chain.motion & 0x80000000 == 0)
            {
                return Err(WorldError::MotionBusy);
            }
        }
        if self.motions.len() >= 4096 && !self.motions.contains_key(&actor)
            || self.motion_last.len() >= 12288
                && !self.motion_last.contains_key(&(actor, token.domain))
            || self
                .motion_last
                .get(&(actor, token.domain))
                .is_some_and(|old| *old >= (token.owner, token.sequence))
        {
            return Err(WorldError::InvalidMotion);
        }
        let epoch = body.accepted().epoch();
        let playback = if let Some(current) = self.motions.get(&actor)
            && !current.playback.cursor().completed
            && current.ending
        {
            current.playback.append_stop(chain, token)
        } else if chain.continues_cycle() {
            self.motions
                .get(&actor)
                .ok_or(WorldError::InvalidMotion)?
                .playback
                .continue_cycle(chain, token)
        } else {
            if let (Some(old), Some(next)) =
                (self.source_motion_state(actor), chain.source_transition())
                && old != next.before
            {
                return Err(WorldError::InvalidMotion);
            }
            MotionPlayback::new(chain, token)
        }
        .map_err(|_| WorldError::InvalidMotion)?;
        let clear_modifiers = playback.requested_chain().clears_style_modifiers();
        let (token, playback) = if let Some((next_token, next)) = suffix {
            let next = if let Some(style) = &style_asset {
                let drive = body
                    .validate_animated_style(style, clear_modifiers)
                    .map_err(|_| WorldError::InvalidMotion)?;
                std::sync::Arc::new(
                    next.with_locomotion_modifiers(&style.profile, drive)
                        .map_err(|_| WorldError::InvalidMotion)?,
                )
            } else {
                next
            };
            let appended = playback
                .append_stop(next, next_token)
                .map_err(|_| WorldError::InvalidMotion)?;
            (next_token, appended)
        } else {
            (token, playback)
        };
        self.reserve_motion_buffers();
        if let Some(style) = style_asset {
            self.body_mut(actor)?
                .adopt_animated_style(style, clear_modifiers)
                .expect("preflighted style controls");
        }
        self.motion_last
            .insert((actor, token.domain), (token.owner, token.sequence));
        self.motions.insert(
            actor,
            OwnedMotion {
                playback,
                epoch,
                ending: false,
            },
        );
        Ok(())
    }
    pub fn cancel_motion(&mut self, actor: EntityId, token: MotionToken) -> Result<(), WorldError> {
        if self.retirement_held(actor) {
            return Err(WorldError::VitalReserved);
        }
        let current = self.motions.get(&actor).ok_or(WorldError::InvalidMotion)?;
        if current.playback.token != token {
            return Err(WorldError::InvalidMotion);
        }
        if !queue_cancellation(&mut self.motion_events, actor, current) {
            return Err(WorldError::MotionBackpressure);
        }
        self.motions.remove(&actor);
        if let Some(actor) = self.actors.get_mut(&actor) {
            actor.body.clear_motion_root();
        }
        Ok(())
    }
    pub fn end_cast_motion(&mut self, actor: EntityId, owner: u64) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Casting)
    }
    pub fn end_physical_motion(&mut self, actor: EntityId, owner: u64) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Physical)
    }
    pub fn end_interaction_motion(
        &mut self,
        actor: EntityId,
        owner: u64,
    ) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Interaction)
    }
    pub fn end_crafting_motion(&mut self, actor: EntityId, owner: u64) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Crafting)
    }
    pub fn end_recall_motion(&mut self, actor: EntityId, owner: u64) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Recall)
    }
    pub fn end_inventory_motion(&mut self, actor: EntityId, owner: u64) -> Result<(), WorldError> {
        self.end_motion_domain(actor, owner, MotionDomain::Inventory)
    }
    fn end_motion_domain(
        &mut self,
        actor: EntityId,
        owner: u64,
        domain: MotionDomain,
    ) -> Result<(), WorldError> {
        if self.retirement_held(actor) {
            return Err(WorldError::VitalReserved);
        }
        let Some(current) = self.motions.get(&actor) else {
            return Ok(());
        };
        if current.playback.token.domain != domain || current.playback.token.owner != owner {
            return Err(WorldError::InvalidMotion);
        }
        if current.ending {
            return Ok(());
        }
        let stop = current
            .playback
            .chain()
            .stop_chain()
            .cloned()
            .ok_or(WorldError::InvalidMotion)?;
        let token = MotionToken {
            sequence: current
                .playback
                .token
                .sequence
                .checked_add(1)
                .ok_or(WorldError::InvalidMotion)?,
            ..current.playback.token
        };
        let playback = current
            .playback
            .append_stop(stop, token)
            .map_err(|_| WorldError::InvalidMotion)?;
        let epoch = current.epoch;
        self.motion_last
            .insert((actor, domain), (token.owner, token.sequence));
        self.motions.insert(
            actor,
            OwnedMotion {
                playback,
                epoch,
                ending: true,
            },
        );
        Ok(())
    }
    pub(super) fn retire_removed_actor_motion(&mut self, actor: EntityId) {
        self.motion_callbacks.remove(&actor);
        if let Some(value) = self.actors.get_mut(&actor) {
            value.body.clear_motion_root();
        }

        if self
            .motions
            .get(&actor)
            .is_some_and(|m| m.playback.cursor().completed)
        {
            self.motions.remove(&actor);
        }
        self.motion_last.retain(|(id, _), _| *id != actor);
        self.motion_blocked.remove(&actor);
    }
    pub fn can_retire_actor_motion(&self, actor: EntityId) -> bool {
        self.motions
            .get(&actor)
            .is_none_or(|m| m.playback.cursor().completed)
            && !self.motion_events.iter().any(|event| event.actor == actor)
    }
    pub fn retire_actor_motion(&mut self, actor: EntityId) -> Result<(), WorldError> {
        if self.retirement_held(actor) {
            return Err(WorldError::VitalReserved);
        }
        if self
            .motions
            .get(&actor)
            .is_some_and(|m| !m.playback.cursor().completed)
        {
            return Err(WorldError::MotionBusy);
        }
        if self.motion_events.iter().any(|event| event.actor == actor) {
            return Err(WorldError::MotionBackpressure);
        }
        self.retire_removed_actor_motion(actor);
        Ok(())
    }
    pub fn take_motion_event(&mut self) -> Option<WorldMotionEvent> {
        let event = self.motion_events.pop_front()?;
        self.record_motion_callback(event);
        Some(event)
    }
    pub fn take_motion_event_for(&mut self, domain: MotionDomain) -> Option<WorldMotionEvent> {
        let index = self
            .motion_events
            .iter()
            .position(|event| event.token.domain == domain)?;
        let event = self.motion_events.remove(index)?;
        self.record_motion_callback(event);
        Some(event)
    }
    /// A delegated service consumes only its own completion, leaving unrelated
    /// interaction owners' hooks in their original order.
    pub fn take_motion_event_matching(
        &mut self,
        actor: EntityId,
        token: MotionToken,
    ) -> Option<WorldMotionEvent> {
        let index = self
            .motion_events
            .iter()
            .position(|event| event.actor == actor && event.token == token)?;
        let event = self.motion_events.remove(index)?;
        self.record_motion_callback(event);
        Some(event)
    }
    fn record_motion_callback(&mut self, event: WorldMotionEvent) {
        if event.event == MotionExecutionEvent::Completed
            && (self.motion_callbacks.len() < 4096
                || self.motion_callbacks.contains_key(&event.actor))
        {
            self.motion_callbacks.insert(
                event.actor,
                CompletedCallback {
                    token: event.token,
                    epoch: event.epoch,
                    phase: self.motion_phase,
                },
            );
        }
    }
    /// Drain only callback-appended zero-count transitions in the same physics
    /// phase. Ordinary input cannot authorize this path. No frame/pose time is
    /// consumed; exhausted capacity retains the new controller for a later tick.
    pub fn drain_motion_callbacks(
        &mut self,
        actor: EntityId,
        completed: MotionToken,
        epoch: u16,
    ) -> Result<usize, WorldError> {
        if self.retirement_held(actor) {
            return Err(WorldError::VitalReserved);
        }
        let authorization = self
            .motion_callbacks
            .get(&actor)
            .ok_or(WorldError::InvalidMotion)?;
        if self.motion_phase == 0
            || authorization.phase != self.motion_phase
            || authorization.token != completed
            || authorization.epoch != epoch
            || self.body(actor)?.accepted().epoch() != epoch
        {
            return Err(WorldError::InvalidMotion);
        }
        let owned = self.motions.get(&actor).ok_or(WorldError::InvalidMotion)?;
        if owned.epoch != epoch
            || owned.playback.token.domain != completed.domain
            || owned.playback.token.owner != completed.owner
            || owned.playback.token.sequence <= completed.sequence
        {
            return Err(WorldError::InvalidMotion);
        }
        if self.motion_callback_budget == 0 {
            self.motion_blocked.insert(actor);
            return Err(WorldError::MotionBackpressure);
        }
        self.motion_scratch.clear();
        let owned = self
            .motions
            .get_mut(&actor)
            .expect("callback owner preflight");
        if owned.playback.has_ready_zero_callback() {
            let result = owned.playback.advance_tagged(
                0.0,
                &mut self.motion_scratch,
                4096 - self.motion_events.len(),
            );
            if let Err(error) = result {
                return Err(match error {
                    TimelineError::Capacity => {
                        self.motion_blocked.insert(actor);
                        WorldError::MotionBackpressure
                    }
                    _ => WorldError::InvalidMotion,
                });
            }
        }
        self.motion_callback_budget -= 1;
        self.motion_callbacks.remove(&actor);
        let count = self.motion_scratch.len();
        for event in self.motion_scratch.drain(..) {
            self.motion_events.push_back(WorldMotionEvent {
                actor,
                epoch,
                token: event.token,
                event: event.event,
            });
        }
        self.motion_blocked.remove(&actor);
        Ok(count)
    }
    pub fn has_motion_state(&self) -> bool {
        !self.motions.is_empty() || !self.motion_events.is_empty()
    }
    pub fn motion_busy(&self, actor: EntityId) -> bool {
        self.motions
            .get(&actor)
            .is_some_and(|m| !m.playback.cursor().completed)
    }
    pub fn has_pending_action_motion(&self, actor: EntityId) -> bool {
        self.motions
            .get(&actor)
            .is_some_and(|m| m.playback.has_pending_action())
    }
    pub fn motion_backpressured(&self, actor: EntityId) -> bool {
        self.motion_blocked.contains(&actor)
    }
    pub fn source_motion_state(&self, actor: EntityId) -> Option<bace_motion::SourceMotionState> {
        let epoch = self.actors.get(&actor)?.body.accepted().epoch();
        let motion = self.motions.get(&actor).filter(|m| m.epoch == epoch);
        let source = motion
            .and_then(|m| m.playback.chain().source_transition())
            .map(|s| s.after);
        let released = motion.is_none_or(|m| {
            (m.ending || m.playback.requested_chain().motion == 0x41000003)
                && m.playback.cursor().completed
                && m.playback.chain().cyclic_is_rootless()
        });
        if released
            && let Some((style, drive)) = self
                .actors
                .get(&actor)
                .and_then(|a| a.body.locomotion_projection())
        {
            return Some(bace_motion::SourceMotionState {
                style,
                substate: drive.forward_motion,
                speed: drive.forward_rate,
            });
        }
        source
    }
    pub fn source_motion_token(&self, actor: EntityId) -> Option<MotionToken> {
        self.motions.get(&actor).map(|m| m.playback.token)
    }
    pub(super) fn advance_motions(&mut self) -> Result<(), WorldError> {
        self.motion_phase = self
            .motion_phase
            .checked_add(1)
            .ok_or(WorldError::InvalidMotion)?;
        self.motion_callbacks.clear();
        self.motion_callback_budget = 256;
        self.motion_retired.clear();
        for (id, owned) in &mut self.motions {
            if self.retirement_holds.contains_key(id) {
                continue;
            }
            if owned.playback.token.domain != MotionDomain::Death
                && self.actors.get(id).is_some_and(|actor| {
                    self.dormant_landblocks
                        .contains(&((actor.cell.0 >> 16) as u16))
                })
            {
                continue;
            }
            let player_cast = owned.playback.token.domain == MotionDomain::Casting
                && self.combatants.get(id).is_some_and(|c| c.profile().player);
            let ready_released = (owned.ending
                || owned.playback.requested_chain().motion == 0x41000003)
                && owned.playback.cursor().completed
                && owned
                    .playback
                    .chain()
                    .source_transition()
                    .is_some_and(|s| s.after.substate == 0x41000003)
                && owned.playback.chain().cyclic_is_rootless();
            let unsupported_slide = player_cast
                && !ready_released
                && !owned.playback.chain().is_rootless()
                && owned.playback.requested_chain().motion & 0x80000000 == 0
                && owned.playback.requested_chain().motion != 0x41000003
                && self.actors.get(id).is_some_and(|a| {
                    a.body.accepted().grounded() && a.body.has_locomotion_intent()
                });
            let invalid = unsupported_slide
                || self
                    .actors
                    .get(id)
                    .is_none_or(|a| a.body.accepted().epoch() != owned.epoch)
                || self.portal_transit.contains_key(id)
                || self.combatants.get(id).is_some_and(|c| c.health() == 0)
                    && owned.playback.token.domain != MotionDomain::Death;
            if invalid {
                if queue_cancellation(&mut self.motion_events, *id, owned) {
                    self.motion_retired.push(*id);
                } else {
                    self.motion_blocked.insert(*id);
                }
                continue;
            }
            self.motion_scratch.clear();
            let result = owned.playback.advance_tagged(
                f64::from(bace_physics::STEP_SECONDS),
                &mut self.motion_scratch,
                4096 - self.motion_events.len(),
            );
            let actor = self.actors.get_mut(id).expect("checked motion actor");
            match result {
                Ok(root) => {
                    if ready_released || player_cast && owned.playback.chain().is_rootless() {
                        actor.body.clear_motion_root();
                    } else {
                        actor
                            .body
                            .apply_motion_root(root)
                            .expect("validated finite motion root");
                    }
                    self.motion_blocked.remove(id);
                    for event in self.motion_scratch.drain(..) {
                        // Death completion is read from this trusted cursor by
                        // the lifecycle owner; client visual hooks stay visual.
                        if event.token.domain == MotionDomain::Death {
                            continue;
                        }
                        self.motion_events.push_back(WorldMotionEvent {
                            actor: *id,
                            epoch: owned.epoch,
                            token: event.token,
                            event: event.event,
                        });
                    }
                    // Retain the authored return/cyclic sequence until replaced
                    // or cancelled; a completion notification does not delete it.
                }
                Err(TimelineError::Capacity) => {
                    // Keep the program/cursor while output is full. Gravity and
                    // collision still run; no fabricated motion_done is emitted.
                    if player_cast && owned.playback.chain().is_rootless() {
                        actor.body.clear_motion_root();
                    } else {
                        let _ = actor.body.apply_motion_root(RootFrame::default());
                    }
                    self.motion_blocked.insert(*id);
                }
                Err(_) => {
                    if queue_cancellation(&mut self.motion_events, *id, owned) {
                        self.motion_retired.push(*id);
                    } else {
                        self.motion_blocked.insert(*id);
                    }
                }
            }
        }
        for id in self.motion_retired.drain(..) {
            self.motions.remove(&id);
            self.motion_blocked.remove(&id);
            if !self.actors.contains_key(&id) {
                self.motion_last.retain(|(actor, _), _| *actor != id);
            }
        }
        Ok(())
    }
}
