//! A local signal selects all same-landblock listeners from accepted owner
//! geometry. Prepare every new VM before changing any listener's schedule.
use super::*;
#[cfg(test)]
mod tests;
use bace_entity::{PropertyFamily, PropertyValue};
pub(super) fn cylinder(
    world: &World,
    id: EntityId,
) -> Result<bace_emotes::SignalCylinder, NpcFailure> {
    let body = world.body(id).map_err(|_| NpcFailure::MissingActor)?;
    let (radius, height) = if let Some(shape) = body.collision_shape() {
        (
            shape.nominal_radius().ok_or(NpcFailure::MissingContent)?,
            shape.nominal_height().ok_or(NpcFailure::MissingContent)?,
        )
    } else {
        (body.collision_radius(), body.collision_radius() * 2.0)
    };
    Ok(bace_emotes::SignalCylinder {
        position: body.accepted().position(),
        radius,
        height,
    })
}
/// ACE WorldObject_Use.IsWithinUseRadiusOf preserves signed Float54 and
/// compares the float-cast signed cylinder distance. Negative radii are valid
/// authored thresholds, not malformed actors or a request for a default.
pub(super) fn within_use_radius(
    world: &World,
    actor: EntityId,
    target: EntityId,
    radius: f32,
) -> Result<bool, NpcFailure> {
    if !radius.is_finite() {
        return Err(NpcFailure::InvalidInput);
    }
    let (_, clear) = world
        .attack_geometry(actor, target, 0.)
        .map_err(|_| NpcFailure::MissingActor)?;
    if !clear {
        return Ok(false);
    }
    let distance = bace_emotes::cylinder_distance(cylinder(world, actor)?, cylinder(world, target)?)
        .map_err(|_| NpcFailure::InvalidInput)? as f32;
    Ok(distance <= radius)
}
impl Npcs {
    pub(crate) fn signal(
        &mut self,
        expected: &NpcProposal,
        world: &World,
        tick: u64,
        inventory: &crate::inventory::Inventory,
    ) -> Result<usize, NpcFailure> {
        self.validate_service(expected)?;
        let NpcEffect::Service(NpcOperation::Signal(ref message)) = expected.effect else {
            return Err(NpcFailure::Unsupported);
        };
        if message.trim().is_empty() {
            return Ok(0);
        }
        if message.len() > 4096 {
            return Err(NpcFailure::InvalidInput);
        }
        let emitter = expected.context.source;
        let emitter_cell = world
            .actor_state(emitter)
            .map_err(|_| NpcFailure::MissingActor)?
            .0;
        let emitter_shape = cylinder(world, emitter)?;
        let root = self.root.as_ref().ok_or(NpcFailure::MissingContent)?;
        let event = self
            .sources
            .get(&emitter)
            .ok_or(NpcFailure::MissingActor)?
            .event_id;
        let stream = root
            .event_stream(event, Domain::Npc)
            .map_err(|_| NpcFailure::InvalidInput)?;
        let scope = stream
            .fork(b"local-signal", expected.ticket)
            .map_err(|_| NpcFailure::InvalidInput)?;
        let mut deliveries = Vec::new();
        for &actor in &self.source_order {
            if actor == emitter {
                continue;
            }
            let source = self.sources.get(&actor).ok_or(NpcFailure::MissingActor)?;
            if !source.recovery_ready
                || inventory.reserved(actor)
                || source.journal_hold.is_some()
                || source.manager.busy()
                || self
                    .handins
                    .values()
                    .any(|(h, _)| h.request.source == actor)
            {
                continue;
            }
            let Some(props) = world.properties(actor) else {
                continue;
            };
            if !matches!(props.get(PropertyFamily::Int,290),Some(PropertyValue::Int(v))if *v!=0) {
                continue;
            }
            let radius = match props.get(PropertyFamily::Int, 291) {
                Some(PropertyValue::Int(v)) => *v,
                _ => 0,
            };
            let cell = world
                .actor_state(actor)
                .map_err(|_| NpcFailure::MissingActor)?
                .0;
            if cell.0 >> 16 != emitter_cell.0 >> 16 {
                continue;
            }
            if bace_emotes::cylinder_distance(emitter_shape, cylinder(world, actor)?)
                .map_err(|_| NpcFailure::InvalidInput)?
                > f64::from(radius)
            {
                continue;
            }
            let mut identity = scope
                .fork(&actor.0.to_le_bytes(), 0)
                .map_err(|_| NpcFailure::InvalidInput)?;
            let mut recipient_event = [0u8; 16];
            recipient_event[..8].copy_from_slice(
                &identity
                    .next_u64()
                    .map_err(|_| NpcFailure::Capacity)?
                    .to_le_bytes(),
            );
            recipient_event[8..].copy_from_slice(
                &identity
                    .next_u64()
                    .map_err(|_| NpcFailure::Capacity)?
                    .to_le_bytes(),
            );
            let mut random = root
                .event_stream(recipient_event, Domain::Npc)
                .map_err(|_| NpcFailure::InvalidInput)?;
            let draw = (random.next_u64().map_err(|_| NpcFailure::Capacity)? >> 11) as f64
                * (1.0 / 9_007_199_254_740_992.0);
            let operation = source
                .active_operation
                .checked_add(1)
                .ok_or(NpcFailure::Capacity)?;
            let trigger = NativeTrigger {
                category: 37,
                quest: Some(message.clone()),
                ..Default::default()
            };
            let Some(checkpoint) = source
                .manager
                .preview_trigger(
                    &trigger,
                    NpcContext {
                        source: actor,
                        target: Some(emitter),
                        operation,
                    },
                    tick as f64 / 30.0 + source.clock_offset,
                    Some(draw),
                )
                .map_err(|_| NpcFailure::Conflict)?
            else {
                continue;
            };
            let manager = source
                .manager
                .restore_like(checkpoint)
                .map_err(|_| NpcFailure::Conflict)?;
            let mut invocations = source.invocations.clone();
            invocations.retain(|operation, _| {
                self.pending.values().any(|p| {
                    p.proposal.context.source == actor && p.proposal.context.operation == *operation
                })
            });
            if self.pending.values().any(|p| {
                p.proposal.context.source == actor
                    && p.proposal.context.operation == source.active_operation
            }) {
                invocations.insert(
                    source.active_operation,
                    NpcInvocationCheckpoint {
                        operation: source.active_operation,
                        event_id: source.event_id,
                        key_version: source.key_version,
                        random_position: source.random.as_ref().map_or(0, RandomStream::position),
                    },
                );
            }
            if invocations.len() >= 76 || invocations.contains_key(&operation) {
                return Err(NpcFailure::Capacity);
            }
            deliveries.push((
                actor,
                manager,
                random,
                recipient_event,
                operation,
                invocations,
            ));
        }
        let count = deliveries.len();
        let key_version = root.key_version();
        for (actor, manager, random, event, operation, invocations) in deliveries {
            let source = self
                .sources
                .get_mut(&actor)
                .expect("prepared registered listener");
            source.manager = manager;
            source.random = Some(random);
            source.event_id = event;
            source.active_operation = operation;
            source.key_version = key_version;
            source.invocations = invocations;
        }
        Ok(count)
    }
}
