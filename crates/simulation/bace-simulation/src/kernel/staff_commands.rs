//! Typed staff ingress. Runtime queue admission is not execution acknowledgment.
use super::*;
use bace_gameplay_api::staff::{StaffAction as A, StaffCommand, StaffError, StaffEvent};
impl Kernel {
    pub fn apply_staff_command(&mut self, command: StaffCommand) -> Result<(), Box<StaffCommand>> {
        if matches!(&command.action, A::QueryTarget { .. }) {
            self.drain_health_observations();
        }
        // One action projection plus one correlated outcome. Keep the original
        // command queued when output pressure prevents complete publication.
        let needed = if matches!(
            &command.action,
            A::Register(_) | A::Refresh(_) | A::Remove(_)
        ) {
            1
        } else {
            2
        };
        if !self.staff.room(needed.min(self.staff.capacity())) {
            return Err(Box::new(command));
        }
        if matches!(&command.action, A::GagCommitted(_)) && !self.gag_commit_room() {
            return Err(Box::new(command));
        }
        if matches!(&command.action, A::GagRejected(_))
            && self.social.events.len() >= self.social.capacity
        {
            return Err(Box::new(command));
        }
        if let A::Audit { texts, .. } = &command.action
            && self.social.events.len().saturating_add(texts.len()) > self.social.capacity
        {
            return Err(Box::new(command));
        }
        if let A::Gag { target, .. } = &command.action
            && self
                .social_gags
                .states
                .get(&target.character)
                .is_some_and(|s| s.pending_interval.is_some())
        {
            return Err(Box::new(command));
        }
        let actor = match &command.action {
            A::Gag { context, .. } | A::QueryTarget { context, .. } => Some(context.actor),
            A::GagCommitted(p) | A::GagRejected(p) => Some(p.context.actor),
            A::Register(r) | A::Refresh(r) => Some(r.binding.actor),
            A::Remove(b) => Some(b.actor),
            A::SpellCommitted(ticket) | A::SpellRejected(ticket) => Some(ticket.context.actor),
            A::Spellbook { context, .. } | A::Buff { context, .. } => Some(context.actor),
            A::Broadcast { context, .. }
            | A::Audit { context, .. }
            | A::Regenerate { context, .. }
            | A::GrantExperience { context, .. }
            | A::MapTeleport { context, .. }
            | A::Teleport { context, .. }
            | A::Inspect { context, .. }
            | A::Run { context, .. }
            | A::CastSpell { context, .. }
            | A::Heal { context, .. } => Some(context.actor),
        };
        let proposal = matches!(
            &command.action,
            A::Spellbook { .. } | A::GrantExperience { .. } | A::Gag { .. }
        );
        let result = if needed > self.staff.capacity() {
            Err(StaffError::Capacity)
        } else if command.token == 0 {
            Err(StaffError::Invalid)
        } else {
            match command.action {
                A::QueryTarget {
                    context,
                    kind,
                    target,
                    mana,
                } => self.query_staff_target(context, kind, target, mana),
                A::Gag {
                    context,
                    target,
                    requested_name,
                    enabled,
                    unix_seconds,
                    sudo,
                } => self.prepare_staff_gag(
                    context,
                    command.token,
                    target,
                    requested_name,
                    enabled,
                    unix_seconds,
                    sudo,
                ),
                A::GagCommitted(p) => {
                    if p.operation == command.token {
                        self.complete_staff_gag(&p, true)
                    } else {
                        Err(StaffError::Invalid)
                    }
                }
                A::GagRejected(p) => {
                    if p.operation == command.token {
                        self.complete_staff_gag(&p, false)
                            .and(Err(StaffError::Stale))
                    } else {
                        Err(StaffError::Invalid)
                    }
                }
                A::Regenerate {
                    context,
                    target,
                    sudo,
                } => self.staff_regenerate(context, target, sudo),
                A::Broadcast {
                    context,
                    text,
                    emote,
                    local,
                    sudo,
                } => self.staff_broadcast(context, command.token, text, emote, local, sudo),
                A::Audit {
                    context,
                    texts,
                    sudo,
                } => self.staff_audit(context, texts, sudo),
                A::GrantExperience {
                    context,
                    target,
                    amount,
                    sudo,
                } => self.staff_grant_experience(context, target, amount, command.token, sudo),
                A::Buff {
                    context,
                    target,
                    fellowship,
                    maximum_level,
                    equipment,
                    sudo,
                } => self.staff_buff(context, target, fellowship, maximum_level, equipment, sudo),
                A::CastSpell {
                    context,
                    target,
                    spell,
                    sudo,
                } => self.staff_cast_spell(context, target, spell, command.token, sudo),
                A::Run {
                    context,
                    mode,
                    sudo,
                } => self.staff_run(context, mode, sudo),
                A::Spellbook {
                    context,
                    spell,
                    learn,
                    sudo,
                } => self
                    .prepare_staff_spellbook(context, command.token, spell, learn, sudo)
                    .map(|_| ()),
                A::SpellCommitted(ticket) => {
                    if ticket.operation == command.token {
                        self.confirm_staff_spellbook(&ticket)
                    } else {
                        Err(StaffError::Invalid)
                    }
                }
                A::SpellRejected(ticket) => {
                    if ticket.operation == command.token {
                        self.reject_staff_spellbook(&ticket)
                            .and(Err(StaffError::Stale))
                    } else {
                        Err(StaffError::Invalid)
                    }
                }
                A::Register(r) => self.register_staff(r),
                A::Refresh(r) => self.refresh_staff(r),
                A::Remove(b) => self.remove_staff(b),
                A::MapTeleport {
                    context,
                    request,
                    prepared,
                } => self.staff_map_teleport(context, request, prepared),
                A::Teleport {
                    context,
                    target,
                    destination,
                    expected_epoch,
                    kind,
                    sudo,
                } => self.staff_teleport(context, target, destination, expected_epoch, kind, sudo),
                A::Inspect {
                    context,
                    target,
                    kind,
                    sudo,
                } => self.staff_inspect(context, target, kind, sudo),
                A::Heal {
                    context,
                    target,
                    target_name,
                    sudo,
                } => self.staff_heal(context, target, target_name.as_deref(), sudo),
            }
        };
        if proposal && result.is_ok() {
            return Ok(());
        }
        self.staff.push(StaffEvent::Outcome {
            token: command.token,
            actor,
            result,
        });
        Ok(())
    }
}
