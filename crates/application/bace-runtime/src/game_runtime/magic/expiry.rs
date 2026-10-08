//! Exact owner and item name are frozen by the accepted depletion event.
use super::*;
impl GameRuntime {
    pub(super) fn project_magic_expiry(
        &mut self,
        event: &bace_simulation::MagicEvent,
    ) -> Result<bool, String> {
        let bace_simulation::MagicEvent::EnchantmentExpired {
            recipient,
            spell,
            layer,
            item_name,
            sound,
            ..
        } = event
        else {
            return Err("expiry event kind".into());
        };
        let Some((key, table)) = self.sessions.iter().find_map(|(key, s)| {
            s.loading
                .as_ref()
                .filter(|l| l.loaded.binding.actor == *recipient)
                .and_then(|l| l.spell_table.clone())
                .map(|t| (*key, t))
        }) else {
            return Ok(true);
        };
        if self.sessions[&key].disconnected {
            return Ok(true);
        }
        if self.network_output.len() >= self.limits.messages {
            return Ok(false);
        }
        let names = if let Some(item_name) = item_name {
            Some((
                table
                    .spells
                    .get(spell)
                    .ok_or("expiry spell name missing")?
                    .name
                    .as_str(),
                item_name.as_str(),
            ))
        } else {
            None
        };
        let replica = self
            .players
            .replication(*recipient)
            .ok_or("expiry replication missing")?;
        let batch = replica
            .events
            .project_enchantment_expiry(
                replica.binding,
                u16::try_from(*spell).map_err(|_| "expiry spell overflow")?,
                *layer,
                names,
                *sound,
                bace_replication::BatchLimits {
                    max_messages: 2,
                    max_bytes: self.limits.message_bytes.saturating_mul(2),
                    max_message_bytes: self.limits.message_bytes,
                    max_string_bytes: 1024,
                },
            )
            .map_err(|e| format!("expiry projection: {e:?}"))?;
        self.network_output
            .push_back(NetworkCommand::SendOrderedBatch {
                key,
                messages: batch
                    .messages
                    .into_iter()
                    .map(|m| (m.queue, m.bytes))
                    .collect(),
            });
        Ok(true)
    }
}
