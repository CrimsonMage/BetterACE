//! Receipt-gated item acceptance precedes the authored Give invocation.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcHandInRequest {
    pub context: bace_gameplay_api::ActionContext,
    pub source: EntityId,
    pub item: EntityId,
    pub count: u32,
    pub event: [u8; 16],
    pub operation: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NpcHandInTicket {
    /// Exact source category selected before acceptance: Refuse1 or Give6.
    pub category: u32,
    pub character_revision: u64,
    pub accepted_count: u32,
    pub request: NpcHandInRequest,
    pub inventory: crate::InventoryTicket,
    pub checkpoint: NpcSourceCheckpoint,
}
impl Npcs {
    pub(crate) fn preview_handin(
        &self,
        request: &NpcHandInRequest,
        template: u32,
        world: &World,
        tick: u64,
    ) -> Result<(NpcSourceCheckpoint, u32), NpcFailure> {
        if request.count == 0 || request.event == [0; 16] || request.operation == 0 {
            return Err(NpcFailure::InvalidInput);
        }
        let source = self
            .sources
            .get(&request.source)
            .ok_or(NpcFailure::MissingActor)?;
        if !source.recovery_ready
            || source.journal_hold.is_some()
            || source.manager.busy()
            || source.manager.has_detached()
            || self.reserved(request.source)
        {
            return Err(NpcFailure::DurabilityPending);
        }
        if !super::signals::within_use_radius(
            world,
            request.context.actor,
            request.source,
            source.use_radius,
        )? {
            return Err(NpcFailure::InvalidInput);
        }
        let root = self.root.as_ref().ok_or(NpcFailure::MissingContent)?;
        let mut random = root
            .event_stream(request.event, Domain::Npc)
            .map_err(|_| NpcFailure::InvalidInput)?;
        let draw = (random.next_u64().map_err(|_| NpcFailure::Capacity)? >> 11) as f64
            * (1.0 / 9_007_199_254_740_992.0);
        let logical_now = tick as f64 / 30.0 + source.clock_offset;
        // Pinned WorldObject.HasGiveOrRefuseEmoteForItem: Refuse precedes Give,
        // including an empty selected Refuse set. Each lookup consumes its draw.
        let context = NpcContext {
            source: request.source,
            target: Some(request.context.actor),
            operation: request.operation,
        };
        let refusal = source
            .manager
            .preview_selected_trigger(
                &NativeTrigger {
                    category: 1,
                    template: Some(template),
                    ..Default::default()
                },
                context,
                logical_now,
                Some(draw),
            )
            .map_err(|_| NpcFailure::Conflict)?;
        let (vm, category) = if let Some(refusal) = refusal {
            (refusal, 1)
        } else {
            let draw = (random.next_u64().map_err(|_| NpcFailure::Capacity)? >> 11) as f64
                * (1.0 / 9_007_199_254_740_992.0);
            let given = source
                .manager
                .preview_selected_trigger(
                    &NativeTrigger {
                        category: 6,
                        template: Some(template),
                        ..Default::default()
                    },
                    context,
                    logical_now,
                    Some(draw),
                )
                .map_err(|_| NpcFailure::Conflict)?;
            if let Some(given) = given {
                (given, 6)
            } else if matches!(
                world
                    .properties(request.source)
                    .and_then(|p| p.get(bace_entity::PropertyFamily::Bool, 79)),
                Some(bace_entity::PropertyValue::Bool(true))
            ) {
                (source.manager.checkpoint(), 6)
            } else {
                return Err(NpcFailure::MissingContent);
            }
        };
        let invocation = NpcInvocationCheckpoint {
            operation: request.operation,
            event_id: request.event,
            key_version: root.key_version(),
            random_position: random.position(),
        };
        Ok((
            NpcSourceCheckpoint {
                inventory: None,
                location: Some({
                    let (cell, state) = world
                        .actor_state(request.source)
                        .map_err(|_| NpcFailure::MissingActor)?;
                    NpcSourceLocation {
                        cell,
                        position: state.position(),
                        heading: state.heading_radians(),
                        facts: bace_emotes::NpcActorFacts {
                            player: world
                                .combatant(request.source)
                                .is_some_and(|c| c.profile().player),
                            creature: world.combatant(request.source).is_some(),
                        },
                    }
                }),
                properties: world.properties(request.source).cloned(),
                source_quests: self.quests.get(&request.source).map(|q| {
                    (
                        q.revision(),
                        q.iter().map(|(n, p)| (n.to_owned(), p)).collect(),
                    )
                }),
                archive: None,
                source: request.source,
                active_operation: request.operation,
                invocations: vec![invocation],
                logical_now,
                event_id: request.event,
                key_version: root.key_version(),
                random_position: random.position(),
                vm,
                pending: vec![],
            },
            category,
        ))
    }
}
