use crate::envelope::message_writer;
use crate::opcode::{GameEventType as Event, GameMessageOpcode};
use crate::{Enchantment, WireError};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MagicEvent<'a> {
    UpdateSpell { spell: u16, layer: u16 },
    UpdateEnchantment(&'a Enchantment),
    UpdateMultiple(&'a [Enchantment]),
    Remove { spell: u16, layer: u16 },
    RemoveMultiple(&'a [(u16, u16)]),
    Dispel { spell: u16, layer: u16 },
    DispelMultiple(&'a [(u16, u16)]),
    Purge,
    PurgeBad,
}
impl MagicEvent<'_> {
    pub fn encode(
        &self,
        actor: u32,
        sequence: u32,
        max_entries: usize,
        max_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        let event = match self {
            Self::UpdateSpell { .. } => Event::MagicUpdateSpell,
            Self::UpdateEnchantment(_) => Event::MagicUpdateEnchantment,
            Self::UpdateMultiple(_) => Event::MagicUpdateMultipleEnchantments,
            Self::Remove { .. } => Event::MagicRemoveEnchantment,
            Self::RemoveMultiple(_) => Event::MagicRemoveMultipleEnchantments,
            Self::Dispel { .. } => Event::MagicDispelEnchantment,
            Self::DispelMultiple(_) => Event::MagicDispelMultipleEnchantments,
            Self::Purge => Event::MagicPurgeEnchantments,
            Self::PurgeBad => Event::MagicPurgeBadEnchantments,
        };
        let mut w = message_writer(GameMessageOpcode::GameEvent);
        for value in [actor, sequence, event.0] {
            w.u32(value);
        }
        match self {
            Self::UpdateSpell { spell, layer }
            | Self::Remove { spell, layer }
            | Self::Dispel { spell, layer } => {
                w.u16(*spell);
                w.u16(*layer);
            }
            Self::UpdateEnchantment(e) => e.write(&mut w)?,
            Self::UpdateMultiple(entries) => {
                crate::enchantment::write_enchantments(&mut w, entries, max_entries)?
            }
            Self::RemoveMultiple(entries) | Self::DispelMultiple(entries) => {
                if entries.len() > max_entries || entries.len() > u32::MAX as usize {
                    return Err(WireError::LimitExceeded);
                }
                w.u32(entries.len() as u32);
                for (spell, layer) in *entries {
                    w.u16(*spell);
                    w.u16(*layer);
                }
            }
            Self::Purge | Self::PurgeBad => {}
        }
        let bytes = w.into_bytes();
        if bytes.len() > max_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(bytes)
    }
}
