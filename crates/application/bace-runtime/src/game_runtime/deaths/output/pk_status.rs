//! Pinned ACE Player_Death.cs PK_DeathTick broadcasts PropertyInt 134, then
//! sends the restored PK/PKLite error privately. Other status transitions have
//! different source effects and remain retained by this projector.
use super::*;
use bace_gameplay_api::CharacterBinding;
use bace_replication::{EventSequencer, InventoryProjection as P};
use bace_wire::{ObjectCodecLimits, PropertyValue, SimpleGameEvent};
use std::collections::BTreeMap;

fn steps(actor: EntityId, status: u32) -> Option<[P<'static>; 2]> {
    let error = match status {
        4 => 0x04F1,  // YouArePKAgain
        64 => 0x0508, // YouAreNowPKLite
        _ => return None,
    };
    Some([
        P::Property {
            item: actor,
            property: 134, // PlayerKillerStatus
            value: PropertyValue::Int(status as i32),
        },
        P::Simple(SimpleGameEvent::WeenieError(error)),
    ])
}

impl GameRuntime {
    pub(super) fn project_death_pk_status(
        &mut self,
        actor: EntityId,
        status: u32,
        recipient: Option<CharacterBinding>,
    ) -> Result<bool, String> {
        let Some(steps) = steps(actor, status) else {
            return Ok(false);
        };
        let room = self.network_output.len() < self.limits.messages;
        let key = match self.death_private_admission(actor, recipient, room)? {
            protection::Admission::Detached => return Ok(true),
            protection::Admission::Retain => return Ok(false),
            protection::Admission::Publish(key) => key,
        };
        let binding = recipient.expect("admitted PK recipient");
        let message_bytes = self.limits.message_bytes;
        let objects = ObjectCodecLimits {
            max_message_bytes: message_bytes,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 4096,
        };
        let limits = BatchLimits {
            max_messages: 2,
            max_bytes: message_bytes.saturating_mul(2),
            max_message_bytes: message_bytes,
            max_string_bytes: 4096,
        };
        // Preflight the complete public/private pair and observer capacity on
        // copied counters. No canonical sequence advances under pressure.
        let preview = {
            let replica = self
                .players
                .replication(actor)
                .ok_or("PK respite canonical owner disappeared")?;
            if replica.binding != binding || replica.key != key {
                return Err("PK respite recipient binding changed".into());
            }
            let mut counters = replica.properties.proposal_copy();
            let mut events = EventSequencer::new(binding, replica.events.next_sequence());
            events
                .project_inventory_with_actor(
                    binding,
                    &steps,
                    &mut BTreeMap::new(),
                    Some(&mut counters),
                    objects,
                    limits,
                )
                .map_err(|error| format!("PK respite preflight: {error:?}"))?
        };
        let [public, _private] = preview.messages.as_slice() else {
            return Err("PK respite projection count".into());
        };
        if !self.observer_room(1, public.bytes.len()) {
            return Ok(false);
        }
        crate::game_messages::session_batch_command(key, preview)
            .map_err(|error| format!("PK respite batch preflight: {error}"))?;
        let replica = self
            .players
            .replication(actor)
            .ok_or("PK respite canonical owner disappeared")?;
        let batch = replica
            .events
            .project_inventory_with_actor(
                binding,
                &steps,
                &mut replica.item_properties,
                Some(&mut replica.properties),
                objects,
                limits,
            )
            .map_err(|error| format!("PK respite projection: {error:?}"))?;
        let public = batch.messages[0].clone();
        self.retain_observer_messages(vec![(actor, vec![public])])?;
        self.network_output.push_back(
            crate::game_messages::session_batch_command(key, batch)
                .map_err(|error| error.to_string())?,
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_gameplay_api::SessionId;
    use bace_replication::{SequenceKind, Sequences};
    use bace_types::AccountId;

    #[test]
    fn source_pk_restoration_orders_public_property_before_private_error() {
        let actor = EntityId(0x5000_0001);
        let binding = CharacterBinding {
            actor,
            account: AccountId(1),
            session: SessionId(7),
        };
        let objects = ObjectCodecLimits {
            max_message_bytes: 1024,
            max_model_entries: 255,
            max_children: 128,
            max_restrictions: 1024,
            max_motion_commands: 32,
            max_string_bytes: 4096,
        };
        for (status, error) in [(4u32, 0x04F1u32), (64, 0x0508)] {
            let mut events = EventSequencer::new(binding, 12);
            let mut actor_sequences = Sequences::new(256).unwrap();
            let batch = events
                .project_inventory_with_actor(
                    binding,
                    &steps(actor, status).unwrap(),
                    &mut BTreeMap::new(),
                    Some(&mut actor_sequences),
                    objects,
                    BatchLimits {
                        max_messages: 2,
                        max_bytes: 2048,
                        max_message_bytes: 1024,
                        max_string_bytes: 4096,
                    },
                )
                .unwrap();
            assert_eq!(batch.messages.len(), 2);
            assert_eq!(batch.messages[0].queue, 9);
            assert_eq!(&batch.messages[0].bytes[..4], &0x02CEu32.to_le_bytes());
            assert_eq!(&batch.messages[0].bytes[5..9], &actor.0.to_le_bytes());
            assert_eq!(&batch.messages[0].bytes[9..13], &134u32.to_le_bytes());
            assert_eq!(&batch.messages[0].bytes[13..17], &status.to_le_bytes());
            assert_eq!(batch.messages[1].queue, 9);
            assert_eq!(&batch.messages[1].bytes[0..4], &0xF7B0u32.to_le_bytes());
            assert_eq!(&batch.messages[1].bytes[8..12], &12u32.to_le_bytes());
            assert_eq!(&batch.messages[1].bytes[12..16], &0x028Au32.to_le_bytes());
            assert_eq!(&batch.messages[1].bytes[16..20], &error.to_le_bytes());
            assert_eq!(events.next_sequence(), 13);
        }
        assert!(steps(actor, 2).is_none());
        let mut events = EventSequencer::new(binding, 12);
        let mut actor_sequences = Sequences::new(256).unwrap();
        assert!(
            events
                .project_inventory_with_actor(
                    binding,
                    &steps(actor, 4).unwrap(),
                    &mut BTreeMap::new(),
                    Some(&mut actor_sequences),
                    objects,
                    BatchLimits {
                        max_messages: 2,
                        max_bytes: 16,
                        max_message_bytes: 1024,
                        max_string_bytes: 4096,
                    },
                )
                .is_err()
        );
        assert_eq!(events.next_sequence(), 12);
        assert_eq!(actor_sequences.current(SequenceKind::PropertyInt, 134), 255);
    }
}
