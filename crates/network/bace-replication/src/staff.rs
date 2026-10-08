//! Staff projections use existing authoritative sequence owners. Returned batches
//! stay caller-owned until the entire reliable batch is admitted.
use crate::{
    BatchLimits, PortalBatch, PortalPhase, PortalView, SequenceKind, Sequences, SessionBatch,
    SessionProjectionError,
};
use bace_gameplay_api::{CharacterBinding, staff::StaffEvent};
use bace_wire::{ChatMessage, CurrentVitalUpdate, PositionPack};
/// Pinned HandleGamecast/HandleGameCastEmote use WorldBroadcast (0x14), UI queue 9.
pub fn project_staff_broadcast(
    text: &str,
    maximum_bytes: usize,
) -> Result<Vec<u8>, SessionProjectionError> {
    if text.len() > 4096 {
        return Err(SessionProjectionError::Limit);
    }
    let bytes = ChatMessage::System {
        text,
        chat_type: 0x14,
    }
    .encode()?;
    if bytes.len() > maximum_bytes {
        return Err(SessionProjectionError::Limit);
    }
    Ok(bytes)
}
pub fn project_staff_text(
    binding: CharacterBinding,
    event: &StaffEvent,
    limits: BatchLimits,
) -> Result<SessionBatch, SessionProjectionError> {
    let StaffEvent::Inspection { context, lines, .. } = event else {
        return Err(SessionProjectionError::InvalidProjection);
    };
    if binding.actor != context.actor
        || binding.account != context.account
        || binding.session != context.session
    {
        return Err(SessionProjectionError::WrongBinding);
    }
    let mut messages = Vec::new();
    let mut total = 0;
    for line in lines {
        if line.len() > limits.max_string_bytes {
            return Err(SessionProjectionError::Limit);
        }
        crate::session_output::push(
            &mut messages,
            &mut total,
            9,
            ChatMessage::System {
                text: line,
                chat_type: 0,
            }
            .encode()?,
            limits,
        )?;
    }
    Ok(SessionBatch { binding, messages })
}
pub fn project_staff_heal(
    binding: CharacterBinding,
    event: &StaffEvent,
    sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<SessionBatch, SessionProjectionError> {
    let StaffEvent::Healed { target, vitals, .. } = event else {
        return Err(SessionProjectionError::InvalidProjection);
    };
    if binding.actor != *target {
        return Err(SessionProjectionError::WrongBinding);
    }
    if vitals.len() != 3 || vitals.iter().map(|p| p.0).collect::<Vec<_>>() != [2, 4, 6] {
        return Err(SessionProjectionError::InvalidProjection);
    }
    let mut messages = Vec::new();
    let mut total = 0;
    for &(id, current) in vitals {
        let id = u32::from(id);
        let sequence = (sequences.current(SequenceKind::Vital, id) as u8).wrapping_add(1);
        crate::session_output::push(
            &mut messages,
            &mut total,
            9,
            CurrentVitalUpdate {
                sequence,
                vital: id,
                current,
            }
            .encode(),
            limits,
        )?;
    }
    sequences
        .advance_batch([
            (SequenceKind::Vital, 2),
            (SequenceKind::Vital, 4),
            (SequenceKind::Vital, 6),
        ])
        .map_err(|_| SessionProjectionError::Limit)?;
    Ok(SessionBatch { binding, messages })
}
/// Reuse the pinned teleport packet ordering and observer visibility reset.
/// Hide/materialize phases continue through the existing portal lifecycle owner.
pub fn project_staff_teleport(
    binding: CharacterBinding,
    event: &StaffEvent,
    position: PositionPack,
    physics_state: Option<u32>,
    sequences: &mut Sequences,
    limits: BatchLimits,
) -> Result<PortalBatch, SessionProjectionError> {
    let StaffEvent::Teleported { target, after, .. } = event else {
        return Err(SessionProjectionError::InvalidProjection);
    };
    if binding.actor != *target {
        return Err(SessionProjectionError::WrongBinding);
    }
    if position.position.cell != after.cell
        || position.position.origin != after.origin
        || position.position.rotation != after.rotation
    {
        return Err(SessionProjectionError::InvalidProjection);
    }
    crate::project_portal(
        binding,
        PortalPhase::Teleport,
        PortalView {
            object: target.0,
            position,
            physics_state,
        },
        sequences,
        limits,
    )
}

/// AccountCommands emits private System messages with Broadcast/WorldBroadcast types.
pub fn project_staff_response(
    binding: CharacterBinding,
    text: &str,
    chat_type: u32,
    limits: BatchLimits,
) -> Result<SessionBatch, SessionProjectionError> {
    if text.len() > limits.max_string_bytes || !matches!(chat_type, 0 | 20) {
        return Err(SessionProjectionError::Limit);
    }
    let mut messages = Vec::new();
    let mut total = 0;
    crate::session_output::push(
        &mut messages,
        &mut total,
        9,
        ChatMessage::System { text, chat_type }.encode()?,
        limits,
    )?;
    Ok(SessionBatch { binding, messages })
}
