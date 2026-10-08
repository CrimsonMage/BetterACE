//! Pinned ACE WorldObject_Use.CheckUseRequirements, in source evaluation order.
use crate::WieldValues;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivationObject {
    #[default]
    Ordinary,
    Creature {
        olthoi: bool,
        vendor: bool,
        looks_like_object: bool,
    },
    Lifestone,
    RestrictedOlthoi,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivationRequirements {
    pub difficulty: Option<i32>,
    pub item_skill: Option<i32>,
    pub item_skill_level: Option<i32>,
    pub skill: Option<i32>,
    pub skill_level: Option<i32>,
    pub specialized: Option<i32>,
    pub item_specialized: Option<i32>,
    pub level: Option<i32>,
    pub attribute: Option<i32>,
    pub attribute_level: Option<i32>,
    pub vital: Option<i32>,
    pub vital_level: Option<i32>,
    pub heritage: u32,
    pub cooldown: Option<i32>,
    pub object: ActivationObject,
}
pub trait ActivationValues: WieldValues {
    fn heritage(&self) -> u32;
    fn mapped_skill(&self, skill: i32) -> Option<u32>;
    fn cooldown_ready(&self, group: Option<i32>) -> bool;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationFailure {
    SkillLow(u32),
    SkillUntrained(u32),
    SkillUnspecialized(u32),
    Level,
    AttributeLow(i32),
    VitalLow(i32),
    Heritage(u32),
    Cooldown,
    OlthoiInteract,
    OlthoiLifestone,
    OlthoiVendor,
    OlthoiCreature,
    MissingValue,
}
pub fn check_item_activation(
    r: &ActivationRequirements,
    v: &impl ActivationValues,
) -> Result<(), ActivationFailure> {
    let skill = |key: i32| -> Result<(u32, u32, u32), ActivationFailure> {
        let id = v.mapped_skill(key).ok_or(ActivationFailure::MissingValue)?;
        let (_, current, advancement) = v.skill(key).ok_or(ActivationFailure::MissingValue)?;
        Ok((id, current, advancement))
    };
    let low = |current: u32, limit: i32| i64::from(current) < i64::from(limit);
    if let Some(limit) = r.difficulty {
        let (id, current, _) = skill(14)?;
        if low(current, limit) {
            return Err(ActivationFailure::SkillLow(id));
        }
    }
    if let (Some(key), Some(limit)) = (r.item_skill, r.item_skill_level) {
        let (id, current, _) = skill(key)?;
        if low(current, limit) {
            return Err(ActivationFailure::SkillLow(id));
        }
    }
    if let Some(key) = r.skill {
        let (id, current, advancement) = skill(key)?;
        if advancement < 2 {
            return Err(ActivationFailure::SkillUntrained(id));
        }
        if r.skill_level.is_some_and(|limit| low(current, limit)) {
            return Err(ActivationFailure::SkillLow(id));
        }
    }
    for (key, limit) in [
        (r.specialized, r.skill_level),
        (r.item_specialized, r.item_skill_level),
    ] {
        if let Some(key) = key {
            let (id, current, advancement) = skill(key)?;
            if advancement < 3 {
                return Err(ActivationFailure::SkillUnspecialized(id));
            }
            if limit.is_some_and(|limit| low(current, limit)) {
                return Err(ActivationFailure::SkillLow(id));
            }
        }
    }
    if r.level.is_some_and(|level| v.level() < level) {
        return Err(ActivationFailure::Level);
    }
    if let Some(key) = r.attribute {
        let (_, current) = v.attribute(key).ok_or(ActivationFailure::MissingValue)?;
        if r.attribute_level.is_some_and(|limit| low(current, limit)) {
            return Err(ActivationFailure::AttributeLow(key));
        }
    }
    if let Some(key) = r.vital {
        let (_, current) = v.vital(key).ok_or(ActivationFailure::MissingValue)?;
        if r.vital_level.is_some_and(|limit| low(current, limit)) {
            return Err(ActivationFailure::VitalLow(key));
        }
    }
    if r.heritage != 0
        && !matches!(r.object, ActivationObject::Creature { .. })
        && v.heritage() != r.heritage
    {
        return Err(ActivationFailure::Heritage(r.heritage));
    }
    if !v.cooldown_ready(r.cooldown) {
        return Err(ActivationFailure::Cooldown);
    }
    if matches!(v.heritage(), 12 | 13) {
        match r.object {
            ActivationObject::Creature { olthoi: true, .. } => {}
            ActivationObject::Creature { vendor: true, .. } => {
                return Err(ActivationFailure::OlthoiVendor);
            }
            ActivationObject::Creature {
                looks_like_object: true,
                ..
            }
            | ActivationObject::RestrictedOlthoi => return Err(ActivationFailure::OlthoiInteract),
            ActivationObject::Creature { .. } => return Err(ActivationFailure::OlthoiCreature),
            ActivationObject::Lifestone => return Err(ActivationFailure::OlthoiLifestone),
            ActivationObject::Ordinary => {}
        }
    }
    Ok(())
}
