//! Pinned ACE DeathItem.cs and Player_Death.CalculateDeathItems. Selection is
//! immutable; creation, placement and destruction are one reserved transaction.
use bace_types::EntityId;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathPossession {
    pub id: EntityId,
    pub template: u32,
    pub item_type: u32,
    pub value: i32,
    pub stack: u32,
    pub wielded: bool,
    /// ACE BondedStatus: Destroy=-2, Slippery=-1, Normal=0, Bonded=1, Sticky=2.
    pub bonded: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathDrop {
    pub item: EntityId,
    pub amount: u32,
    /// Stack splits use a new template instance, never cloned enchantments.
    pub fresh_template: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathItemPlan {
    pub drops: Vec<DeathDrop>,
    pub destroyed: Vec<EntityId>,
    pub coins: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathItemError {
    Invalid,
    Capacity,
    Overflow,
    Random,
}
/// Category ordinals are frozen ACE DeathItemCategory values. Exact equality is
/// intentional: a combined ItemType mask is not one of these categories.
pub fn death_item_category(item_type: u32) -> u8 {
    match item_type {
        0x1 => 1,
        0x100 => 2,
        0x8000 | 0x200000 => 3,
        0x2 => 4,
        0x4 => 5,
        0x8 => 6,
        0x20 => 7,
        0x800 => 8,
        0x1000 => 9,
        0x80000 => 10,
        0x400000 | 0x800000 | 0x2000000 | 0x4000000 | 0x8000000 => 11,
        0x2000 => 12,
        0x4000 => 13,
        0x40000 => 14,
        0x80 => 15,
        _ => 0,
    }
}
/// Input order is the authoritative possession enumeration order; stable sorting
/// preserves ACE tie ordering. Draws occur after category/value sorting, including
/// candidates beyond the eventual drop count. No-drop/PKLite paths draw nothing.
pub fn select_death_items<F>(
    possessions: &[DeathPossession],
    level: u32,
    item_count: u32,
    coin_value: u32,
    suppress_drops: bool,
    mut variance: F,
) -> Result<DeathItemPlan, DeathItemError>
where
    F: FnMut() -> Result<f32, DeathItemError>,
{
    if possessions.len() > 4096 || item_count > 14 {
        return Err(DeathItemError::Capacity);
    }
    let mut ids = std::collections::BTreeSet::new();
    if level == 0
        || possessions
            .iter()
            .any(|p| p.id.0 == 0 || p.stack == 0 || p.stack > i32::MAX as u32 || !ids.insert(p.id))
    {
        return Err(DeathItemError::Invalid);
    }
    let mut plan = DeathItemPlan {
        drops: Vec::new(),
        destroyed: Vec::new(),
        coins: 0,
    };
    if suppress_drops {
        return Ok(plan);
    }
    let mut ranked: Vec<_> = possessions
        .iter()
        .filter(|p| p.template != 273 && (!p.wielded || level >= 35) && p.bonded == 0)
        .filter_map(|p| {
            let category = death_item_category(p.item_type);
            (category != 0).then_some((p, category, p.value / p.stack as i32))
        })
        .collect();
    ranked.sort_by_key(|(_, category, value)| (*category, std::cmp::Reverse(*value)));
    let mut previous = 0;
    for (_, category, value) in &mut ranked {
        if *category == previous {
            *value /= 2;
        } else {
            previous = *category;
        }
        let random = variance()?;
        if !random.is_finite() || !(-0.1..=0.1).contains(&random) {
            return Err(DeathItemError::Random);
        }
        let adjusted = *value as f32 + *value as f32 * random;
        if !adjusted.is_finite()
            || f64::from(adjusted) >= 2147483648.0
            || f64::from(adjusted) < -2147483648.0
        {
            return Err(DeathItemError::Overflow);
        }
        *value = adjusted as i32;
    }
    ranked.sort_by_key(|(_, _, value)| std::cmp::Reverse(*value));
    plan.drops.extend(
        ranked
            .into_iter()
            .take(item_count as usize)
            .map(|(p, _, _)| DeathDrop {
                item: p.id,
                amount: 1,
                fresh_template: p.stack > 1,
            }),
    );
    for p in possessions {
        if p.bonded == -2 {
            plan.destroyed.push(p.id);
        }
        if p.bonded == -1 {
            plan.drops.push(DeathDrop {
                item: p.id,
                amount: p.stack,
                fresh_template: false,
            });
        }
    }
    plan.coins = if level > 5 { coin_value / 2 } else { 0 };
    Ok(plan)
}
