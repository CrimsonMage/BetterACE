//! Immutable owner-prepared spell impact qualities. Raw resistance and rating
//! inputs exclude the live registry, which the simulation composes at delivery.
use bace_gameplay_api::weapon_combat::{
    PhysicalQuality, PhysicalRatings, PhysicalResistance, PhysicalSkill,
};
#[derive(Clone, Debug, PartialEq)]
pub struct MagicWand {
    pub entity: u32,
    pub revision: u64,
    pub damage_type: u32,
    pub elemental_modifier: f64,
    pub elemental_present: bool,
    pub inherit_wielder: bool,
    pub imbues: u32,
    pub biting: f64,
    pub double_enchant_biting: bool,
    pub crushing: f64,
    pub double_enchant_crushing: bool,
    pub slayer_type: u32,
    pub slayer_bonus: f64,
    pub resistance_cleaving: Option<u32>,
    pub ignore_magic_resistance: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MagicDamageProfile {
    pub player: bool,
    pub spellcraft: Option<u32>,
    pub elemental_modifier: f64,
    pub creature_type: u32,
    pub current_enemy: Option<u32>,
    pub base_strength: u32,
    pub base_endurance: u32,
    pub ratings: PhysicalRatings,
    pub rating_properties: Vec<(u32, i32)>,
    pub magic_defense: PhysicalSkill,
    pub sneak: PhysicalSkill,
    pub deception: PhysicalSkill,
    pub assess_person: PhysicalSkill,
    /// Slash, pierce, bludgeon, fire, cold, acid, electric, nether, health drain,
    /// stamina drain, mana drain. Enchantment details retain separate prot/vuln.
    pub resistances: [PhysicalResistance; 11],
    pub augmentation_family: bool,
    pub critical_defense: bool,
    pub ignore_magic_resistance: bool,
    pub wand: Option<MagicWand>,
    pub cloak: Option<crate::MagicCloak>,
    pub sigils: Vec<crate::MagicSigil>,
    pub proc_items: Vec<MagicProcItem>,
    pub healing_ratings: [i32; 3],
    pub boost_resistances: [f64; 3],
    pub dot_ratings: [u32; 2],
    /// Equipped combat missile weapon with IgnoreSomeMagicProjectileDamage.
    pub missile_absorption: bool,
    /// Equipped shield (base skill, advancement, ABSORB_MAGIC_DAMAGE_FLOAT).
    pub shield: Option<(PhysicalSkill, f32)>,
    pub shield_entity: Option<u32>,
}
impl MagicDamageProfile {
    /// Neutral values for an explicitly prepared actor; never an authentic-data
    /// fallback. Adapters must populate the source's actual qualities/equipment.
    pub fn neutral(player: bool) -> Self {
        let skill = PhysicalSkill {
            advancement: 0,
            current: 0,
        };
        Self {
            player,
            spellcraft: None,
            elemental_modifier: 0.,
            creature_type: 0,
            current_enemy: None,
            base_strength: 0,
            base_endurance: 0,
            ratings: PhysicalRatings::default(),
            rating_properties: Vec::new(),
            magic_defense: skill,
            sneak: skill,
            deception: skill,
            assess_person: skill,
            resistances: [PhysicalResistance {
                quality: PhysicalQuality {
                    raw: 1.,
                    increasing: 1.,
                    decreasing: 1.,
                    additive_increasing: 0.,
                    additive_decreasing: 0.,
                },
                augmentation: 0,
            }; 11],
            augmentation_family: false,
            critical_defense: false,
            ignore_magic_resistance: false,
            wand: None,
            cloak: None,
            sigils: Vec::new(),
            proc_items: Vec::new(),
            healing_ratings: [0; 3],
            boost_resistances: [1.; 3],
            dot_ratings: [0; 2],
            missile_absorption: false,
            shield: None,
            shield_entity: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MagicDamageRolls {
    pub variance: f64,
    pub critical: f64,
    pub critical_defense: f32,
    pub sneak: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MagicProcItem {
    pub item: u32,
    pub spellcraft: Option<u32>,
    /// War, Life, Creature, Item, Void as MagicSchool indexes.
    pub skills: [u32; 5],
    pub cloak: bool,
    pub wield_difficulty: Option<u32>,
}
/// GDLE DetermineSkillLevelForSpell; source ownership is validated by the caller.
pub fn magic_item_skill(
    item: &MagicProcItem,
    school: Option<crate::MagicSchool>,
    category: u16,
) -> u32 {
    if (683..=686).contains(&category) {
        return 1;
    }
    item.spellcraft.unwrap_or_else(|| {
        school.map_or_else(
            || item.skills.into_iter().max().unwrap_or(0),
            |s| item.skills[s as usize - 1],
        )
    })
}
/// SpellProjectile's cloak Life exception uses the item, never the player's rank.
pub fn magic_cloak_projectile_skill(
    item: &MagicProcItem,
    spell: u32,
    skill: u32,
) -> Result<u32, crate::EffectError> {
    if item.cloak && matches!(spell, 1783..=1789 | 5331) {
        item.wield_difficulty
            .map(|n| n.checked_mul(3).ok_or(crate::EffectError::Overflow))
            .transpose()
            .map(|v| v.unwrap_or(150))
    } else {
        Ok(skill)
    }
}
