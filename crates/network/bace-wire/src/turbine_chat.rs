//! Pinned ACE Turbine chat layout. Declared lengths are metadata (the outbound
//! serializer overstates both by eight); parsing is bounded by actual bytes.
use crate::envelope::{expect_opcode, message_writer};
use crate::opcode::GameMessageOpcode;
use crate::{Reader, WireError, Writer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurbineFrameHeader {
    pub declared_message_bytes: u32,
    pub blob_type: u32,
    pub dispatch: u32,
    pub target_type: u32,
    pub target_id: u32,
    pub transport_type: u32,
    pub transport_id: u32,
    pub cookie: u32,
    pub declared_payload_bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurbineFrame<'a> {
    pub header: TurbineFrameHeader,
    pub payload: &'a [u8],
}
impl<'a> TurbineFrame<'a> {
    pub fn decode(bytes: &'a [u8], max_frame_bytes: usize) -> Result<Self, WireError> {
        if bytes.len() > max_frame_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::TurbineChat)?;
        let header = TurbineFrameHeader {
            declared_message_bytes: reader.u32()?,
            blob_type: reader.u32()?,
            dispatch: reader.u32()?,
            target_type: reader.u32()?,
            target_id: reader.u32()?,
            transport_type: reader.u32()?,
            transport_id: reader.u32()?,
            cookie: reader.u32()?,
            declared_payload_bytes: reader.u32()?,
        };
        Ok(Self {
            header,
            payload: reader.take(reader.remaining())?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurbineChatRequest {
    pub header: TurbineFrameHeader,
    pub context_id: u32,
    pub response_id: u32,
    pub method_id: u32,
    /// Even dispatch ByName is read as this DWORD by the pinned handler.
    pub channel: u32,
    pub text: String,
    pub extra_data_size: u32,
    /// Claimed client identity, never an authenticated sender.
    pub claimed_sender_id: u32,
    pub result: i32,
    pub chat_type: u32,
    pub trailing_bytes: usize,
}
impl TurbineChatRequest {
    pub fn decode(
        bytes: &[u8],
        max_frame_bytes: usize,
        max_text_units: usize,
    ) -> Result<Self, WireError> {
        let frame = TurbineFrame::decode(bytes, max_frame_bytes)?;
        if frame.header.blob_type != 3 {
            return Err(WireError::UnexpectedOpcode(frame.header.blob_type));
        }
        let mut reader = Reader::new(frame.payload);
        Ok(Self {
            header: frame.header,
            context_id: reader.u32()?,
            response_id: reader.u32()?,
            method_id: reader.u32()?,
            channel: reader.u32()?,
            text: read_packed_utf16(&mut reader, max_text_units)?,
            extra_data_size: reader.u32()?,
            claimed_sender_id: reader.u32()?,
            result: reader.u32()? as i32,
            chat_type: reader.u32()?,
            trailing_bytes: reader.remaining(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurbineChatEvent {
    pub channel: u32,
    pub sender_name: String,
    pub text: String,
    pub sender_id: u32,
    pub chat_type: u32,
}
impl TurbineChatEvent {
    pub fn encode(
        &self,
        max_frame_bytes: usize,
        max_text_units: usize,
    ) -> Result<Vec<u8>, WireError> {
        let name_units = self.sender_name.encode_utf16().count();
        let text_units = self.text.encode_utf16().count();
        // Above these bounds ACE writes a lossy prefix (and long sender names
        // accidentally use message.Length). Refuse those malformed output forms.
        if name_units > 127 || text_units > 255 || text_units > max_text_units {
            return Err(WireError::LimitExceeded);
        }
        let payload_len =
            20 + 1 + name_units * 2 + if text_units < 128 { 1 } else { 2 } + text_units * 2;
        if payload_len + 40 > max_frame_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut writer = server_header(1, payload_len)?;
        writer.u32(self.channel);
        write_ace_utf16(&mut writer, &self.sender_name, name_units);
        write_ace_utf16(&mut writer, &self.text, text_units);
        writer.u32(12);
        writer.u32(self.sender_id);
        writer.u32(0);
        writer.u32(self.chat_type);
        Ok(writer.into_bytes())
    }
}
/// The pinned handler answers ByID requests with dispatch ByName (1), while the
/// response and method DWORDs in its body are both 2. No routing policy is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurbineChatResponse {
    pub context_id: u32,
}
impl TurbineChatResponse {
    pub fn encode(self) -> Vec<u8> {
        // Fixed-size source response: 56 total bytes, payload size word = 24.
        let mut writer = message_writer(GameMessageOpcode::TurbineChat);
        for value in [
            56,
            5,
            1,
            1,
            0x000b00b5,
            1,
            0x000b00b5,
            0,
            24,
            self.context_id,
            2,
            2,
            0,
        ] {
            writer.u32(value);
        }
        writer.into_bytes()
    }
}
fn server_header(blob_type: u32, payload_len: usize) -> Result<Writer, WireError> {
    let total = u32::try_from(
        payload_len
            .checked_add(40)
            .ok_or(WireError::LimitExceeded)?,
    )
    .map_err(|_| WireError::LimitExceeded)?;
    let mut writer = message_writer(GameMessageOpcode::TurbineChat);
    for value in [
        total,
        blob_type,
        1,
        1,
        0x000b00b5,
        1,
        0x000b00b5,
        0,
        total - 32,
    ] {
        writer.u32(value);
    }
    Ok(writer)
}
fn write_ace_utf16(writer: &mut Writer, text: &str, units: usize) {
    if units < 128 {
        writer.bytes(&[units as u8]);
    } else {
        writer.u16(0x80 | ((units as u16) << 8));
    }
    for unit in text.encode_utf16() {
        writer.u16(unit);
    }
}
fn read_packed_utf16(reader: &mut Reader<'_>, max_units: usize) -> Result<String, WireError> {
    let first = reader.take(1)?[0];
    let units = if first & 0x80 == 0 {
        usize::from(first)
    } else {
        (usize::from(first & 0x7f) << 8) | usize::from(reader.take(1)?[0])
    };
    if units > max_units {
        return Err(WireError::LimitExceeded);
    }
    let bytes = reader.take(units * 2)?;
    let units: Vec<_> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| WireError::InvalidEncoding)
}
