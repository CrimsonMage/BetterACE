//! Explicit cold startup policy. Gameplay implementations remain in their owners;
//! this adapter only joins verified DAT tables and indexed accepted content.
#[derive(Clone, Copy, Debug)]
pub struct StartupAssetPolicy {
    pub world_open: bool,
    pub local_offset_seconds: i32,
    pub player: crate::game_runtime::PlayerLoginPolicy,
    pub creature_think_interval: u64,
    pub creature_corpse_decay_ticks: u64,
    /// ACE PropertyManager aetheria_drop_rate is a multiplier, with default 1.
    pub aetheria_drop_rate: f32,
    /// Accepted namespace-52 ACE table-set key. One selection per game child.
    pub treasure_table_set_id: u32,
}
impl Default for StartupAssetPolicy {
    fn default() -> Self {
        Self {
            world_open: true,
            local_offset_seconds: 0,
            player: Default::default(),
            // GDLE CMonsterWeenie::Tick calls the AI each world tick.
            creature_think_interval: 1,
            // Pinned GDLE Corpse.cpp CORPSE_EXIST_TIME = 180 seconds.
            creature_corpse_decay_ticks: 180 * 30,
            aetheria_drop_rate: 1.,
            treasure_table_set_id: 1,
        }
    }
}
