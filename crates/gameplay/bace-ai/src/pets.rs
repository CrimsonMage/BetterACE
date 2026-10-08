//! GDLE weenie/PetDevice.cpp requirements/ownership with ACE Pet.cs missing
//! lifecycle completion. No peer position, clocks, inventory or world mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetUseError {
    PortalSpace,
    NotOwned,
    Untrained,
    Unspecialized,
    Skill,
    Level,
    Mastery,
    NoCharges,
    ActivePet,
    Cooldown,
    InvalidProfile,
}
#[derive(Clone, Copy, Debug)]
pub struct PetUseRequirements {
    pub skill_required: bool,
    pub skill_level: u32,
    pub level: u32,
    pub mastery: u32,
    pub unlimited: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct PetUser {
    pub portal_space: bool,
    pub owns_device: bool,
    /// AC advancement 0 inactive, 1 untrained, 2 trained, 3 specialized.
    pub advancement: u32,
    pub skill: u32,
    pub level: u32,
    pub mastery: u32,
    pub charges: u32,
    pub active_combat_pet: bool,
    pub cooldown_active: bool,
}
impl PetUseRequirements {
    pub fn check(self, user: PetUser) -> Result<(), PetUseError> {
        if user.portal_space {
            return Err(PetUseError::PortalSpace);
        }
        if !user.owns_device {
            return Err(PetUseError::NotOwned);
        }
        if user.advancement > 3 || self.mastery > 3 {
            return Err(PetUseError::InvalidProfile);
        }
        if self.skill_required {
            if user.advancement < 2 {
                return Err(PetUseError::Untrained);
            }
            if self.skill_level == 570 && user.advancement != 3 {
                return Err(PetUseError::Unspecialized);
            }
            if user.skill < self.skill_level {
                return Err(PetUseError::Skill);
            }
        }
        if user.level < self.level {
            return Err(PetUseError::Level);
        }
        if self.mastery != 0 && user.mastery != self.mastery {
            return Err(PetUseError::Mastery);
        }
        if user.active_combat_pet {
            return Err(PetUseError::ActivePet);
        }
        if user.cooldown_active {
            return Err(PetUseError::Cooldown);
        }
        if !self.unlimited && user.charges == 0 {
            return Err(PetUseError::NoCharges);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatPetLifetime {
    pub owner: u32,
    pub device: u32,
    pub expires: f64,
}
impl CombatPetLifetime {
    pub fn new(owner: u32, device: u32, lifetime: f64, now: f64) -> Result<Self, PetUseError> {
        if owner == 0
            || device == 0
            || !now.is_finite()
            || now < 0.0
            || !lifetime.is_finite()
            || !(0.0..=86400.0).contains(&lifetime)
            || lifetime == 0.0
        {
            return Err(PetUseError::InvalidProfile);
        }
        Ok(Self {
            owner,
            device,
            expires: now + lifetime,
        })
    }
    pub fn expired(self, now: f64, owner_present: bool) -> bool {
        !owner_present || now >= self.expires
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PetTarget {
    pub id: u32,
    pub creature: bool,
    pub player: bool,
    pub combat_pet: bool,
    pub alive: bool,
    pub attackable: bool,
    pub visible: bool,
    pub same_faction: bool,
    pub retaliating: bool,
    pub distance_squared: f32,
}
/// GDLE forbids players/pets; ACE supplies visibility/faction safeguards. The
/// source nearest-target loop's early break is not reproduced as a requirement.
pub fn combat_pet_target(candidates: &[PetTarget], range: f32) -> Result<Option<u32>, PetUseError> {
    if candidates.len() > 4096
        || !range.is_finite()
        || range < 0.0
        || candidates
            .iter()
            .any(|c| !c.distance_squared.is_finite() || c.distance_squared < 0.0)
    {
        return Err(PetUseError::InvalidProfile);
    }
    Ok(candidates
        .iter()
        .filter(|c| {
            c.id != 0
                && c.creature
                && !c.player
                && !c.combat_pet
                && c.alive
                && c.attackable
                && c.visible
                && (!c.same_faction || c.retaliating)
                && c.distance_squared <= range * range
        })
        .min_by(|a, b| {
            a.distance_squared
                .total_cmp(&b.distance_squared)
                .then(a.id.cmp(&b.id))
        })
        .map(|c| c.id))
}
