//! Cold view of ACE requirements. Live proposals must re-evaluate the same
//! policy using current owner projections before reserving an equipment change.
use super::*;
use bace_content::WeenieV1;
use bace_inventory::{WieldCriterion, WieldFailure, WieldPolicy, WieldValues};
use bace_types::EntityId;
struct Values<'a> {
    player: &'a WeenieV1,
    state: &'a bace_simulation::OwnedPlayerState,
    stats: &'a PlayerStatProjection,
}
fn integer(w: &WeenieV1, key: i32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| i64::from(p.id) == i64::from(key))
        .map_or(0, |p| p.value)
}
impl WieldValues for Values<'_> {
    fn skill(&self, key: i32) -> Option<(u32, u32, u32)> {
        let mut mapped = u32::try_from(key).ok()?;
        match mapped {
            1 | 4 | 5 | 9 | 10 | 11 | 13 => {
                mapped = 45;
                let current = |id| {
                    self.stats
                        .physical_skills
                        .iter()
                        .find(|(skill, _)| *skill == id)
                        .map(|(_, s)| s.current)
                };
                for candidate in [44, 46] {
                    if current(candidate)? > current(mapped)? {
                        mapped = candidate;
                    }
                }
            }
            2 | 3 | 8 | 12 => mapped = 47,
            _ => {}
        }
        let input = self
            .stats
            .skills
            .inputs
            .iter()
            .find(|(id, _)| *id == mapped)?
            .1;
        let base = self
            .state
            .character
            .progression
            .skill_values(mapped, input)
            .ok()?;
        let current = self
            .stats
            .physical_skills
            .iter()
            .find(|(id, _)| *id == mapped)?
            .1;
        Some((base.base, current.current, current.advancement))
    }
    fn attribute(&self, key: i32) -> Option<(u32, u32)> {
        let index = usize::try_from(key.checked_sub(1)?).ok()?;
        Some((
            *self.stats.base_attributes.get(index)?,
            *self.stats.current_attributes.get(index)?,
        ))
    }
    fn vital(&self, key: i32) -> Option<(u32, u32)> {
        let index = match key {
            1 => 0,
            3 => 1,
            5 => 2,
            _ => return None,
        };
        Some((
            self.stats.base_vitals[index],
            self.stats.vitals[index].maximum,
        ))
    }
    fn level(&self) -> i32 {
        self.state
            .character
            .native_services
            .as_ref()
            .map_or(1, |s| i32::try_from(s.level).unwrap_or(i32::MAX))
    }
    fn int_property(&self, key: i32) -> i32 {
        integer(self.player, key)
    }
    fn bool_property(&self, key: i32) -> bool {
        self.player
            .properties
            .bools
            .iter()
            .any(|p| i64::from(p.id) == i64::from(key) && p.value)
    }
    fn creature_type(&self) -> i32 {
        integer(self.player, 2)
    }
}
pub(super) fn wield_requirements(
    actor: EntityId,
    item: &WeenieV1,
    player: &WeenieV1,
    state: &bace_simulation::OwnedPlayerState,
    stats: &PlayerStatProjection,
) -> Result<bool, String> {
    let policy = prepare_wield_policy(actor, item, integer(player, 188) as u32);
    match bace_inventory::check_wield_requirements(
        policy,
        &Values {
            player,
            state,
            stats,
        },
    ) {
        Ok(()) => Ok(true),
        Err(WieldFailure::MissingValue) => {
            Err("wield requirement references unavailable authoritative stat".into())
        }
        Err(_) => Ok(false),
    }
}

pub fn prepare_wield_policy(actor: EntityId, item: &WeenieV1, heritage: u32) -> WieldPolicy {
    WieldPolicy {
        enabled: true,
        actor: actor.0,
        heritage,
        allowed_wielder: item
            .properties
            .instance_ids
            .iter()
            .find(|p| p.id == 38)
            .map(|p| p.value),
        heritage_specific_armor: item
            .properties
            .ints
            .iter()
            .find(|p| p.id == 324)
            .map(|p| p.value),
        valid_locations: integer(item, 9) as u32,
        criteria: [158, 270, 273, 276].map(|base| WieldCriterion {
            kind: integer(item, base) as u32,
            key: integer(item, base + 1),
            difficulty: integer(item, base + 2),
        }),
    }
}

/// Immutable source slot metadata; accepted placement/revision are checked again
/// by simulation, including the entire currently equipped roster.
pub fn prepare_wield_slot(item: &bace_storage_codec::ItemSaveV4) -> bace_inventory::WieldSlotItem {
    let w = &item.entity.state;
    let optional = |key| {
        w.properties
            .ints
            .iter()
            .find(|p| p.id == key)
            .map(|p| p.value as u32)
    };
    bace_inventory::WieldSlotItem {
        item: EntityId(item.entity.object_id),
        revision: item.entity.mutation_revision,
        location: match item.placement {
            bace_storage_codec::ItemPlacementV2::Contained { equipped, .. } => equipped,
            _ => 0,
        },
        valid: optional(9).unwrap_or(0),
        clothing: w.weenie_type == 2,
        coverage: optional(4),
        item_type: optional(1).unwrap_or(0),
        style: optional(46),
        weapon_skill: optional(48),
        ammo: optional(50),
        combat_use: optional(51),
    }
}
