//! Source messages from ACE WorldObject_Use.CheckUseRequirements and enum extensions.
use crate::ActivationFailure;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivationMessage {
    Error(u32),
    ErrorWithString { error: u32, text: String },
    Transient(String),
}
fn skill(id: u32) -> String {
    match id {
        0 => "None".into(),
        1 => "Axe".into(),
        2 => "Bow".into(),
        3 => "Crossbow".into(),
        4 => "Dagger".into(),
        5 => "Mace".into(),
        6 => "Melee Defense".into(),
        7 => "Missile Defense".into(),
        8 => "Sling".into(),
        9 => "Spear".into(),
        10 => "Staff".into(),
        11 => "Sword".into(),
        12 => "Thrown Weapon".into(),
        13 => "Unarmed Combat".into(),
        14 => "Arcane Lore".into(),
        15 => "Magic Defense".into(),
        16 => "Mana Conversion".into(),
        17 => "Spellcraft".into(),
        18 => "Item Tinkering".into(),
        19 => "Assess Person".into(),
        20 => "Deception".into(),
        21 => "Healing".into(),
        22 => "Jump".into(),
        23 => "Lockpick".into(),
        24 => "Run".into(),
        25 => "Awareness".into(),
        26 => "Arms And Armor Repair".into(),
        27 => "Assess Creature".into(),
        28 => "Weapon Tinkering".into(),
        29 => "Armor Tinkering".into(),
        30 => "Magic Item Tinkering".into(),
        31 => "Creature Enchantment".into(),
        32 => "Item Enchantment".into(),
        33 => "Life Magic".into(),
        34 => "War Magic".into(),
        35 => "Leadership".into(),
        36 => "Loyalty".into(),
        37 => "Fletching".into(),
        38 => "Alchemy".into(),
        39 => "Cooking".into(),
        40 => "Salvaging".into(),
        41 => "Two Handed Combat".into(),
        42 => "Gearcraft".into(),
        43 => "Void Magic".into(),
        44 => "Heavy Weapons".into(),
        45 => "Light Weapons".into(),
        46 => "Finesse Weapons".into(),
        47 => "Missile Weapons".into(),
        48 => "Shield".into(),
        49 => "Dual Wield".into(),
        50 => "Recklessness".into(),
        51 => "Sneak Attack".into(),
        52 => "Dirty Fighting".into(),
        53 => "Challenge".into(),
        54 => "Summoning".into(),
        _ => id.to_string(),
    }
}
impl ActivationFailure {
    pub fn message(self, item_name: &str) -> Option<ActivationMessage> {
        use ActivationMessage as M;
        let (error, text) = match self {
            Self::SkillLow(id) => (0x4c9, skill(id)),
            Self::SkillUntrained(id) => {
                return Some(M::Transient(format!(
                    "You must have {} trained to use that item's magic",
                    skill(id)
                )));
            }
            Self::SkillUnspecialized(id) => (0x4cb, skill(id)),
            Self::Level => {
                return Some(M::Transient(
                    "You are not high enough level to use that!".into(),
                ));
            }
            Self::AttributeLow(id) => (
                0x4c9,
                match id {
                    0 => "Undef".into(),
                    1 => "Strength".into(),
                    2 => "Endurance".into(),
                    3 => "Quickness".into(),
                    4 => "Coordination".into(),
                    5 => "Focus".into(),
                    6 => "Self".into(),
                    _ => id.to_string(),
                },
            ),
            Self::VitalLow(id) => (
                0x4c9,
                match id {
                    0 => "Undef".into(),
                    1 => "Maximum Health".into(),
                    2 => "Health".into(),
                    3 => "Maximum Stamina".into(),
                    4 => "Stamina".into(),
                    5 => "Maximum Mana".into(),
                    6 => "Mana".into(),
                    _ => id.to_string(),
                },
            ),
            Self::Heritage(id) => (
                0x4c6,
                match id {
                    0 => "Invalid".into(),
                    1 => "Aluvian".into(),
                    2 => "Gharu'ndim".into(),
                    3 => "Sho".into(),
                    4 => "Viamontian".into(),
                    5 => "Umbraen".into(),
                    6 => "Gearknight".into(),
                    7 => "Tumerok".into(),
                    8 => "Lugian".into(),
                    9 => "Empyrean".into(),
                    10 => "Penumbraen".into(),
                    11 => "Undead".into(),
                    12 | 13 => "Olthoi".into(),
                    _ => id.to_string(),
                },
            ),
            Self::Cooldown => {
                return Some(M::Transient("You have used this item too recently".into()));
            }
            Self::OlthoiInteract => return Some(M::Error(0x587)),
            Self::OlthoiLifestone => return Some(M::Error(0x588)),
            Self::OlthoiVendor => return Some(M::Error(0x589)),
            Self::OlthoiCreature => {
                if item_name.len() > 512 {
                    return None;
                }
                (0x58a, item_name.into())
            }
            Self::MissingValue => return None,
        };
        Some(M::ErrorWithString { error, text })
    }
}
