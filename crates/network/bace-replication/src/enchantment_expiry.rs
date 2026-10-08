//! ACE EnchantmentManager.Remove: player Remove event or item-owner expiry text,
//! followed by SpellExpire, except player cooldowns. Encode atomically.
use crate::{BatchLimits, EventSequencer, SessionBatch, SessionProjectionError};
use bace_gameplay_api::CharacterBinding;
impl EventSequencer {
    pub fn project_enchantment_expiry(
        &mut self,
        binding: CharacterBinding,
        spell: u16,
        layer: u16,
        item_names: Option<(&str, &str)>,
        sound: bool,
        limits: BatchLimits,
    ) -> Result<SessionBatch, SessionProjectionError> {
        self.check_binding(binding)?;
        let mut messages = Vec::new();
        let mut total = 0;
        let bytes = if let Some((spell_name, item_name)) = item_names {
            let text = format!("The spell {spell_name} on {item_name} has expired.");
            if text.len() > limits.max_string_bytes {
                return Err(SessionProjectionError::Limit);
            }
            bace_wire::ChatMessage::System {
                text: &text,
                chat_type: 7,
            }
            .encode()?
        } else {
            bace_wire::MagicEvent::Remove { spell, layer }.encode(
                binding.actor.0,
                self.next,
                1,
                limits.max_message_bytes,
            )?
        };
        crate::session_output::push(&mut messages, &mut total, 9, bytes, limits)?;
        if sound {
            let bytes = bace_wire::CombatEffect::Sound {
                object_id: binding.actor.0,
                sound_id: 0x96,
                volume: 1.,
            }
            .encode(limits.max_string_bytes, limits.max_message_bytes)?;
            crate::session_output::push(&mut messages, &mut total, 10, bytes, limits)?;
        }
        if item_names.is_none() {
            self.next = self.next.wrapping_add(1);
        }
        Ok(SessionBatch { binding, messages })
    }
}
