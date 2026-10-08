//! Portal output ordering from pinned Player_Location.Teleport,
//! DoPreTeleportHide and OnTeleportComplete. This does not authorize movement.
use crate::{
    BatchLimits, ReplicationMessage, SequenceKind as Kind, Sequences, SessionBatch,
    SessionProjectionError,
};
use bace_gameplay_api::CharacterBinding;
use bace_wire::{CombatEffect, ObjectControl, PositionPack, PositionUpdate};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalPhase {
    Hide,
    Teleport,
    Materialize,
}
#[derive(Clone, Copy, Debug)]
pub struct PortalView {
    pub object: u32,
    /// Accepted immutable destination at Teleport. Incoming stamps are replaced
    /// by the shared object's sequence owner, never by per-observer counters.
    pub position: PositionPack,
    /// None at Teleport means source physics flags did not change. Materialize
    /// always supplies the owner's final flags, including cloak/report policy.
    pub physics_state: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalBatch {
    pub owner: SessionBatch,
    /// Broadcast once to the authoritative knowledge recipients. F751 is owner
    /// only; F748, physics state and Hide script share identical object stamps.
    pub observers: Vec<ReplicationMessage>,
    /// The world/knowledge owner must invalidate stale forget work for EVERY
    /// recall, including same-cell recalls. Encoding packets is not that mutation.
    pub reset_visibility: bool,
}
pub fn project_portal(
    binding: CharacterBinding,
    phase: PortalPhase,
    view: PortalView,
    sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<PortalBatch, SessionProjectionError> {
    if view.object == 0 || binding.actor.0 != view.object {
        return Err(SessionProjectionError::WrongBinding);
    }
    let mut messages = Vec::new();
    let mut total = 0;
    let mut push =
        |bytes: Vec<u8>| crate::session_output::push(&mut messages, &mut total, 10, bytes, limits);
    let instance = sequences.current(Kind::ObjectInstance, 0);
    match phase {
        PortalPhase::Hide => push(
            CombatEffect::Script {
                object_id: view.object,
                script_id: 0x74,
                speed: 1.0,
            }
            .encode(limits.max_string_bytes, limits.max_message_bytes)?,
        )?,
        PortalPhase::Teleport => {
            let mut pack = view.position;
            if pack.position.cell == 0
                || pack
                    .position
                    .origin
                    .into_iter()
                    .chain(pack.position.rotation)
                    .any(|v| !v.is_finite())
                || pack
                    .velocity
                    .is_some_and(|v| v.iter().any(|x| !x.is_finite()))
            {
                return Err(SessionProjectionError::InvalidProjection);
            }
            let norm = pack.position.rotation.iter().map(|v| v * v).sum::<f32>();
            if !(0.999..=1.001).contains(&norm) {
                return Err(SessionProjectionError::InvalidProjection);
            }
            if view
                .physics_state
                .is_some_and(|state| state & 0x4010 != 0x4010 || state & 8 != 0)
            {
                return Err(SessionProjectionError::InvalidProjection);
            }
            let teleport = sequences.current(Kind::ObjectTeleport, 0).wrapping_add(1);
            pack.instance_sequence = instance;
            pack.teleport_sequence = teleport;
            pack.position_sequence = sequences.current(Kind::ObjectPosition, 0).wrapping_add(1);
            pack.force_position_sequence = sequences.current(Kind::ObjectForcePosition, 0);
            if pack.velocity == Some([0.0; 3]) {
                pack.velocity = None;
            }
            push(ObjectControl::Teleport { sequence: teleport }.encode())?;
            push(
                PositionUpdate {
                    object_id: view.object,
                    pack,
                }
                .encode(),
            )?;
            if let Some(state) = view.physics_state {
                push(
                    ObjectControl::SetState {
                        object_id: view.object,
                        state,
                        instance_sequence: instance,
                        state_sequence: sequences.current(Kind::ObjectState, 0).wrapping_add(1),
                    }
                    .encode(),
                )?;
            }
        }
        PortalPhase::Materialize => {
            let state = view
                .physics_state
                .ok_or(SessionProjectionError::InvalidProjection)?;
            if state & 0x4010 != 0 {
                return Err(SessionProjectionError::InvalidProjection);
            }
            push(
                ObjectControl::SetState {
                    object_id: view.object,
                    state,
                    instance_sequence: instance,
                    state_sequence: sequences.current(Kind::ObjectState, 0).wrapping_add(1),
                }
                .encode(),
            )?;
        }
    }
    match phase {
        PortalPhase::Hide => {}
        PortalPhase::Teleport if view.physics_state.is_some() => {
            sequences
                .advance_batch([
                    (Kind::ObjectTeleport, 0),
                    (Kind::ObjectPosition, 0),
                    (Kind::ObjectState, 0),
                ])
                .map_err(|_| SessionProjectionError::Limit)?;
        }
        PortalPhase::Teleport => {
            sequences
                .advance_batch([(Kind::ObjectTeleport, 0), (Kind::ObjectPosition, 0)])
                .map_err(|_| SessionProjectionError::Limit)?;
        }
        PortalPhase::Materialize => {
            sequences
                .advance_batch([(Kind::ObjectState, 0)])
                .map_err(|_| SessionProjectionError::Limit)?;
        }
    }
    let observers = messages[usize::from(phase == PortalPhase::Teleport)..].to_vec();
    Ok(PortalBatch {
        owner: SessionBatch { binding, messages },
        observers,
        reset_visibility: phase == PortalPhase::Teleport,
    })
}
