//! Untrusted character-creation request from ACE CharacterHandler,
//! Entity/CharacterCreateInfo.cs and Entity/Appearance.cs. DAT validation and
//! permission checks belong to character/authentication services, never this DTO.
use crate::envelope::expect_opcode;
use crate::opcode::GameMessageOpcode;
use crate::{Reader, WireError};

#[derive(Clone, Debug, PartialEq)]
pub struct CharacterAppearance {
    pub eyes: u32,
    pub nose: u32,
    pub mouth: u32,
    pub hair_color: u32,
    pub eye_color: u32,
    pub hair_style: u32,
    pub headgear_style: u32,
    pub headgear_color: u32,
    pub shirt_style: u32,
    pub shirt_color: u32,
    pub pants_style: u32,
    pub pants_color: u32,
    pub footwear_style: u32,
    pub footwear_color: u32,
    pub skin_hue: f64,
    pub hair_hue: f64,
    pub headgear_hue: f64,
    pub shirt_hue: f64,
    pub pants_hue: f64,
    pub footwear_hue: f64,
}
impl CharacterAppearance {
    fn decode(reader: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            eyes: reader.u32()?,
            nose: reader.u32()?,
            mouth: reader.u32()?,
            hair_color: reader.u32()?,
            eye_color: reader.u32()?,
            hair_style: reader.u32()?,
            headgear_style: reader.u32()?,
            headgear_color: reader.u32()?,
            shirt_style: reader.u32()?,
            shirt_color: reader.u32()?,
            pants_style: reader.u32()?,
            pants_color: reader.u32()?,
            footwear_style: reader.u32()?,
            footwear_color: reader.u32()?,
            skin_hue: reader.f64()?,
            hair_hue: reader.f64()?,
            headgear_hue: reader.f64()?,
            shirt_hue: reader.f64()?,
            pants_hue: reader.f64()?,
            footwear_hue: reader.f64()?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterAbilities {
    pub strength: u32,
    pub endurance: u32,
    pub coordination: u32,
    pub quickness: u32,
    pub focus: u32,
    pub self_ability: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterCreateRequest {
    pub account: String,
    /// Official Unpack skips this word; preserve it without assuming it must be 1.
    pub unknown_constant: u32,
    pub heritage: u32,
    pub gender: u32,
    pub appearance: CharacterAppearance,
    pub template_option: i32,
    pub abilities: CharacterAbilities,
    pub character_slot: u32,
    pub class_id: u32,
    pub skill_advancement_classes: Vec<u32>,
    pub name: String,
    pub start_area: u32,
    /// These are CLIENT REQUESTS, not granted privileges.
    pub requested_admin: bool,
    pub requested_sentinel: bool,
    pub trailing_bytes: usize,
}
impl CharacterCreateRequest {
    pub fn decode(
        bytes: &[u8],
        max_payload_bytes: usize,
        max_string_units: usize,
        max_skills: usize,
    ) -> Result<Self, WireError> {
        if bytes.len() > max_payload_bytes {
            return Err(WireError::LimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        expect_opcode(&mut reader, GameMessageOpcode::CharacterCreate)?;
        let account = reader.client_string16(max_string_units)?;
        let unknown_constant = reader.u32()?;
        let heritage = reader.u32()?;
        let gender = reader.u32()?;
        let appearance = CharacterAppearance::decode(&mut reader)?;
        let template_option = reader.u32()? as i32;
        let abilities = CharacterAbilities {
            strength: reader.u32()?,
            endurance: reader.u32()?,
            coordination: reader.u32()?,
            quickness: reader.u32()?,
            focus: reader.u32()?,
            self_ability: reader.u32()?,
        };
        let character_slot = reader.u32()?;
        let class_id = reader.u32()?;
        let count = reader.u32()? as usize;
        if count > max_skills {
            return Err(WireError::LimitExceeded);
        }
        if count > reader.remaining() / 4 {
            return Err(WireError::Truncated);
        }
        let skill_advancement_classes = (0..count)
            .map(|_| reader.u32())
            .collect::<Result<Vec<_>, _>>()?;
        let name = reader.client_string16(max_string_units)?;
        let start_area = reader.u32()?;
        // Upstream compares exactly to 1, not a nonzero boolean conversion.
        let requested_admin = reader.u32()? == 1;
        let requested_sentinel = reader.u32()? == 1;
        Ok(Self {
            account,
            unknown_constant,
            heritage,
            gender,
            appearance,
            template_option,
            abilities,
            character_slot,
            class_id,
            skill_advancement_classes,
            name,
            start_area,
            requested_admin,
            requested_sentinel,
            trailing_bytes: reader.remaining(),
        })
    }
}
