//! Restart-only world policy. Configuration never grants geometry admission.
use crate::{ConfigError, world_defaults};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    pub zones: ZoneRulesConfig,
    pub preloading: PreloadingConfig,
    pub recalls: RecallConfig,
    pub death: PlayerDeathConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ZoneRulesConfig {
    /// Whole-list replacement. An explicit empty list disables this zone rule.
    pub no_relog: Vec<u16>,
    /// Prevents player death inventory loss, not corpse creation.
    pub no_death_item_drop: Vec<u16>,
    /// Suppresses kill XP and kill luminance, not quest rewards.
    pub no_kill_experience: Vec<u16>,
}
impl Default for ZoneRulesConfig {
    fn default() -> Self {
        Self {
            no_relog: world_defaults::NO_RELOG.to_vec(),
            no_death_item_drop: world_defaults::NO_DEATH_ITEM_DROP.to_vec(),
            no_kill_experience: world_defaults::NO_KILL_EXPERIENCE.to_vec(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreloadEntry {
    /// 16-bit landblock, e.g. 0xE74E, not a 32-bit cell identifier.
    pub landblock: Option<u16>,
    /// Explicit equivalent of ACE's description-selected apartment group.
    pub apartment_group: bool,
    pub description: String,
    pub enabled: bool,
    pub permanent: bool,
    pub include_adjacent: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PreloadingConfig {
    pub enabled: bool,
    pub max_resident_regions: usize,
    pub entries: Vec<PreloadEntry>,
}
impl Default for PreloadingConfig {
    fn default() -> Self {
        let rows = [
            (
                Some(0xE74E),
                false,
                "Hebian-To (Global Events)",
                true,
                false,
            ),
            (Some(0xA9B4), false, "Holtburg", false, true),
            (Some(0xDA55), false, "Shoushi", false, true),
            (Some(0x7D64), false, "Yaraq", false, true),
            (Some(0x0007), false, "Town Network", false, false),
            (None, true, "Apartment Landblocks", false, false),
        ];
        Self {
            enabled: true,
            max_resident_regions: 1024,
            entries: rows
                .into_iter()
                .map(
                    |(landblock, apartment_group, description, enabled, include_adjacent)| {
                        PreloadEntry {
                            landblock,
                            apartment_group,
                            description: description.into(),
                            enabled,
                            permanent: true,
                            include_adjacent,
                        }
                    },
                )
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecallConfig {
    pub lifestone: bool,
    pub house: bool,
    pub marketplace: bool,
    pub allegiance_hometown: bool,
    pub allegiance_housing: bool,
    pub pk_arena: bool,
    pub pkl_arena: bool,
    pub spell_recalls: bool,
    pub lifestone_binding: bool,
    pub allegiance_binding: bool,
    pub pk_timer_seconds: u32,
}
impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            lifestone: true,
            house: true,
            marketplace: true,
            allegiance_hometown: true,
            allegiance_housing: true,
            pk_arena: true,
            pkl_arena: true,
            spell_recalls: true,
            lifestone_binding: true,
            allegiance_binding: true,
            pk_timer_seconds: 20,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerDeathConfig {
    pub vitae_penalty: f64,
    pub vitae_penalty_max: f64,
    pub destroy_pyreals: bool,
    pub lifestone_broadcast: bool,
    pub pk_respite_seconds: u32,
    pub pk_server: bool,
    pub pkl_server: bool,
    pub safe_training_academy: bool,
    /// Pinned ACE option used when selecting plain Wield CreateList rows for
    /// death treasure. Restart-only; NoCorpse and ordinary creature death
    /// must use the same accepted value.
    pub creatures_drop_createlist_wield: bool,
}
impl Default for PlayerDeathConfig {
    fn default() -> Self {
        Self {
            vitae_penalty: 0.05,
            vitae_penalty_max: 0.40,
            destroy_pyreals: true,
            lifestone_broadcast: true,
            pk_respite_seconds: 300,
            pk_server: false,
            pkl_server: false,
            safe_training_academy: false,
            creatures_drop_createlist_wield: false,
        }
    }
}

impl WorldConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for zones in [
            &self.zones.no_relog,
            &self.zones.no_death_item_drop,
            &self.zones.no_kill_experience,
        ] {
            if zones.len() > 4096
                || zones.iter().copied().collect::<BTreeSet<_>>().len() != zones.len()
            {
                return Err(ConfigError::Invalid(
                    "world zone lists must be unique and bounded to 4096",
                ));
            }
        }
        if self.preloading.entries.len() > 256
            || !(1..=4096).contains(&self.preloading.max_resident_regions)
        {
            return Err(ConfigError::Invalid(
                "world preloading capacity exceeds supported bounds",
            ));
        }
        let mut targets = BTreeSet::new();
        for entry in &self.preloading.entries {
            if entry.landblock.is_some() == entry.apartment_group
                || entry.description.len() > 256
                || !targets.insert((entry.landblock, entry.apartment_group))
            {
                return Err(ConfigError::Invalid(
                    "preload entry must select one unique bounded target",
                ));
            }
        }
        if self.recalls.pk_timer_seconds > 86400
            || self.death.pk_respite_seconds > 86400
            || self.death.pk_server && self.death.pkl_server
            || !self.death.vitae_penalty.is_finite()
            || !self.death.vitae_penalty_max.is_finite()
            || !(0.0..=1.0).contains(&self.death.vitae_penalty)
            || !(0.0..1.0).contains(&self.death.vitae_penalty_max)
            || self.death.vitae_penalty > self.death.vitae_penalty_max
        {
            return Err(ConfigError::Invalid("invalid recall/death policy"));
        }
        Ok(())
    }
}
