//! ACE Player_Inventory.DoHandleActionGetAndWieldItem/CheckWeaponCollision and
//! Creature_Equipment.GetEquippedItems. Cold metadata is revision fenced by the
//! simulation owner; live mode and Aetheria flags come from the accepted actor.
use bace_types::EntityId;
const MELEE: u32 = 0x100000;
const SHIELD: u32 = 0x200000;
const MISSILE: u32 = 0x400000;
const AMMO: u32 = 0x800000;
const HELD: u32 = 0x1000000;
const TWO: u32 = 0x2000000;
const SELECTABLE_AMMO: u32 = MELEE | SHIELD | MISSILE | AMMO | HELD | TWO;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WieldSlotItem {
    pub item: EntityId,
    pub revision: u64,
    pub location: u32,
    pub valid: u32,
    pub clothing: bool,
    pub coverage: Option<u32>,
    pub item_type: u32,
    pub style: Option<u32>,
    pub weapon_skill: Option<u32>,
    pub ammo: Option<u32>,
    pub combat_use: Option<u32>,
}
impl WieldSlotItem {
    fn two(&self) -> bool {
        self.weapon_skill == Some(41)
    }
    fn caster(&self) -> bool {
        self.style == Some(0x200)
    }
    fn launcher(&self) -> bool {
        matches!(self.style, Some(0x10 | 0x20 | 0x400))
    }
    fn shield(&self) -> bool {
        self.combat_use == Some(4)
    }
    fn parent(&self, location: u32) -> u32 {
        match location {
            MELEE | HELD | TWO => 1,
            SHIELD => {
                if self.item_type == 2 {
                    3
                } else {
                    8
                }
            }
            MISSILE => {
                if matches!(self.style, Some(0x10 | 0x20)) {
                    2
                } else {
                    1
                }
            }
            _ => 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WieldSlotError {
    Invalid,
    Conflict,
    Locked,
    Capacity,
}
fn mainhand(gear: &[WieldSlotItem]) -> Option<&WieldSlotItem> {
    gear.iter()
        .find(|i| matches!(i.location, MELEE | TWO) && i.parent(i.location) == 1)
        .or_else(|| gear.iter().find(|i| i.location == MISSILE))
        .or_else(|| gear.iter().find(|i| i.location == HELD))
}
/// Original CheckWeaponCollision's new-item branch (combat-mode-only checks
/// belong to the combat action owner).
pub fn check_weapon_collision(
    item: &WieldSlotItem,
    location: u32,
    gear: &[WieldSlotItem],
    mode: u32,
) -> bool {
    let off = gear.iter().find(|i| i.location == SHIELD);
    let main = mainhand(gear);
    let ammo = gear.iter().find(|i| i.location == AMMO);
    match location {
        SHIELD => off.is_none() && main.is_none_or(|i| !i.two() && !i.caster() && !i.launcher()),
        MISSILE => {
            !(off.is_some() && item.launcher())
                && main.is_none()
                && !(item.ammo.is_some() && ammo.is_some_and(|a| a.ammo != item.ammo))
        }
        AMMO => {
            !main.is_some_and(|m| m.ammo.is_some() && item.ammo.is_some() && m.ammo != item.ammo)
        }
        TWO => off.is_none() && main.is_none(),
        MELEE => {
            off.is_none_or(|i| !i.two() && !i.caster() && !i.launcher())
                && main.is_none_or(|i| !i.two() && !i.caster() && !i.launcher())
        }
        HELD => {
            !(item.caster() && off.is_some())
                && main.is_none()
                && (mode == 1 || item.style.is_some())
        }
        _ => true,
    }
}
/// Returns normalized source location. No conflicting item is automatically
/// removed: ACE expects the client to send an explicit preceding dequip.
pub fn check_wield_slots(
    item: &WieldSlotItem,
    location: u32,
    gear: &[WieldSlotItem],
    mode: u32,
    aetheria: u32,
) -> Result<u32, WieldSlotError> {
    use WieldSlotError::*;
    if gear.len() > 64 {
        return Err(Capacity);
    }
    if item.item.0 == 0 || location == 0 || item.valid == 0 {
        return Err(Invalid);
    }
    if item.location & SELECTABLE_AMMO == 0 && !check_weapon_collision(item, location, gear, mode) {
        return Err(Conflict);
    }
    if location == SHIELD
        && !item.shield()
        && mainhand(gear).is_some_and(|m| m.two() || matches!(m.location, MISSILE | HELD))
    {
        return Err(Conflict);
    }
    if location == MISSILE && gear.iter().any(|g| g.location == SHIELD && !g.shield()) {
        return Err(Conflict);
    }
    if item.valid & location == 0 && !(item.valid == MELEE && location == SHIELD) {
        return Err(Invalid);
    }
    if location == MELEE && item.valid & location == 0 {
        return Err(Invalid);
    }
    for (slot, flag) in [(0x10000000, 1), (0x20000000, 2), (0x40000000, 4)] {
        if location & slot != 0 && aetheria & flag == 0 {
            return Err(Locked);
        }
    }
    let location = if item.clothing { item.valid } else { location };
    if item.two() && location & item.valid == 0 {
        return Err(Invalid);
    }
    if !wield_slot_available(item, location, gear) {
        return Err(Conflict);
    }
    Ok(location)
}
/// Source coverage/attachment occupancy test, independent of weapon conflicts.
pub fn wield_slot_available(item: &WieldSlotItem, location: u32, gear: &[WieldSlotItem]) -> bool {
    let conflict = if matches!(location, MELEE | HELD | TWO | SHIELD | MISSILE) {
        gear.iter().any(|g| {
            g.location != AMMO
                && g.parent(g.location) != 0
                && g.parent(g.location) == item.parent(location)
        })
    } else if item.clothing {
        gear.iter().any(|g| {
            g.coverage
                .is_some_and(|c| c & item.coverage.unwrap_or(0) != 0)
        })
    } else {
        gear.iter().any(|g| g.location & location != 0)
    };
    !conflict
}
