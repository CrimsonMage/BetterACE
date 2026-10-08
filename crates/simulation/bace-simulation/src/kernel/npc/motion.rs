//! Authored NPC motions run through the world's immutable motion program owner.
//! Exact tokens isolate them from casts, combat, and other interaction callbacks.
use super::Kernel;
use crate::{NpcEffect, NpcProposal};
use bace_gameplay_api::{NpcCompletion, NpcFailure as E, NpcOperation};
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken, PreparedMotionChain};
use bace_world::WorldMotionEvent;
use std::sync::Arc;
impl Kernel {
    pub fn inspect_npc_motion_source(
        &self,
        expected: &NpcProposal,
    ) -> Result<
        (
            bace_types::EntityId,
            u16,
            u64,
            bace_motion::SourceMotionState,
            f64,
        ),
        E,
    > {
        self.npcs.validate_service(expected)?;
        let NpcEffect::Service(NpcOperation::Motion { target, .. }) = expected.effect else {
            return Err(E::Unsupported);
        };
        let actor = if target {
            expected.context.target.ok_or(E::MissingActor)?
        } else {
            expected.context.source
        };
        let body = self.world.body(actor).map_err(|_| E::MissingActor)?;
        let properties = self.world.properties(actor).ok_or(E::MissingContent)?;
        let before = self
            .world
            .source_motion_state(actor)
            .ok_or(E::MissingContent)?;
        let scale = match properties.get(bace_entity::PropertyFamily::Float, 39) {
            Some(bace_entity::PropertyValue::Float(v)) => *v,
            _ => 1.0,
        };
        Ok((
            actor,
            body.accepted().epoch(),
            properties.revision(),
            before,
            scale,
        ))
    }
    pub fn begin_npc_prepared_motion(
        &mut self,
        expected: &NpcProposal,
        actor: bace_types::EntityId,
        epoch: u16,
        revision: u64,
        before: bace_motion::SourceMotionState,
        chain: Arc<PreparedMotionChain>,
    ) -> Result<MotionToken, E> {
        let current = self.inspect_npc_motion_source(expected)?;
        if (current.0, current.1, current.2, current.3) != (actor, epoch, revision, before) {
            return Err(E::Conflict);
        }
        self.begin_npc_motion(expected, chain)
    }

    pub fn begin_npc_motion(
        &mut self,
        expected: &NpcProposal,
        chain: Arc<PreparedMotionChain>,
    ) -> Result<MotionToken, E> {
        self.npcs.validate_service(expected)?;
        if self.npcs.motion_services.contains_key(&expected.ticket) {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::Motion {
            target,
            motion,
            extent,
            ..
        }) = expected.effect
        else {
            return Err(E::Unsupported);
        };
        let actor = if target {
            expected.context.target.ok_or(E::MissingActor)?
        } else {
            expected.context.source
        };
        if chain.motion != motion || chain.speed != extent {
            return Err(E::InvalidInput);
        }
        if chain.stop_chain().is_none() {
            return Err(E::MissingContent);
        }
        if self.npcs.reserved_except(actor, expected.ticket) {
            return Err(E::DurabilityPending);
        }
        let epoch = self
            .world
            .body(actor)
            .map_err(|_| E::MissingActor)?
            .accepted()
            .epoch();
        let token = MotionToken {
            domain: MotionDomain::Interaction,
            owner: expected.ticket,
            sequence: 1,
        };
        self.world
            .begin_motion(actor, token, chain)
            .map_err(|_| E::Conflict)?;
        self.npcs
            .motion_services
            .insert(expected.ticket, (expected.clone(), actor, epoch, token));
        Ok(token)
    }
    /// Hooks remain actual world outputs for projection. Only a matching terminal
    /// callback is retained as evidence permitting completion of the script row.
    pub fn poll_npc_motion(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<Option<WorldMotionEvent>, E> {
        let (proposal, actor, epoch, token) = self
            .npcs
            .motion_services
            .get(&expected.ticket)
            .ok_or(E::Conflict)?;
        if proposal != expected {
            return Err(E::Conflict);
        }
        if let Some(event) = self.npcs.motion_completions.get(&expected.ticket) {
            return Ok(Some(*event));
        }
        let Some(event) = self.world.take_motion_event_matching(*actor, *token) else {
            return Ok(None);
        };
        if event.epoch != *epoch {
            return Err(E::Conflict);
        }
        if matches!(
            event.event,
            MotionExecutionEvent::Completed | MotionExecutionEvent::Cancelled
        ) {
            self.npcs.motion_completions.insert(expected.ticket, event);
        }
        Ok(Some(event))
    }
    pub fn adopt_npc_motion_completion(
        &mut self,
        expected: &NpcProposal,
        event: WorldMotionEvent,
    ) -> Result<(), E> {
        if self.npcs.motion_completions.get(&expected.ticket) != Some(&event)
            || self
                .npcs
                .motion_services
                .get(&expected.ticket)
                .is_none_or(|(p, ..)| p != expected)
        {
            return Err(E::Conflict);
        }
        self.npcs.validate_service(expected)?;
        if event.event == MotionExecutionEvent::Completed {
            self.world
                .end_interaction_motion(event.actor, expected.ticket)
                .map_err(|_| E::Conflict)?;
        }
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.motion_services.remove(&expected.ticket);
        self.npcs.motion_completions.remove(&expected.ticket);
        self.confirm_npc_committed(expected)
    }
}
