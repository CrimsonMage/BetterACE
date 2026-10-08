//! Prepared authoritative enchantment fields for network projection. These are
//! neither persistence DTOs nor client-supplied mutations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnchantmentProjection {
    pub spell_id: u16,
    pub layer: u16,
    pub category: u16,
    pub power: u32,
    pub start_time: f64,
    pub duration: f64,
    pub caster_id: u32,
    pub degrade_modifier: f32,
    pub degrade_limit: f32,
    pub last_time_degraded: f64,
    pub stat_type: u32,
    pub stat_key: u32,
    pub stat_value: f32,
    pub spell_set_id: u32,
}
