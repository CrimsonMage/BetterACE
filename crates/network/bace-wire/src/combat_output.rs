//! Pinned ACE combat notifications. Timing, damage and authority are caller-owned.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::{GameEventType as Event, GameMessageOpcode as Message};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageNotification<'a> {
    pub name: &'a str,
    pub damage_type: u32,
    /// ACE accepts float then widens to double on the wire.
    pub percent: f32,
    pub damage: u32,
    pub critical: bool,
    pub conditions: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEvent<'a> {
    AttackDone(u32),
    CommenceAttack,
    Attacker(DamageNotification<'a>),
    Defender {
        damage: DamageNotification<'a>,
        location: u32,
    },
    EvadedBy(&'a str),
    EvadedAttackFrom(&'a str),
    UpdateHealth {
        target_id: u32,
        fraction: f32,
    },
    KillerNotification(&'a str),
    VictimNotification(&'a str),
}
impl CombatEvent<'_> {
    pub fn encode(
        &self,
        object_id: u32,
        sequence: u32,
        max_string_bytes: usize,
        max_message_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        let event = match self {
            Self::AttackDone(_) => Event::AttackDone,
            Self::CommenceAttack => Event::CombatCommenceAttack,
            Self::Attacker(_) => Event::AttackerNotification,
            Self::Defender { .. } => Event::DefenderNotification,
            Self::EvadedBy(_) => Event::EvasionAttackerNotification,
            Self::EvadedAttackFrom(_) => Event::EvasionDefenderNotification,
            Self::UpdateHealth { .. } => Event::UpdateHealth,
            Self::KillerNotification(_) => Event::KillerNotification,
            Self::VictimNotification(_) => Event::VictimNotification,
        };
        let mut writer = message_writer(Message::GameEvent);
        for word in [object_id, sequence, event.0] {
            writer.u32(word);
        }
        match self {
            Self::AttackDone(error) => writer.u32(*error),
            Self::CommenceAttack => {}
            Self::Attacker(damage) | Self::Defender { damage, .. } => {
                bounded_string(&mut writer, damage.name, max_string_bytes)?;
                writer.u32(damage.damage_type);
                writer.f64(f64::from(damage.percent));
                writer.u32(damage.damage);
                if let Self::Defender { location, .. } = self {
                    writer.u32(*location);
                }
                writer.u32(u32::from(damage.critical));
                writer.u64(damage.conditions);
                if matches!(self, Self::Defender { .. }) {
                    writer.align4();
                }
            }
            Self::UpdateHealth {
                target_id,
                fraction,
            } => {
                writer.u32(*target_id);
                writer.f32(*fraction);
            }
            Self::EvadedBy(name)
            | Self::EvadedAttackFrom(name)
            | Self::KillerNotification(name)
            | Self::VictimNotification(name) => {
                bounded_string(&mut writer, name, max_string_bytes)?
            }
        }
        let bytes = writer.into_bytes();
        if bytes.len() > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CombatEffect<'a> {
    Sound {
        object_id: u32,
        sound_id: u32,
        volume: f32,
    },
    Script {
        object_id: u32,
        script_id: u32,
        speed: f32,
    },
    PlayerKilled {
        text: &'a str,
        victim_id: u32,
        killer_id: u32,
    },
}
impl CombatEffect<'_> {
    pub fn encode(
        &self,
        max_string_bytes: usize,
        max_message_bytes: usize,
    ) -> Result<Vec<u8>, WireError> {
        let opcode = match self {
            Self::Sound { .. } => Message::Sound,
            Self::Script { .. } => Message::PlayEffect,
            Self::PlayerKilled { .. } => Message::PlayerKilled,
        };
        let mut writer = message_writer(opcode);
        match self {
            Self::Sound {
                object_id,
                sound_id,
                volume,
            } => {
                writer.u32(*object_id);
                writer.u32(*sound_id);
                writer.f32(*volume);
            }
            Self::Script {
                object_id,
                script_id,
                speed,
            } => {
                writer.u32(*object_id);
                writer.u32(*script_id);
                writer.f32(*speed);
            }
            Self::PlayerKilled {
                text,
                victim_id,
                killer_id,
            } => {
                bounded_string(&mut writer, text, max_string_bytes)?;
                writer.u32(*victim_id);
                writer.u32(*killer_id);
            }
        }
        let bytes = writer.into_bytes();
        if bytes.len() > max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(bytes)
    }
}

fn bounded_string(writer: &mut crate::Writer, text: &str, limit: usize) -> Result<(), WireError> {
    if text.chars().count() > limit {
        return Err(WireError::LimitExceeded);
    }
    writer.string16(text)
}
