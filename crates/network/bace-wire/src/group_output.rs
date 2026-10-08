//! Pinned ACE fellowship/allegiance structures. Source TODO fields remain zero.
use crate::{
    SocialCodecLimits, WireError, WirePosition, Writer,
    envelope::message_writer,
    opcode::{GameEventType as Op, GameMessageOpcode},
    social_tables::string,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FellowData {
    pub actor: u32,
    pub name: String,
    pub level: u32,
    pub maximum: [u32; 3],
    pub current: [u32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FellowshipData {
    pub name: String,
    pub leader: u32,
    pub members: Vec<FellowData>,
    pub share_xp: bool,
    pub even_share: bool,
    pub open: bool,
    pub locked: bool,
    pub departed: Vec<(u32, i32)>,
    pub locks: Vec<(String, u32, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllegianceMemberData {
    pub actor: u32,
    pub cached: u32,
    pub tithed: u32,
    pub online: bool,
    pub may_pass_up: bool,
    pub gender: u8,
    pub heritage: u8,
    pub rank: u16,
    pub level: u32,
    pub loyalty: u16,
    pub leadership: u16,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AllegianceProfileData {
    pub total_members: u32,
    pub total_vassals: u32,
    pub chat_room: u32,
    pub name: String,
    pub sanctuary: Option<WirePosition>,
    pub monarch: Option<AllegianceMemberData>,
    pub records: Vec<(u32, AllegianceMemberData)>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum GroupEvent<'a> {
    ErrorWithString {
        code: u32,
        text: &'a str,
    },
    Fellowship(&'a FellowshipData),
    Fellow {
        fellow: &'a FellowData,
        share_loot: bool,
        update_type: u32,
    },
    FellowQuit(u32),
    FellowDismiss(u32),
    FellowDisband,
    FellowDone,
    Allegiance {
        rank: u32,
        profile: &'a AllegianceProfileData,
    },
    AllegianceInfo {
        subject: u32,
        profile: &'a AllegianceProfileData,
    },
    AllegianceDone(u32),
    Confirmation {
        kind: u32,
        token: u32,
        text: &'a str,
    },
    ConfirmationDone {
        kind: u32,
        token: u32,
    },
}
impl GroupEvent<'_> {
    pub fn encode(
        &self,
        actor: u32,
        sequence: u32,
        limits: SocialCodecLimits,
    ) -> Result<Vec<u8>, WireError> {
        let op = match self {
            Self::ErrorWithString { .. } => Op::WeenieErrorWithString,
            Self::Fellowship(_) => Op::FellowshipFullUpdate,
            Self::Fellow { .. } => Op::FellowshipUpdateFellow,
            Self::FellowQuit(_) => Op::FellowshipQuit,
            Self::FellowDismiss(_) => Op::FellowshipDismiss,
            Self::FellowDisband => Op::FellowshipDisband,
            Self::FellowDone => Op::FellowshipFellowUpdateDone,
            Self::Allegiance { .. } => Op::AllegianceUpdate,
            Self::AllegianceInfo { .. } => Op::AllegianceInfoResponse,
            Self::AllegianceDone(_) => Op::AllegianceAllegianceUpdateDone,
            Self::Confirmation { .. } => Op::CharacterConfirmationRequest,
            Self::ConfirmationDone { .. } => Op::CharacterConfirmationDone,
        };
        let mut w = message_writer(GameMessageOpcode::GameEvent);
        w.u32(actor);
        w.u32(sequence);
        w.u32(op.0);
        match self {
            Self::ErrorWithString { code, text } => {
                w.u32(*code);
                string(&mut w, text, limits.max_string_bytes)?
            }
            Self::Fellowship(f) => f.write(&mut w, limits)?,
            Self::Fellow {
                fellow,
                share_loot,
                update_type,
            } => {
                fellow.write(&mut w, u32::from(*share_loot) << 1, limits)?;
                w.u32(*update_type)
            }
            Self::FellowQuit(id) | Self::FellowDismiss(id) | Self::AllegianceDone(id) => w.u32(*id),
            Self::FellowDisband | Self::FellowDone => {}
            Self::Allegiance { rank, profile } => {
                w.u32(*rank);
                profile.write(&mut w, limits)?
            }
            Self::AllegianceInfo { subject, profile } => {
                w.u32(*subject);
                profile.write(&mut w, limits)?
            }
            Self::Confirmation { kind, token, text } => {
                w.u32(*kind);
                w.u32(*token);
                string(&mut w, text, limits.max_string_bytes)?
            }
            Self::ConfirmationDone { kind, token } => {
                w.u32(*kind);
                w.u32(*token)
            }
        }
        if w.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(w.into_bytes())
    }
}
impl FellowData {
    fn write(&self, w: &mut Writer, share: u32, l: SocialCodecLimits) -> Result<(), WireError> {
        for value in [self.actor, 0, 0, self.level]
            .into_iter()
            .chain(self.maximum)
            .chain(self.current)
            .chain([share])
        {
            w.u32(value)
        }
        string(w, &self.name, l.max_string_bytes)
    }
}
impl FellowshipData {
    fn write(&self, w: &mut Writer, l: SocialCodecLimits) -> Result<(), WireError> {
        if self.members.len() > 9
            || self.members.len() > l.max_entries
            || self.departed.len() > l.max_entries
            || self.locks.len() > l.max_entries
            || self.departed.len() > u16::MAX as usize
            || self.locks.len() > u16::MAX as usize
        {
            return Err(WireError::LimitExceeded);
        }
        let mut members: Vec<_> = self.members.iter().collect();
        members.sort_by_key(|m| (m.actor % 16, m.actor));
        if members.windows(2).any(|p| p[0].actor == p[1].actor) {
            return Err(WireError::InvalidEncoding);
        }
        w.u16(members.len() as u16);
        w.u16(16);
        for m in members {
            m.write(w, 0x10, l)?
        }
        string(w, &self.name, l.max_string_bytes)?;
        for n in [
            self.leader,
            u32::from(self.share_xp),
            u32::from(self.even_share),
            u32::from(self.open),
            u32::from(self.locked),
        ] {
            w.u32(n)
        }
        let mut departed = self.departed.clone();
        departed.sort_by_key(|(id, _)| (*id % 32, *id));
        if departed.windows(2).any(|p| p[0].0 == p[1].0) {
            return Err(WireError::InvalidEncoding);
        }
        w.u16(departed.len() as u16);
        w.u16(32);
        for (id, time) in departed {
            w.u32(id);
            w.u32(time as u32)
        }
        w.u16(self.locks.len() as u16);
        w.u16(32);
        for (name, time, sequence) in &self.locks {
            string(w, name, l.max_string_bytes)?;
            for n in [0, 0, 0, *time, *sequence] {
                w.u32(n)
            }
        }
        Ok(())
    }
}
impl AllegianceMemberData {
    fn write(&self, w: &mut Writer, l: SocialCodecLimits) -> Result<(), WireError> {
        for n in [
            self.actor,
            self.cached,
            self.tithed,
            12 | u32::from(self.online) | if self.may_pass_up { 16 } else { 0 },
        ] {
            w.u32(n)
        }
        w.bytes(&[self.gender, self.heritage]);
        w.u16(self.rank);
        w.u32(self.level);
        w.u16(self.loyalty);
        w.u16(self.leadership);
        w.u32(0);
        w.u32(0);
        string(w, &self.name, l.max_string_bytes)
    }
}
impl AllegianceProfileData {
    fn write(&self, w: &mut Writer, l: SocialCodecLimits) -> Result<(), WireError> {
        let count = self.records.len() + usize::from(self.monarch.is_some());
        if count > 13 || count > l.max_entries {
            return Err(WireError::LimitExceeded);
        }
        if self.monarch.is_none() && !self.records.is_empty() {
            return Err(WireError::InvalidEncoding);
        }
        w.u32(self.total_members);
        w.u32(self.total_vassals);
        w.u16(count as u16);
        w.u16(11);
        w.u16(0);
        w.u16(256);
        w.u32(0);
        for _ in 0..4 {
            w.u32(0)
        }
        string(w, "", l.max_string_bytes)?;
        string(w, "", l.max_string_bytes)?;
        w.u32(self.chat_room);
        let empty = WirePosition {
            cell: 0,
            origin: [0.; 3],
            rotation: [1., 0., 0., 0.],
        };
        let position = self.sanctuary.as_ref().unwrap_or(&empty);
        w.u32(position.cell);
        for n in position.origin.into_iter().chain(position.rotation) {
            w.f32(n)
        }
        string(w, &self.name, l.max_string_bytes)?;
        for _ in 0..3 {
            w.u32(0)
        }
        if let Some(monarch) = &self.monarch {
            monarch.write(w, l)?
        }
        for (parent, node) in &self.records {
            w.u32(*parent);
            node.write(w, l)?
        }
        Ok(())
    }
}
