//! Source staff spellbook notifications; removal intentionally has no invented
//! MagicRemoveSpell packet (ACE RemoveKnownSpell only sends system text).
use crate::{
    BatchLimits, EventSequencer, ReplicationMessage, SessionBatch, SessionProjectionError as E,
};
use bace_gameplay_api::{CharacterBinding, staff::StaffEvent};
use bace_wire::{ChatMessage, CombatEffect, MagicEvent};
impl EventSequencer {
    pub fn project_staff_spellbook(
        &mut self,
        binding: CharacterBinding,
        event: &StaffEvent,
        limits: BatchLimits,
    ) -> Result<SessionBatch, E> {
        let StaffEvent::Spellbook {
            context,
            spell,
            name,
            learn,
            changed,
            ..
        } = event
        else {
            return Err(E::InvalidProjection);
        };
        if self.binding != binding
            || binding.actor != context.actor
            || binding.account != context.account
            || binding.session != context.session
        {
            return Err(E::WrongBinding);
        }
        if *spell == 0 || *spell > 65535 || name.len() > 1024 {
            return Err(E::InvalidProjection);
        }
        let mut messages = vec![];
        let mut bytes = 0;
        if *learn && *changed {
            crate::session_output::push(
                &mut messages,
                &mut bytes,
                9,
                MagicEvent::UpdateSpell {
                    spell: *spell as u16,
                    layer: 0,
                }
                .encode(binding.actor.0, self.next, 1, limits.max_message_bytes)?,
                limits,
            )?;
            crate::session_output::push(
                &mut messages,
                &mut bytes,
                10,
                CombatEffect::Script {
                    object_id: binding.actor.0,
                    script_id: 0x1c,
                    speed: 1.,
                }
                .encode(limits.max_string_bytes, limits.max_message_bytes)?,
                limits,
            )?;
        }
        let text = match (*learn, *changed) {
            (true, true) => format!("You learn the {name} spell.\n"),
            (true, false) => "You already know that spell!".into(),
            (false, true) => format!("{name} removed from spellbook."),
            (false, false) => "You don't know that spell!".into(),
        };
        if text.len() > limits.max_string_bytes {
            return Err(E::Limit);
        }
        crate::session_output::push(
            &mut messages,
            &mut bytes,
            9,
            ChatMessage::System {
                text: &text,
                chat_type: 0,
            }
            .encode()?,
            limits,
        )?;
        if *learn && *changed {
            self.next = self.next.wrapping_add(1);
        }
        Ok(SessionBatch { binding, messages })
    }
}
/// Observer routing remains the accepted interest owner's responsibility.
pub fn project_staff_scripts(
    event: &StaffEvent,
    limits: BatchLimits,
) -> Result<Vec<ReplicationMessage>, E> {
    let StaffEvent::Scripts { targets, .. } = event else {
        return Err(E::InvalidProjection);
    };
    if targets.len() > 4096 {
        return Err(E::Limit);
    }
    let mut messages = vec![];
    let mut bytes = 0;
    for &(target, script) in targets {
        if target.0 == 0 {
            return Err(E::InvalidProjection);
        }
        crate::session_output::push(
            &mut messages,
            &mut bytes,
            10,
            CombatEffect::Script {
                object_id: target.0,
                script_id: script,
                speed: 1.,
            }
            .encode(limits.max_string_bytes, limits.max_message_bytes)?,
            limits,
        )?;
    }
    Ok(messages)
}
