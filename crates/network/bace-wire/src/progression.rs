//! Pinned ACE progression output. No pricing, skill or authority policy lives here.
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttributeUpdate {
    pub sequence: u8,
    pub attribute: u32,
    pub ranks: u32,
    pub starting_value: u32,
    pub experience_spent: u32,
}
impl AttributeUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::PrivateUpdateAttribute);
        writer.bytes(&[self.sequence]);
        for value in [
            self.attribute,
            self.ranks,
            self.starting_value,
            self.experience_spent,
        ] {
            writer.u32(value);
        }
        writer.into_bytes()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VitalUpdate {
    pub sequence: u8,
    pub object_id: Option<u32>,
    pub vital: u32,
    pub ranks: u32,
    pub starting_value: u32,
    pub experience_spent: u32,
    pub current: u32,
}
impl VitalUpdate {
    pub fn encode(self) -> Vec<u8> {
        let opcode = if self.object_id.is_some() {
            GameMessageOpcode::PublicUpdateVital
        } else {
            GameMessageOpcode::PrivateUpdateVital
        };
        let mut writer = message_writer(opcode);
        writer.bytes(&[self.sequence]);
        if let Some(id) = self.object_id {
            writer.u32(id);
        }
        for value in [
            self.vital,
            self.ranks,
            self.starting_value,
            self.experience_spent,
            self.current,
        ] {
            writer.u32(value);
        }
        writer.into_bytes()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkillUpdate {
    pub sequence: u8,
    pub skill: u32,
    pub ranks: u16,
    pub advancement_class: u32,
    pub experience_spent: u32,
    pub initial_level: u32,
    pub resistance_at_last_check: u32,
    pub last_used_time: f64,
}
impl SkillUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::PrivateUpdateSkill);
        writer.bytes(&[self.sequence]);
        writer.u32(self.skill);
        writer.u16(self.ranks);
        writer.u16(1); // ACE adjustPP, explicitly not a u32.
        for value in [
            self.advancement_class,
            self.experience_spent,
            self.initial_level,
            self.resistance_at_last_check,
        ] {
            writer.u32(value);
        }
        writer.f64(self.last_used_time);
        writer.into_bytes()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentVitalUpdate {
    pub sequence: u8,
    pub vital: u32,
    pub current: u32,
}
impl CurrentVitalUpdate {
    pub fn encode(self) -> Vec<u8> {
        let mut writer = message_writer(GameMessageOpcode::PrivateUpdateAttribute2ndLevel);
        writer.bytes(&[self.sequence]);
        writer.u32(self.vital);
        writer.u32(self.current);
        writer.into_bytes()
    }
}
