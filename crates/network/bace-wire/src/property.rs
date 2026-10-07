//! ACE private/public property updates. Sequence bytes are supplied by replication.
use crate::WireError;
use crate::envelope::message_writer;
use crate::opcode::GameMessageOpcode as Op;

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue<'a> {
    Int(i32),
    Int64(i64),
    Bool(bool),
    Float(f64),
    String(&'a str),
    DataId(u32),
    InstanceId(u32),
}
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyUpdate<'a> {
    pub sequence: u8,
    /// `None` is private; `Some` addresses a publicly visible object.
    pub object_id: Option<u32>,
    pub property: u32,
    pub value: PropertyValue<'a>,
}
impl PropertyUpdate<'_> {
    pub fn encode(&self) -> Result<Vec<u8>, WireError> {
        let public = self.object_id.is_some();
        let opcode = match (&self.value, public) {
            (PropertyValue::Int(_), false) => Op::PrivateUpdatePropertyInt,
            (PropertyValue::Int(_), true) => Op::PublicUpdatePropertyInt,
            (PropertyValue::Int64(_), false) => Op::PrivateUpdatePropertyInt64,
            (PropertyValue::Int64(_), true) => Op::PublicUpdatePropertyInt64,
            (PropertyValue::Bool(_), false) => Op::PrivateUpdatePropertyBool,
            (PropertyValue::Bool(_), true) => Op::PublicUpdatePropertyBool,
            (PropertyValue::Float(_), false) => Op::PrivateUpdatePropertyFloat,
            (PropertyValue::Float(_), true) => Op::PublicUpdatePropertyFloat,
            (PropertyValue::String(_), false) => Op::PrivateUpdatePropertyString,
            (PropertyValue::String(_), true) => Op::PublicUpdatePropertyString,
            (PropertyValue::DataId(_), false) => Op::PrivateUpdatePropertyDataID,
            (PropertyValue::DataId(_), true) => Op::PublicUpdatePropertyDataID,
            (PropertyValue::InstanceId(_), false) => Op::PrivateUpdatePropertyInstanceID,
            (PropertyValue::InstanceId(_), true) => Op::PublicUpdateInstanceId,
        };
        let mut writer = message_writer(opcode);
        writer.bytes(&[self.sequence]);
        // Public strings put property BEFORE guid, unlike the other public updates.
        if matches!(self.value, PropertyValue::String(_)) {
            writer.u32(self.property);
            if let Some(id) = self.object_id {
                writer.u32(id);
            }
            writer.bytes(&[0; 3]);
        } else {
            if let Some(id) = self.object_id {
                writer.u32(id);
            }
            writer.u32(self.property);
        }
        match self.value {
            PropertyValue::Int(value) => writer.u32(value as u32),
            PropertyValue::Int64(value) => writer.u64(value as u64),
            PropertyValue::Bool(value) => writer.u32(u32::from(value)),
            PropertyValue::Float(value) => writer.f64(value),
            PropertyValue::String(value) => writer.string16(value)?,
            PropertyValue::DataId(value) | PropertyValue::InstanceId(value) => writer.u32(value),
        }
        Ok(writer.into_bytes())
    }
}
