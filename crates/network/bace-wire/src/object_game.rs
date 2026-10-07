//! Exact WeenieDesc optional-field order from WorldObject.SerializeCreateObject.
//! Presence is explicit; projection decides which authoritative properties exist.
use crate::object_model::known_type;
use crate::{ObjectRestrictions, WireError, Writer};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectGameOptions {
    pub plural_name: Option<String>,
    pub item_capacity: Option<u8>,
    pub container_capacity: Option<u8>,
    pub ammo_type: Option<u16>,
    pub value: Option<i32>,
    pub usable: Option<u32>,
    pub use_radius: Option<f32>,
    pub target_type: Option<u32>,
    pub ui_effects: Option<u32>,
    pub combat_use: Option<i8>,
    pub structure: Option<u16>,
    pub max_structure: Option<u16>,
    pub stack_size: Option<u16>,
    pub max_stack_size: Option<u16>,
    pub container: Option<u32>,
    pub wielder: Option<u32>,
    pub valid_locations: Option<u32>,
    pub wielded_location: Option<u32>,
    pub clothing_priority: Option<u32>,
    pub radar_color: Option<u8>,
    pub radar_behavior: Option<u8>,
    pub script: Option<u16>,
    pub workmanship: Option<f32>,
    pub burden: Option<u16>,
    pub spell: Option<u16>,
    pub house_owner: Option<u32>,
    pub house_restrictions: Option<ObjectRestrictions>,
    pub hook_item_types: Option<u32>,
    pub monarch: Option<u32>,
    pub hook_type: Option<u16>,
    pub icon_overlay: Option<u32>,
    pub icon_underlay: Option<u32>,
    pub material_type: Option<u32>,
    pub cooldown: Option<i32>,
    pub cooldown_duration: Option<f64>,
    pub pet_owner: Option<u32>,
}
impl ObjectGameOptions {
    pub fn header_flags(&self) -> (u32, u32) {
        let mut first = 0;
        for (present, flag) in [
            (self.plural_name.is_some(), 0x1),
            (self.item_capacity.is_some(), 0x2),
            (self.container_capacity.is_some(), 0x4),
            (self.value.is_some(), 0x8),
            (self.usable.is_some(), 0x10),
            (self.use_radius.is_some(), 0x20),
            (self.monarch.is_some(), 0x40),
            (self.ui_effects.is_some(), 0x80),
            (self.ammo_type.is_some(), 0x100),
            (self.combat_use.is_some(), 0x200),
            (self.structure.is_some(), 0x400),
            (self.max_structure.is_some(), 0x800),
            (self.stack_size.is_some(), 0x1000),
            (self.max_stack_size.is_some(), 0x2000),
            (self.container.is_some(), 0x4000),
            (self.wielder.is_some(), 0x8000),
            (self.valid_locations.is_some(), 0x10000),
            (self.wielded_location.is_some(), 0x20000),
            (self.clothing_priority.is_some(), 0x40000),
            (self.target_type.is_some(), 0x80000),
            (self.radar_color.is_some(), 0x100000),
            (self.burden.is_some(), 0x200000),
            (self.spell.is_some(), 0x400000),
            (self.radar_behavior.is_some(), 0x800000),
            (self.workmanship.is_some(), 0x1000000),
            (self.house_owner.is_some(), 0x2000000),
            (self.house_restrictions.is_some(), 0x4000000),
            (self.script.is_some(), 0x8000000),
            (self.hook_type.is_some(), 0x10000000),
            (self.hook_item_types.is_some(), 0x20000000),
            (self.icon_overlay.is_some(), 0x40000000),
            (self.material_type.is_some(), 0x80000000),
        ] {
            if present {
                first |= flag;
            }
        }
        let second = u32::from(self.icon_underlay.is_some())
            | (u32::from(self.cooldown.is_some()) << 1)
            | (u32::from(self.cooldown_duration.is_some()) << 2)
            | (u32::from(self.pet_owner.is_some()) << 3);
        (first, second)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectGameData {
    pub name: String,
    pub class_id: u32,
    pub icon_id: u32,
    pub item_type: u32,
    /// IncludesSecondHeader is added when the second header has fields. All
    /// remaining bits are supplied by projection, with no privilege inference.
    pub description_flags: u32,
    pub options: ObjectGameOptions,
}
impl ObjectGameData {
    pub fn encode(
        &self,
        max_string_bytes: usize,
        max_permissions: usize,
    ) -> Result<Vec<u8>, WireError> {
        let mut writer = Writer::new();
        self.write(&mut writer, max_string_bytes, max_permissions)?;
        Ok(writer.into_bytes())
    }
    pub(crate) fn write(
        &self,
        writer: &mut Writer,
        max_string_bytes: usize,
        max_permissions: usize,
    ) -> Result<(), WireError> {
        check_string(&self.name, max_string_bytes)?;
        if let Some(value) = &self.options.plural_name {
            check_string(value, max_string_bytes)?;
        }
        let (flags, flags2) = self.options.header_flags();
        let description_flags = self.description_flags | if flags2 != 0 { 0x04000000 } else { 0 };
        writer.u32(flags);
        writer.string16(&self.name)?;
        writer.packed_u32(self.class_id)?;
        known_type(writer, self.icon_id, 0x06000000)?;
        writer.u32(self.item_type);
        writer.u32(description_flags);
        writer.align4();
        if description_flags & 0x04000000 != 0 {
            writer.u32(flags2);
        }
        let value = &self.options;
        if let Some(v) = &value.plural_name {
            writer.string16(v)?;
        }
        if let Some(v) = value.item_capacity {
            writer.bytes(&[v]);
        }
        if let Some(v) = value.container_capacity {
            writer.bytes(&[v]);
        }
        if let Some(v) = value.ammo_type {
            writer.u16(v);
        }
        if let Some(v) = value.value {
            writer.u32(v as u32);
        }
        if let Some(v) = value.usable {
            writer.u32(v);
        }
        if let Some(v) = value.use_radius {
            writer.f32(v);
        }
        if let Some(v) = value.target_type {
            writer.u32(v);
        }
        if let Some(v) = value.ui_effects {
            writer.u32(v);
        }
        if let Some(v) = value.combat_use {
            writer.bytes(&[v as u8]);
        }
        if let Some(v) = value.structure {
            writer.u16(v);
        }
        if let Some(v) = value.max_structure {
            writer.u16(v);
        }
        if let Some(v) = value.stack_size {
            writer.u16(v);
        }
        if let Some(v) = value.max_stack_size {
            writer.u16(v);
        }
        if let Some(v) = value.container {
            writer.u32(v);
        }
        if let Some(v) = value.wielder {
            writer.u32(v);
        }
        if let Some(v) = value.valid_locations {
            writer.u32(v);
        }
        if let Some(v) = value.wielded_location {
            writer.u32(v);
        }
        if let Some(v) = value.clothing_priority {
            writer.u32(v);
        }
        if let Some(v) = value.radar_color {
            writer.bytes(&[v]);
        }
        if let Some(v) = value.radar_behavior {
            writer.bytes(&[v]);
        }
        if let Some(v) = value.script {
            writer.u16(v);
        }
        if let Some(v) = value.workmanship {
            writer.f32(v);
        }
        if let Some(v) = value.burden {
            writer.u16(v);
        }
        if let Some(v) = value.spell {
            writer.u16(v);
        }
        if let Some(v) = value.house_owner {
            writer.u32(v);
        }
        if let Some(v) = &value.house_restrictions {
            v.write(writer, max_permissions)?;
        }
        if let Some(v) = value.hook_item_types {
            writer.u32(v);
        }
        if let Some(v) = value.monarch {
            writer.u32(v);
        }
        if let Some(v) = value.hook_type {
            writer.u16(v);
        }
        if let Some(v) = value.icon_overlay {
            known_type(writer, v, 0x06000000)?;
        }
        if let Some(v) = value.icon_underlay {
            known_type(writer, v, 0x06000000)?;
        }
        if let Some(v) = value.material_type {
            writer.u32(v);
        }
        if let Some(v) = value.cooldown {
            writer.u32(v as u32);
        }
        if let Some(v) = value.cooldown_duration {
            writer.f64(v);
        }
        if let Some(v) = value.pet_owner {
            writer.u32(v);
        }
        writer.align4();
        Ok(())
    }
}
fn check_string(value: &str, max: usize) -> Result<(), WireError> {
    if value.chars().count() > max {
        Err(WireError::LimitExceeded)
    } else {
        Ok(())
    }
}
