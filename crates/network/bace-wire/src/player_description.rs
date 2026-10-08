//! ACE PlayerDescription including the explicitly supplied enchantment registry.
//! Property visibility/name decoration and inventory ordering are projection policy.
use crate::envelope::message_writer;
use crate::opcode::{GameEventType, GameMessageOpcode};
use crate::{ContainerEntry, WireError, WirePosition, Writer};

#[derive(Clone, Copy, Debug)]
pub struct PlayerDescriptionLimits {
    pub max_table_entries: usize,
    pub max_string_bytes: usize,
    pub max_gameplay_options_bytes: usize,
    pub max_message_bytes: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoginAttribute {
    pub ranks: u32,
    pub starting: u32,
    pub experience: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoginVital {
    pub attribute: LoginAttribute,
    pub current: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginSkill {
    pub id: u32,
    pub ranks: u16,
    pub advancement: u32,
    pub experience: u32,
    pub initial_level: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginShortcut {
    pub index: u32,
    pub object_id: u32,
    pub spell_id: u16,
    pub layer: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginEquipment {
    pub object_id: u32,
    pub location: u32,
    pub priority: u32,
}
/// Immutable preselected fields. `has_enchantments` must reflect the source state;
/// true is rejected until registry serialization is implemented, never discarded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerDescription {
    pub weenie_type: u32,
    pub integers: Vec<(u16, i32)>,
    pub integers64: Vec<(u16, i64)>,
    pub booleans: Vec<(u16, bool)>,
    pub doubles: Vec<(u16, f64)>,
    pub strings: Vec<(u16, String)>,
    pub data_ids: Vec<(u16, u32)>,
    pub instance_ids: Vec<(u16, u32)>,
    pub last_outside_death: Option<WirePosition>,
    /// Strength, Endurance, Quickness, Coordination, Focus, Self (wire order).
    pub attributes: [LoginAttribute; 6],
    /// Health, Stamina, Mana.
    pub vitals: [LoginVital; 3],
    pub skills: Vec<LoginSkill>,
    pub known_spells: Vec<i32>,
    pub has_enchantments: bool,
    pub enchantments: Option<crate::EnchantmentRegistry>,
    pub options1: u32,
    pub options2: u32,
    pub spellbook_filters: u32,
    pub shortcuts: Vec<LoginShortcut>,
    pub spell_bars: [Vec<u32>; 8],
    pub desired_components: Vec<(u32, u32)>,
    /// The existing opaque client options blob; no length prefix is added by ACE.
    pub gameplay_options: Vec<u8>,
    /// Stable placement order: ordinary items first, then backpack slots.
    pub inventory: Vec<ContainerEntry>,
    /// Authoritative equipped-object iteration order.
    pub equipment: Vec<LoginEquipment>,
}
impl PlayerDescription {
    pub fn encode(
        &self,
        object_id: u32,
        sequence: u32,
        limits: PlayerDescriptionLimits,
    ) -> Result<Vec<u8>, WireError> {
        if self.has_enchantments && self.enchantments.is_none() {
            return Err(WireError::InvalidEncoding);
        }
        let mut w = message_writer(GameMessageOpcode::GameEvent);
        for v in [object_id, sequence, GameEventType::PlayerDescription.0] {
            w.u32(v);
        }
        let flags = u32::from(!self.integers.is_empty())
            | (u32::from(!self.booleans.is_empty()) << 1)
            | (u32::from(!self.doubles.is_empty()) << 2)
            | (u32::from(!self.data_ids.is_empty()) << 3)
            | (u32::from(!self.strings.is_empty()) << 4)
            | (u32::from(self.last_outside_death.is_some()) << 5)
            | (u32::from(!self.instance_ids.is_empty()) << 6)
            | (u32::from(!self.integers64.is_empty()) << 7);
        w.u32(flags);
        w.u32(self.weenie_type);
        table(&mut w, &self.integers, 64, limits, |w, v| {
            w.u32(*v as u32);
            Ok(())
        })?;
        table(&mut w, &self.integers64, 64, limits, |w, v| {
            w.u64(*v as u64);
            Ok(())
        })?;
        table(&mut w, &self.booleans, 32, limits, |w, v| {
            w.u32(u32::from(*v));
            Ok(())
        })?;
        table(&mut w, &self.doubles, 32, limits, |w, v| {
            w.f64(*v);
            Ok(())
        })?;
        table(&mut w, &self.strings, 32, limits, |w, v| {
            if v.chars().count() > limits.max_string_bytes {
                return Err(WireError::LimitExceeded);
            }
            w.string16(v)
        })?;
        table(&mut w, &self.data_ids, 32, limits, |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        table(&mut w, &self.instance_ids, 32, limits, |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        if let Some(position) = self.last_outside_death {
            w.u16(1);
            w.u16(16);
            w.u32(14);
            position.write(&mut w);
        }
        w.u32(
            3 | if self.known_spells.is_empty() {
                0
            } else {
                0x100
            } | if self.enchantments.is_some() {
                0x200
            } else {
                0
            },
        );
        w.u32(1); // Full valid player projection always has health.
        w.u32(0x1ff);
        for a in self.attributes {
            attribute(&mut w, a);
        }
        for v in self.vitals {
            attribute(&mut w, v.attribute);
            w.u32(v.current);
        }
        bounded_count(self.skills.len(), limits)?;
        let mut skills: Vec<_> = self.skills.iter().collect();
        skills.sort_by_key(|s| (s.id % 32, s.id));
        if skills.iter().any(|s| s.id > i32::MAX as u32)
            || skills.windows(2).any(|s| s[0].id == s[1].id)
        {
            return Err(WireError::InvalidEncoding);
        }
        w.u16(skills.len() as u16);
        w.u16(32);
        for s in skills {
            w.u32(s.id);
            w.u16(s.ranks);
            w.u16(1);
            w.u32(s.advancement);
            w.u32(s.experience);
            w.u32(s.initial_level);
            w.u32(0);
            w.f64(0.0);
        }
        if !self.known_spells.is_empty() {
            bounded_count(self.known_spells.len(), limits)?;
            let mut spells = self.known_spells.clone();
            spells.sort_by_key(|s| (*s % 64, *s));
            if spells.windows(2).any(|s| s[0] == s[1]) {
                return Err(WireError::InvalidEncoding);
            }
            w.u16(spells.len() as u16);
            w.u16(64);
            for id in spells {
                w.u32(id as u32);
                w.f32(2.0);
            }
        }
        if let Some(registry) = &self.enchantments {
            registry.write(&mut w, limits.max_table_entries)?;
        }
        let option_flags = 0x460
            | u32::from(!self.shortcuts.is_empty())
            | if self.desired_components.is_empty() {
                0
            } else {
                8
            }
            | if self.gameplay_options.is_empty() {
                0
            } else {
                0x200
            };
        w.u32(option_flags);
        w.u32(self.options1);
        if !self.shortcuts.is_empty() {
            bounded_count(self.shortcuts.len(), limits)?;
            w.u32(self.shortcuts.len() as u32);
            for s in &self.shortcuts {
                w.u32(s.index);
                w.u32(s.object_id);
                w.u16(s.spell_id);
                w.u16(s.layer);
            }
        }
        for bar in &self.spell_bars {
            bounded_count(bar.len(), limits)?;
            w.u32(bar.len() as u32);
            for spell in bar {
                w.u32(*spell);
            }
        }
        if !self.desired_components.is_empty() {
            bounded_count(self.desired_components.len(), limits)?;
            let mut values = self.desired_components.clone();
            values.sort_by_key(|(id, _)| id % 256);
            w.u16(values.len() as u16);
            w.u16(256);
            for (id, quantity) in values {
                w.u32(id);
                w.u32(quantity);
            }
        }
        w.u32(self.spellbook_filters);
        w.u32(self.options2);
        if self.gameplay_options.len() > limits.max_gameplay_options_bytes {
            return Err(WireError::LimitExceeded);
        }
        w.bytes(&self.gameplay_options);
        bounded_count(self.inventory.len(), limits)?;
        w.u32(self.inventory.len() as u32);
        for item in &self.inventory {
            w.u32(item.object_id);
            w.u32(item.container_type);
        }
        bounded_count(self.equipment.len(), limits)?;
        w.u32(self.equipment.len() as u32);
        for item in &self.equipment {
            w.u32(item.object_id);
            w.u32(item.location);
            w.u32(item.priority);
        }
        if w.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
        Ok(w.into_bytes())
    }
}
fn attribute(w: &mut Writer, a: LoginAttribute) {
    w.u32(a.ranks);
    w.u32(a.starting);
    w.u32(a.experience);
}
fn bounded_count(n: usize, limits: PlayerDescriptionLimits) -> Result<(), WireError> {
    if n > limits.max_table_entries || n > u16::MAX as usize {
        Err(WireError::LimitExceeded)
    } else {
        Ok(())
    }
}
fn table<T>(
    w: &mut Writer,
    entries: &[(u16, T)],
    buckets: u16,
    limits: PlayerDescriptionLimits,
    write: impl Fn(&mut Writer, &T) -> Result<(), WireError>,
) -> Result<(), WireError> {
    if entries.is_empty() {
        return Ok(());
    }
    bounded_count(entries.len(), limits)?;
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
        if w.position() > limits.max_message_bytes {
            return Err(WireError::LimitExceeded);
        }
    }
    Ok(())
}
