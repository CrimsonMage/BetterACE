//! NPC casts retain their exact service ticket until the magic owner reports a
//! terminal outcome. Admitted or started casts never acknowledge spell effects.
use super::Kernel;
use crate::{NpcEffect, NpcProposal};
use bace_gameplay_api::{
    CastChange, CastOrigin, CastRequest, NpcCompletion, NpcFailure as E, NpcOperation,
    ServerCastOutcome,
};
impl Kernel {
    pub fn inspect_npc_cast_source(
        &self,
        expected: &NpcProposal,
    ) -> Result<
        (
            u16,
            u64,
            Option<bace_motion::SourceMotionState>,
            f64,
            bace_entity::EntityProperties,
        ),
        E,
    > {
        self.npcs.validate_service(expected)?;
        if !matches!(
            expected.effect,
            NpcEffect::Service(NpcOperation::Cast { .. })
        ) {
            return Err(E::Unsupported);
        }
        let actor = expected.context.source;
        let body = self.world.body(actor).map_err(|_| E::MissingActor)?;
        let properties = self.world.properties(actor).ok_or(E::MissingContent)?;
        let scale = match properties.get(bace_entity::PropertyFamily::Float, 39) {
            Some(bace_entity::PropertyValue::Float(value)) => *value,
            _ => 1.0,
        };
        Ok((
            body.accepted().epoch(),
            properties.revision(),
            self.world.source_motion_state(actor),
            scale,
            properties.clone(),
        ))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "immutable source, motion and property fences are checked independently before NPC program admission"
    )]
    pub fn register_npc_cast_program(
        &mut self,
        expected: &NpcProposal,
        epoch: u16,
        property_revision: u64,
        motion: Option<bace_motion::SourceMotionState>,
        source: std::sync::Arc<bace_content::WeenieV1>,
        definition: crate::PreparedMagicDefinition,
        shapes: Vec<(u32, std::sync::Arc<bace_physics::CollisionShape>)>,
    ) -> Result<(), E> {
        let current = self.inspect_npc_cast_source(expected)?;
        let NpcEffect::Service(NpcOperation::Cast { spell, instant, .. }) = expected.effect else {
            return Err(E::Unsupported);
        };
        if (current.0, current.1, current.2) != (epoch, property_revision, motion)
            || definition.spell.spell.id != spell
            || shapes.len() > 256
        {
            return Err(E::Conflict);
        }
        source
            .validate(bace_content::ContentLimits::default())
            .map_err(|_| E::InvalidInput)?;
        if instant && self.world.combatant(expected.context.source).is_none() {
            self.prepare_instant_object_caster(expected.context.source, &source)
                .map_err(|_| E::MissingContent)?;
        } else {
            self.prepare_npc_magic_caster(expected.context.source, source)
                .map_err(|_| E::MissingContent)?;
        }
        let actor = expected.context.source;
        let uses_mana = !matches!(
            self.world
                .properties(actor)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Bool, 6)),
            Some(bace_entity::PropertyValue::Bool(false))
        );
        if instant {
            self.magic.register_instant_definition(definition, shapes)
        } else {
            self.magic
                .register_server_program(actor, definition, shapes, uses_mana)
        }
        .map_err(|_| E::MissingContent)
    }

    pub fn begin_npc_cast(&mut self, expected: &NpcProposal) -> Result<CastChange, E> {
        self.npcs.validate_service(expected)?;
        if self.npcs.cast_services.contains_key(&expected.ticket) {
            return Err(E::Conflict);
        }
        let NpcEffect::Service(NpcOperation::Cast {
            spell,
            instant,
            pet_owner,
        }) = expected.effect
        else {
            return Err(E::Unsupported);
        };
        // Pet-owner identity must be supplied by the authoritative pet service;
        // the target in a script is not proof of that relationship.
        if pet_owner {
            return Err(E::Unsupported);
        }
        let actor = expected.context.source;
        let request = expected
            .context
            .target
            .map_or(CastRequest::Untargeted { spell }, |target| {
                CastRequest::Targeted { target, spell }
            });
        if self.npcs.reserved_except(actor, expected.ticket) {
            return Err(E::DurabilityPending);
        }
        self.magic
            .claim_emote_outcome(actor, expected.ticket)
            .map_err(|_| E::Capacity)?;
        let change = match self.cast_from_server(
            CastOrigin::Emote {
                actor,
                event: expected.ticket,
                instant,
            },
            request,
        ) {
            Ok(change) => change,
            Err(_) => {
                self.magic.release_emote_outcome(actor, expected.ticket);
                return Err(E::Conflict);
            }
        };
        self.npcs
            .cast_services
            .insert(expected.ticket, expected.clone());
        Ok(change)
    }
    /// Returns a terminal owner outcome for persistence/projection. This retains
    /// the pending NPC row; the service adapter must checkpoint completion before
    /// calling `adopt_npc_cast_completion`.
    pub fn poll_npc_cast(
        &mut self,
        expected: &NpcProposal,
    ) -> Result<Option<ServerCastOutcome>, E> {
        if self.npcs.cast_services.get(&expected.ticket) != Some(expected) {
            return Err(E::Conflict);
        }
        if let Some(outcome) = self.npcs.cast_completions.get(&expected.ticket) {
            return Ok(Some(outcome.clone()));
        }
        while let Some(outcome) = self
            .magic
            .take_emote_outcome(expected.context.source, expected.ticket)
        {
            if !matches!(outcome.result, Ok(CastChange::Started { .. })) {
                self.npcs
                    .cast_completions
                    .insert(expected.ticket, outcome.clone());
                return Ok(Some(outcome));
            }
        }
        Ok(None)
    }
    pub fn adopt_npc_cast_completion(
        &mut self,
        expected: &NpcProposal,
        outcome: &ServerCastOutcome,
    ) -> Result<(), E> {
        if self.npcs.cast_services.get(&expected.ticket) != Some(expected)
            || self.npcs.cast_completions.get(&expected.ticket) != Some(outcome)
            || !matches!(outcome.origin,CastOrigin::Emote{actor,event,..}if actor==expected.context.source&&event==expected.ticket)
            || matches!(outcome.result, Ok(CastChange::Started { .. }))
        {
            return Err(E::Conflict);
        }
        self.npcs
            .mark_service_adopted(expected, NpcCompletion::Applied { post_delay: 0.0 })?;
        self.npcs.cast_services.remove(&expected.ticket);
        self.npcs.cast_completions.remove(&expected.ticket);
        self.magic
            .release_emote_outcome(expected.context.source, expected.ticket);
        self.confirm_npc_committed(expected)
    }
}
