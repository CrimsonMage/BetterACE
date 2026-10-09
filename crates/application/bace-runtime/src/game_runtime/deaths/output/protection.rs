//! Pinned ACE Player_Death.cs LifestoneProtectionTick/Dispel send one private
//! GameMessageSystemChat on the Magic channel. The source event freezes its
//! entered binding so an old notice cannot reach a later login generation.
use super::*;
use bace_gameplay_api::CharacterBinding;
use bace_replication::ReplicationMessage;
use bace_wire::ChatMessage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Expired,
    Dispelled,
}

impl Kind {
    fn text(self) -> &'static str {
        match self {
            Self::Expired => "You're no longer protected by the Lifestone's magic!",
            Self::Dispelled => "Your actions have dispelled the Lifestone's magic!",
        }
    }

    fn message(self, max_bytes: usize) -> Result<ReplicationMessage, String> {
        let bytes = ChatMessage::System {
            text: self.text(),
            chat_type: 7, // ACE ChatMessageType.Magic
        }
        .encode()
        .map_err(|error| format!("death protection notice codec: {error:?}"))?;
        if bytes.len() > max_bytes {
            return Err("death protection notice byte limit".into());
        }
        Ok(ReplicationMessage { queue: 9, bytes })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Admission {
    Publish(SessionKey),
    Retain,
    Detached,
}

pub(super) fn admission(
    expected: Option<CharacterBinding>,
    current: Option<(SessionKey, CharacterBinding, bool)>,
    loading_expected: bool,
    output_room: bool,
) -> Admission {
    let Some(expected) = expected else {
        return Admission::Detached;
    };
    match current {
        Some((key, binding, connected)) if binding == expected => {
            if connected && output_room {
                Admission::Publish(key)
            } else {
                Admission::Retain
            }
        }
        Some(_) => Admission::Detached,
        None if loading_expected => Admission::Retain,
        None => Admission::Detached,
    }
}

impl GameRuntime {
    pub(super) fn death_private_admission(
        &mut self,
        actor: EntityId,
        expected: Option<CharacterBinding>,
        room: bool,
    ) -> Result<Admission, String> {
        if expected.is_some_and(|binding| binding.actor != actor) {
            return Err("death private recipient actor mismatch".into());
        }
        let current = self.players.replication(actor).map(|replica| {
            let connected = self.sessions.get(&replica.key).is_some_and(|session| {
                !session.disconnected
                    && session
                        .loading
                        .as_ref()
                        .is_some_and(|loading| loading.loaded.binding == replica.binding)
            });
            (replica.key, replica.binding, connected)
        });
        let loading_expected = expected.is_some_and(|binding| {
            self.sessions.values().any(|session| {
                !session.disconnected
                    && session
                        .loading
                        .as_ref()
                        .is_some_and(|loading| loading.loaded.binding == binding)
            })
        });
        Ok(admission(expected, current, loading_expected, room))
    }

    pub(super) fn project_death_protection_notice(
        &mut self,
        actor: EntityId,
        expected: Option<CharacterBinding>,
        kind: Kind,
    ) -> Result<bool, String> {
        let room = self.network_output.len() < self.limits.messages;
        match self.death_private_admission(actor, expected, room)? {
            Admission::Detached => Ok(true),
            Admission::Retain => Ok(false),
            Admission::Publish(key) => {
                let binding = expected.expect("admitted protection binding");
                let batch = SessionBatch {
                    binding,
                    messages: vec![kind.message(self.limits.message_bytes)?],
                };
                let command = crate::game_messages::session_batch_command(key, batch)
                    .map_err(|error| error.to_string())?;
                self.network_output.push_back(command);
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_gameplay_api::SessionId;
    use bace_types::AccountId;

    fn binding(generation: u64) -> CharacterBinding {
        CharacterBinding {
            actor: EntityId(0x5000_0001),
            account: AccountId(4),
            session: SessionId(generation),
        }
    }

    #[test]
    fn pinned_magic_notice_has_exact_server_message_layout_and_bounded_queue() {
        for kind in [Kind::Expired, Kind::Dispelled] {
            let message = kind.message(1024).unwrap();
            let text = kind.text().as_bytes();
            let mut expected = vec![0xE0, 0xF7, 0, 0]; // ServerMessage 0xF7E0
            expected.extend_from_slice(&(text.len() as u16).to_le_bytes());
            expected.extend_from_slice(text);
            while expected.len() % 4 != 0 {
                expected.push(0);
            }
            expected.extend_from_slice(&7u32.to_le_bytes()); // Magic
            assert_eq!(message.queue, 9);
            assert_eq!(message.bytes, expected);
            assert!(kind.message(expected.len() - 1).is_err());
        }
    }

    #[test]
    fn protection_notice_retains_pressure_and_disconnect_but_not_new_login() {
        let old = binding(7);
        let next = binding(8);
        let key = SessionKey {
            id: 1,
            generation: 7,
        };
        assert_eq!(
            admission(Some(old), Some((key, old, true)), true, false),
            Admission::Retain
        );
        assert_eq!(
            admission(Some(old), Some((key, old, false)), false, true),
            Admission::Retain
        );
        assert_eq!(admission(Some(old), None, true, true), Admission::Retain);
        assert_eq!(
            admission(Some(old), Some((key, old, true)), true, true),
            Admission::Publish(key)
        );
        assert_eq!(admission(Some(old), None, false, true), Admission::Detached);
        assert_eq!(
            admission(Some(old), Some((key, next, true)), false, true),
            Admission::Detached
        );
        assert_eq!(
            admission(None, Some((key, next, true)), false, true),
            Admission::Detached
        );
    }
}
