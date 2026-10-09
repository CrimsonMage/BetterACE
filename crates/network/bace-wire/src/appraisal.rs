//! Pinned ACE AppraiseInfoExtensions and profile writers. Inputs are already
//! accepted, property-filtered appraisal projections; no source defaults here.
use crate::{
    Reader, WireError, Writer,
    opcode::{GameEventType, GameMessageOpcode},
};
/// GameAction IdentifyObject (0x00C8) is exactly one little-endian GUID.
/// Zero clears the requested/current selection in pinned Player.cs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentifyObjectRequest {
    pub target: u32,
}
impl IdentifyObjectRequest {
    pub fn decode(payload: &[u8]) -> Result<Self, WireError> {
        if payload.len() != 4 {
            return Err(WireError::InvalidLength);
        }
        Ok(Self {
            target: Reader::new(payload).u32()?,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct AppraisalLimits {
    pub table_entries: usize,
    pub string_bytes: usize,
    pub message_bytes: usize,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppraisalProfile {
    pub success: bool,
    pub integers: Vec<(u16, i32)>,
    pub integers64: Vec<(u16, i64)>,
    pub booleans: Vec<(u16, bool)>,
    pub doubles: Vec<(u16, f64)>,
    pub strings: Vec<(u16, String)>,
    pub data_ids: Vec<(u16, u32)>,
    pub spells: Vec<u32>,
    /// Slash, pierce, bludgeon, cold, fire, acid, nether, lightning.
    pub armor: Option<[f32; 8]>,
    pub creature: Option<AppraisalCreature>,
    pub weapon: Option<AppraisalWeapon>,
    /// Flags, valid wield locations, ammunition type.
    pub hook: Option<[u32; 3]>,
    /// Highlight and color masks. Absent when the highlight is zero.
    pub armor_mask: Option<(u16, u16)>,
    pub weapon_mask: Option<(u16, u16)>,
    pub resist_mask: Option<(u16, u16)>,
    /// Head, chest, abdomen, upper arm, lower arm, hand, upper leg, lower leg, foot.
    pub armor_levels: Option<[u32; 9]>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppraisalCreature {
    pub health: u32,
    pub maximum_health: u32,
    /// Strength, Endurance, Quickness, Coordination, Focus, Self, current Stamina,
    /// current Mana, maximum Stamina, maximum Mana.
    pub attributes: Option<[u32; 10]>,
    pub attribute_mask: Option<(u16, u16)>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppraisalWeapon {
    pub damage_type: u32,
    pub time: u32,
    pub skill: u32,
    pub damage: u32,
    pub variance: f64,
    pub damage_modifier: f64,
    pub length: f64,
    pub maximum_velocity: f64,
    pub offense: f64,
    pub velocity_estimated: u32,
}
impl AppraisalProfile {
    pub fn encode(
        &self,
        actor: u32,
        sequence: u32,
        target: u32,
        limits: AppraisalLimits,
    ) -> Result<Vec<u8>, WireError> {
        let mut w = Writer::new();
        for value in [
            GameMessageOpcode::GameEvent.0,
            actor,
            sequence,
            GameEventType::IdentifyObjectResponse.0,
            target,
        ] {
            w.u32(value);
        }
        self.write(&mut w, limits)?;
        if w.position() > limits.message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(w.into_bytes())
    }
    fn write(&self, w: &mut Writer, limits: AppraisalLimits) -> Result<(), WireError> {
        if limits.table_entries > u16::MAX as usize {
            return Err(WireError::LimitExceeded);
        }
        let mut flags = 0u32;
        for (present, flag) in [
            (!self.integers.is_empty(), 1),
            (!self.booleans.is_empty(), 2),
            (!self.doubles.is_empty(), 4),
            (!self.strings.is_empty(), 8),
            (!self.spells.is_empty(), 0x10),
            (self.weapon.is_some(), 0x20),
            (self.hook.is_some(), 0x40),
            (self.armor.is_some(), 0x80),
            (self.creature.is_some(), 0x100),
            (self.armor_mask.is_some(), 0x200),
            (self.resist_mask.is_some(), 0x400),
            (self.weapon_mask.is_some(), 0x800),
            (!self.data_ids.is_empty(), 0x1000),
            (!self.integers64.is_empty(), 0x2000),
            (self.armor_levels.is_some(), 0x4000),
        ] {
            if present {
                flags |= flag;
            }
        }
        w.u32(flags);
        w.u32(u32::from(self.success));
        table(w, &self.integers, 16, limits, |w, v| {
            w.u32(*v as u32);
            Ok(())
        })?;
        table(w, &self.integers64, 8, limits, |w, v| {
            w.u64(*v as u64);
            Ok(())
        })?;
        table(w, &self.booleans, 8, limits, |w, v| {
            w.u32(u32::from(*v));
            Ok(())
        })?;
        table(w, &self.doubles, 8, limits, |w, v| {
            w.f64(*v);
            Ok(())
        })?;
        table(w, &self.strings, 8, limits, |w, v| {
            if v.len() > limits.string_bytes {
                return Err(WireError::LimitExceeded);
            }
            w.string16(v)
        })?;
        table(w, &self.data_ids, 8, limits, |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        if !self.spells.is_empty() {
            if self.spells.len() > limits.table_entries {
                return Err(WireError::LimitExceeded);
            }
            w.u32(self.spells.len() as u32);
            for id in &self.spells {
                w.u32(*id);
            }
        }
        if let Some(profile) = self.armor {
            for value in profile {
                w.f32(value);
            }
        }
        if let Some(c) = &self.creature {
            let flags =
                u32::from(c.attribute_mask.is_some()) | (u32::from(c.attributes.is_some()) * 8);
            w.u32(flags);
            w.u32(c.health);
            w.u32(c.maximum_health);
            if let Some(values) = c.attributes {
                for value in values {
                    w.u32(value);
                }
            }
            if let Some((highlight, color)) = c.attribute_mask {
                w.u16(highlight);
                w.u16(color);
            }
        }
        if let Some(p) = &self.weapon {
            for v in [p.damage_type, p.time, p.skill, p.damage] {
                w.u32(v);
            }
            for v in [
                p.variance,
                p.damage_modifier,
                p.length,
                p.maximum_velocity,
                p.offense,
            ] {
                w.f64(v);
            }
            w.u32(p.velocity_estimated);
        }
        if let Some(values) = self.hook {
            for value in values {
                w.u32(value);
            }
        }
        for (highlight, color) in [self.armor_mask, self.weapon_mask, self.resist_mask]
            .into_iter()
            .flatten()
        {
            if highlight == 0 {
                return Err(WireError::InvalidEncoding);
            }
            w.u16(highlight);
            w.u16(color);
        }
        if let Some(values) = self.armor_levels {
            for value in values {
                w.u32(value);
            }
        }
        Ok(())
    }
}
fn table<T>(
    w: &mut Writer,
    entries: &[(u16, T)],
    buckets: u16,
    limits: AppraisalLimits,
    write: impl Fn(&mut Writer, &T) -> Result<(), WireError>,
) -> Result<(), WireError> {
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() > limits.table_entries {
        return Err(WireError::LimitExceeded);
    }
    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by_key(|(id, _)| (*id % buckets, *id));
    if sorted.windows(2).any(|p| p[0].0 == p[1].0) {
        return Err(WireError::InvalidEncoding);
    }
    w.u16(sorted.len() as u16);
    w.u16(buckets);
    for (id, value) in sorted {
        w.u32(u32::from(*id));
        write(w, value)?;
        if w.position() > limits.message_bytes {
            return Err(WireError::LimitExceeded);
        }
    }
    Ok(())
}
