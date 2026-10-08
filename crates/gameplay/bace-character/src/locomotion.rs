//! Pinned ACE EncumbranceSystem and MovementSystem scalar formulas (47edade3).
//! Copyright ACE contributors; AGPL-3.0-only. No client velocity is authoritative.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Encumbrance {
    pub capacity: u32,
    pub maximum_inventory_burden: u64,
    pub ratio: f32,
    pub modifier: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocomotionError {
    InvalidInput,
    Overflow,
    TooTired,
}
pub fn encumbrance(
    strength: u32,
    carrying_augmentations: u32,
    burden: u64,
) -> Result<Encumbrance, LocomotionError> {
    let capacity = strength
        .checked_mul(150 + 30 * carrying_augmentations.min(5))
        .filter(|v| *v <= i32::MAX as u32)
        .ok_or(LocomotionError::Overflow)?;
    if burden > i32::MAX as u64 {
        return Err(LocomotionError::Overflow);
    }
    let ratio = if capacity == 0 {
        3.0
    } else {
        burden as f32 / capacity as f32
    };
    Ok(Encumbrance {
        capacity,
        maximum_inventory_burden: u64::from(capacity) * 3,
        ratio,
        modifier: burden_modifier(ratio),
    })
}
fn burden_modifier(burden: f32) -> f32 {
    if burden < 1.0 {
        1.0
    } else if burden < 2.0 {
        2.0 - burden
    } else {
        0.0
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RunInput {
    pub skill: u32,
    pub burden: f32,
    pub scale: f32,
    pub exhausted: bool,
}
/// Preserve ACE's >=800 branch, including its independence from scale/burden.
pub fn run_rate(input: RunInput) -> Result<f32, LocomotionError> {
    if input.skill > i32::MAX as u32
        || !input.burden.is_finite()
        || input.burden < 0.0
        || !input.scale.is_finite()
        || input.scale <= 0.0
    {
        return Err(LocomotionError::InvalidInput);
    }
    let burden = if input.exhausted { 3.0 } else { input.burden };
    let rate = if input.skill >= 800 {
        18.0 / 4.0
    } else {
        ((burden_modifier(burden) * (input.skill as f32 / (input.skill as f32 + 200.0) * 11.0)
            + 4.0)
            / input.scale)
            / 4.0
    };
    if !rate.is_finite() {
        return Err(LocomotionError::Overflow);
    }
    Ok(rate)
}
#[derive(Clone, Copy, Debug)]
pub struct JumpInput {
    pub skill: u32,
    pub burden: f32,
    pub scale: f32,
    pub extent: f32,
    pub stamina: u32,
    pub pk_timer_active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpProposal {
    pub height: f32,
    pub stamina_cost: u32,
    pub remaining_stamina: u32,
}
/// Authority hardening: insufficient stamina rejects atomically (WeenieError
/// 0x3e); callers never forward an untrusted client jump velocity.
pub fn jump_proposal(input: JumpInput) -> Result<JumpProposal, LocomotionError> {
    if !input.extent.is_finite()
        || !input.burden.is_finite()
        || input.burden < 0.0
        || !input.scale.is_finite()
        || input.scale <= 0.0
    {
        return Err(LocomotionError::InvalidInput);
    }
    let power = input.extent.clamp(0.0, 1.0);
    let raw_cost = if input.pk_timer_active {
        ((power + 1.0) * 100.0).trunc()
    } else {
        ((input.burden + 0.5) * power * 8.0 + 2.0).ceil()
    };
    if !raw_cost.is_finite() || raw_cost > i32::MAX as f32 {
        return Err(LocomotionError::Overflow);
    }
    let stamina_cost = raw_cost as u32;
    let remaining_stamina = input
        .stamina
        .checked_sub(stamina_cost)
        .ok_or(LocomotionError::TooTired)?;
    let height = (burden_modifier(input.burden)
        * (input.skill as f32 / (input.skill as f32 + 1300.0) * 22.2 + 0.05)
        * power
        / input.scale)
        .max(0.35);
    if !height.is_finite() {
        return Err(LocomotionError::Overflow);
    }
    Ok(JumpProposal {
        height,
        stamina_cost,
        remaining_stamina,
    })
}
