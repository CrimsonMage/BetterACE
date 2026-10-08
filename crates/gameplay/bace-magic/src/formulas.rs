//! ACE SkillCheck, WorldObject.MagicDefenseCheck and Creature.GetManaCost,
//! pin 47edade3bd3f6044b676d4eb877c4965c7eda62b, AGPL-3.0-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicFormulaError {
    InvalidInput,
    InvalidSkill,
    InvalidDraw,
    MissingDraw,
}
pub fn component_burn_rate(
    component_loss: f32,
    destruction_modifier: f32,
    power: u32,
    skill: u32,
) -> Result<f32, MagicFormulaError> {
    if skill == 0 {
        return Err(MagicFormulaError::InvalidSkill);
    }
    if [component_loss, destruction_modifier]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return Err(MagicFormulaError::InvalidDraw);
    }
    let rate = component_loss * destruction_modifier * (power as f32 / skill as f32).min(1.0);
    if !rate.is_finite() {
        return Err(MagicFormulaError::InvalidDraw);
    }
    Ok(rate)
}
fn chance(skill: u32, difficulty: u32, factor: f32) -> Result<f64, MagicFormulaError> {
    let skill = i32::try_from(skill).map_err(|_| MagicFormulaError::InvalidSkill)?;
    let difficulty = i32::try_from(difficulty).map_err(|_| MagicFormulaError::InvalidSkill)?;
    let difference = skill
        .checked_sub(difficulty)
        .ok_or(MagicFormulaError::InvalidSkill)?;
    Ok((1.0 - 1.0 / (1.0 + f64::from(factor * difference as f32).exp())).clamp(0.0, 1.0))
}
pub fn cast_chance(skill: u32, difficulty: u32) -> Result<f64, MagicFormulaError> {
    chance(skill, difficulty, 0.07)
}
pub fn resisted(skill: u32, defense: u32, draw: f32) -> Result<(bool, f32), MagicFormulaError> {
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(MagicFormulaError::InvalidDraw);
    }
    let chance = chance(skill, defense, 0.03)?;
    Ok((chance <= f64::from(draw), (1.0 - chance) as f32))
}
/// Returns cost and exact number of consumed random draws (0, 2, or 3).
pub fn mana_cost(
    difficulty: u32,
    mut cost: u32,
    conversion: u32,
    draws: &[f32],
) -> Result<(u32, usize), MagicFormulaError> {
    if conversion == 0 {
        return Ok((cost, 0));
    }
    let draw = |index: usize| -> Result<f32, MagicFormulaError> {
        let value = *draws.get(index).ok_or(MagicFormulaError::MissingDraw)?;
        if !value.is_finite() || !(0.0..1.0).contains(&value) {
            Err(MagicFormulaError::InvalidDraw)
        } else {
            Ok(value)
        }
    };
    let mut success = chance(conversion, difficulty / 2, 0.03)?;
    let mut roll = draw(0)?;
    let luck = draw(1)?;
    if f64::from(roll) < success {
        cost =
            (f64::from(cost) * (1.0 - (success - f64::from(roll * luck)))).round_ties_even() as u32;
    }
    let mut used = 2;
    if cost > 1 {
        success = chance(conversion, difficulty, 0.03)?;
        roll = draw(2)?;
        used = 3;
        if f64::from(roll) < success {
            cost = (f64::from(cost) * (1.0 - (success - f64::from(roll * luck)))).round_ties_even()
                as u32;
        }
    }
    Ok((cost.max(1), used))
}
