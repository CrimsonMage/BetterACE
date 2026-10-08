//! ACE Lifestone/Bindstone.ActOnUse eligibility and final-radius effects.
use crate::{PortalPosition, RecallError, RecallPolicy};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingKind {
    Lifestone,
    Allegiance,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BindingEffect {
    pub kind: BindingKind,
    pub sanctuary: PortalPosition,
    pub stamina_after: Option<u32>,
}
pub fn check_binding(
    policy: &RecallPolicy,
    kind: BindingKind,
    allegiance: bool,
    permission: u32,
) -> Result<(), RecallError> {
    if !match kind {
        BindingKind::Lifestone => policy.lifestone_binding,
        BindingKind::Allegiance => policy.allegiance_binding,
    } {
        return Err(RecallError::Disabled);
    }
    if kind == BindingKind::Allegiance {
        if !allegiance {
            return Err(RecallError::NoAllegiance);
        }
        if permission < 2 {
            return Err(RecallError::Invalid);
        }
    }
    Ok(())
}
pub fn complete_binding(
    kind: BindingKind,
    accepted_position: PortalPosition,
    within_use_radius: bool,
    stamina: u32,
) -> Result<BindingEffect, RecallError> {
    accepted_position
        .validate()
        .map_err(|_| RecallError::Invalid)?;
    if !within_use_radius {
        return Err(RecallError::MovedTooFar);
    }
    Ok(BindingEffect {
        kind,
        sanctuary: accepted_position,
        stamina_after: (kind == BindingKind::Lifestone)
            .then(|| ((stamina as f32 / 2.).round_ties_even()) as u32),
    })
}
