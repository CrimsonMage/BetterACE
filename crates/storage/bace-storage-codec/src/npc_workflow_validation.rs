use crate::{NpcWorkflowSaveV1, SaveCodecError, npc_values_v1::*, npc_workflow_v1::*};
use std::collections::BTreeSet;
fn invalid() -> SaveCodecError {
    SaveCodecError::Invalid("native NPC workflow checkpoint")
}
fn context(c: &NpcContextV1, source: u32) -> bool {
    c.source == source && c.target != Some(0)
}
fn value(v: &NpcValueV1) -> bool {
    match v {
        NpcValueV1::Float(v) => v.is_finite(),
        NpcValueV1::String(s) => s.len() <= 65536,
        _ => true,
    }
}
fn destination(d: &NpcDestinationV1) -> bool {
    d.cell != Some(0)
        && d.position
            .iter()
            .chain(d.rotation.iter())
            .all(|v| v.is_finite())
}
pub(crate) fn operation(op: &NpcOperationV1) -> bool {
    use NpcOperationV1::*;
    match op {
        Text { text, extent, .. } => text.len() <= 65536 && extent.is_finite(),
        Property { mutation, stat, .. } => {
            *stat <= 65535
                && match mutation {
                    NpcPropertyMutationV1::Set { value: v, .. } => v.as_ref().is_none_or(value),
                    _ => true,
                }
        }
        Quest { name, .. } | Event { name, .. } => !name.is_empty() && name.len() <= 256,
        Reward { percent, .. } => percent.is_finite(),
        Give { shade, .. } => shade.is_finite(),
        Motion { extent, .. } | Particle { extent, .. } => extent.is_finite(),
        Move {
            destination: d,
            extent,
            ..
        } => extent.is_finite() && destination(d),
        Turn { rotation, .. } => rotation.iter().all(|v| v.is_finite()),
        TeleportTarget(d) | Sanctuary(d) => destination(d),
        Signal(s) => s.len() <= 65536,
        LockFellow { quest } => quest.as_ref().is_none_or(|s| s.len() <= 256),
        Confirm { key, text } => key.len() <= 256 && text.len() <= 65536,
        _ => true,
    }
}
fn effect(value: &NpcPendingEffectV1) -> bool {
    crate::npc_effects_v1::validate_npc_pending_effect(value).is_ok()
}
pub(crate) fn validate(s: &NpcWorkflowSaveV1) -> Result<(), SaveCodecError> {
    if s.invocation == [0; 16]
        || s.source == 0
        || s.program_hash == [0; 32]
        || s.content_generation == [0; 32]
        || s.event_id == [0; 16]
        || s.key_version == 0
        || !s.logical_now.is_finite()
        || s.logical_now < 0.0
        || s.remaining_instructions > 1_000_000
        || s.scheduled.len() + s.pending.len() + s.detached.len() > 4096
        || s.effects.len() > 4096
    {
        return Err(invalid());
    }
    if s.completed
        && (!s.scheduled.is_empty()
            || !s.pending.is_empty()
            || !s.detached.is_empty()
            || !s.effects.is_empty())
    {
        return Err(invalid());
    }
    let mut invocations = BTreeSet::new();
    if s.invocations.is_empty() || s.invocations.len() > 4096 {
        return Err(invalid());
    }
    for invocation in &s.invocations {
        if !invocations.insert(invocation.operation)
            || invocation.event_id == [0; 16]
            || invocation.key_version != s.key_version
        {
            return Err(invalid());
        }
    }
    let active = s
        .invocations
        .iter()
        .find(|i| i.operation == s.active_operation)
        .ok_or_else(invalid)?;
    if active.event_id != s.event_id || active.random_position != s.random_position {
        return Err(invalid());
    }
    let mut orders = BTreeSet::new();
    for row in s.scheduled.iter().chain(s.pending.iter().map(|p| &p.row)) {
        if row.depth > 128
            || !row.due.is_finite()
            || row.due < 0.0
            || row.order == 0
            || row.order > s.next_order
            || !orders.insert(row.order)
            || !context(&row.context, s.source)
            || !invocations.contains(&row.context.operation)
        {
            return Err(invalid());
        }
    }
    let mut tickets = BTreeSet::new();
    for ticket in s
        .pending
        .iter()
        .map(|p| p.ticket)
        .chain(s.detached.iter().copied())
    {
        if ticket == 0 || !tickets.insert(ticket) {
            return Err(invalid());
        }
    }
    let mut effects = BTreeSet::new();
    for pending in &s.effects {
        if !effects.insert(pending.ticket)
            || !context(&pending.context, s.source)
            || !invocations.contains(&pending.context.operation)
            || !effect(pending)
            || pending.detached != s.detached.contains(&pending.ticket)
        {
            return Err(invalid());
        }
        match pending.completion {
            NpcCompletionV1::Applied { post_delay }
            | NpcCompletionV1::Detached { post_delay, .. }
                if !post_delay.is_finite() || post_delay < 0.0 =>
            {
                return Err(invalid());
            }
            _ => (),
        }
    }
    if tickets != effects {
        return Err(invalid());
    }
    Ok(())
}
