//! Pinned ACE Player_Inventory.CheckWieldRequirements/CheckWieldRequirement.
//! Values come from the authoritative character projection at proposal time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WieldCriterion {
    pub kind: u32,
    pub key: i32,
    pub difficulty: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WieldPolicy {
    pub enabled: bool,
    pub actor: u32,
    pub heritage: u32,
    pub allowed_wielder: Option<u32>,
    pub heritage_specific_armor: Option<i32>,
    pub valid_locations: u32,
    pub criteria: [WieldCriterion; 4],
}
/// Skill getters must apply ACE ConvertToMoASkill (highest accepted melee or
/// Missile Weapons for retired skills). Missing attributes/vitals fail closed.
pub trait WieldValues {
    fn skill(&self, key: i32) -> Option<(u32, u32, u32)>;
    fn attribute(&self, key: i32) -> Option<(u32, u32)>;
    fn vital(&self, key: i32) -> Option<(u32, u32)>;
    fn level(&self) -> i32;
    fn int_property(&self, key: i32) -> i32;
    fn bool_property(&self, key: i32) -> bool;
    fn creature_type(&self) -> i32;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WieldFailure {
    SkillTooLow,
    LevelTooLow,
    NotOwner,
    HeritageRequiresArmor,
    ArmorRequiresHeritage,
    MissingValue,
}
impl WieldFailure {
    pub fn weenie_error(self) -> Option<u32> {
        Some(match self {
            Self::SkillTooLow => 0x468,
            Self::LevelTooLow => 0x420,
            Self::NotOwner => 0x4be,
            Self::HeritageRequiresArmor => 0x585,
            Self::ArmorRequiresHeritage => 0x586,
            Self::MissingValue => return None,
        })
    }
}
pub fn check_wield_requirements(
    policy: WieldPolicy,
    values: &impl WieldValues,
) -> Result<(), WieldFailure> {
    if !policy.enabled {
        return Ok(());
    }
    let matching = policy
        .heritage_specific_armor
        .is_some_and(|h| h as u32 == policy.heritage);
    if matches!(policy.heritage, 12 | 13)
        || policy.heritage == 6 && policy.valid_locations & 0x80007fff != 0
    {
        if !matching {
            return Err(WieldFailure::HeritageRequiresArmor);
        }
    } else if policy.heritage != 6 && policy.heritage_specific_armor.is_some() && !matching {
        return Err(WieldFailure::ArmorRequiresHeritage);
    }
    if policy.allowed_wielder.is_some_and(|id| id != policy.actor) {
        return Err(WieldFailure::NotOwner);
    }
    for criterion in policy.criteria {
        let difficulty = criterion.difficulty as u32;
        let key = criterion.key;
        let fail = match criterion.kind {
            1 | 2 | 8 => {
                let (base, current, training) =
                    values.skill(key).ok_or(WieldFailure::MissingValue)?;
                (match criterion.kind {
                    1 => current,
                    2 => base,
                    _ => training,
                }) < difficulty
            }
            3 | 4 => {
                let (base, current) = values.attribute(key).ok_or(WieldFailure::MissingValue)?;
                (if criterion.kind == 3 { current } else { base }) < difficulty
            }
            5 | 6 => {
                let (base, maximum) = values.vital(key).ok_or(WieldFailure::MissingValue)?;
                (if criterion.kind == 5 { maximum } else { base }) < difficulty
            }
            7 => {
                if i64::from(values.level()) < i64::from(difficulty) {
                    return Err(WieldFailure::LevelTooLow);
                }
                false
            }
            9 => i64::from(values.int_property(key)) < i64::from(difficulty),
            10 => values.bool_property(key) != (difficulty != 0),
            11 => i64::from(values.creature_type()) != i64::from(difficulty),
            12 => {
                if policy.heritage != difficulty {
                    return Err(WieldFailure::ArmorRequiresHeritage);
                }
                false
            }
            _ => false,
        };
        if fail {
            return Err(WieldFailure::SkillTooLow);
        }
    }
    Ok(())
}
