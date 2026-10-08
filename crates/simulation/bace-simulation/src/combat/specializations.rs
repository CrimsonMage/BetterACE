//! Derived combat views are refreshed from the character owner. They contain no
//! spendable state and cannot authorize a progression transition.
use super::*;
use bace_character::{SkillValueInputs, SkillValues};
use bace_combat::specialization::*;
use bace_gameplay_api::SkillAdvancement;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedShield {
    /// Authored armor/resistance and current item enchantments, before skill cap.
    pub effective_armor: f32,
    pub magic_absorption: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedAttributeModifier {
    pub multiplier: f32,
    pub additive: i32,
}
impl Default for PreparedAttributeModifier {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            additive: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedCharacterSkillInputs {
    pub attack_skill: u32,
    /// One per character skill. Formula/modifier input comes from prepared DAT,
    /// current authoritative attributes, augmentations and enchantments.
    pub inputs: Vec<(u32, SkillValueInputs)>,
    pub shield: Option<PreparedShield>,
    pub attribute_modifiers: [PreparedAttributeModifier; 6],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillRefreshError {
    MissingActor,
    MissingSkill,
    InvalidInput,
    Busy,
    Capacity,
}
#[derive(Clone, Debug)]
pub(crate) struct CombatSkills {
    pub prepared: PreparedCharacterSkillInputs,
    pub values: BTreeMap<u32, CombatSkill>,
    pub revision: u64,
    pub registry_revision: Option<u64>,
    pub dirty_signature: [i32; 2],
    pub power: f32,
}
impl CombatSkills {
    pub fn skill(&self, id: u32) -> CombatSkill {
        self.values.get(&id).copied().unwrap_or(CombatSkill {
            advancement: SkillAdvancement::Inactive,
            base: 0,
            current: 0,
        })
    }
    pub fn attack(&self) -> u32 {
        self.skill(self.prepared.attack_skill).current
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyFightingImpact {
    pub attacker: EntityId,
    pub target: EntityId,
    pub spells: [u32; 2],
    pub count: usize,
}
impl Combat {
    /// Character owner projections update derived combat inputs without changing
    /// equipment identity or the already admitted animation rate. Future rates
    /// require an exact prepared DAT chain; a missing variant fails closed.
    pub(crate) fn refresh_physical_skills(
        &mut self,
        actor: EntityId,
        values: &[SkillValues],
        current_attributes: [u32; 6],
        base_attributes: [u32; 6],
    ) -> Result<(), SkillRefreshError> {
        let Some(previous) = self.physical.get(&actor) else {
            return Ok(());
        };
        if values.len() > 256
            || values
                .iter()
                .enumerate()
                .any(|(i, v)| values[..i].iter().any(|old| old.skill == v.skill))
        {
            return Err(SkillRefreshError::InvalidInput);
        }
        let mut profile = (**previous).clone();
        profile.skills = values
            .iter()
            .map(|v| {
                (
                    v.skill,
                    bace_gameplay_api::weapon_combat::PhysicalSkill {
                        advancement: v.advancement as u32,
                        current: v.current,
                    },
                )
            })
            .collect();
        profile.skills.sort_by_key(|v| v.0);
        profile.strength = current_attributes[0];
        profile.quickness = current_attributes[2];
        profile.coordination = current_attributes[3];
        profile.base_strength = base_attributes[0];
        profile.base_endurance = base_attributes[1];
        if let Some((_, skill)) = profile.skills.iter().find(|(id, _)| *id == 48) {
            profile.shield_skill = *skill;
        }
        bace_combat::physical::validate_physical_profile(&profile)
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        let refreshed = self
            .physical_attacks
            .get(&actor)
            .map(|attack| {
                bace_combat::physical::select_melee(
                    &profile,
                    attack.height,
                    attack.power,
                    attack.selected.hand == bace_gameplay_api::weapon_combat::PhysicalHand::Offhand,
                )
            })
            .transpose()
            .map_err(|_| SkillRefreshError::InvalidInput)?;
        let profile = std::sync::Arc::new(profile);
        self.physical.insert(actor, profile.clone());
        if let Some(attack) = self.physical_attacks.get_mut(&actor) {
            attack.profile = profile;
            if let Some(selected) = refreshed {
                attack.selected.skill = selected.skill;
                attack.selected.skill_level = selected.skill_level;
            }
        }
        Ok(())
    }
    pub(crate) fn configure_random(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
        epoch: u64,
    ) -> Result<(), SkillRefreshError> {
        if self.random.is_some() || !self.attacks.is_empty() || epoch == 0 {
            return Err(SkillRefreshError::InvalidInput);
        }
        self.random = Some((root, epoch));
        Ok(())
    }
    pub(crate) fn can_set_skills(&self, actor: EntityId) -> Result<(), SkillRefreshError> {
        if !self.skills.contains_key(&actor) && self.skills.len() >= self.capacity {
            return Err(SkillRefreshError::Capacity);
        }
        Ok(())
    }
    pub(crate) fn set_skills(
        &mut self,
        actor: EntityId,
        prepared: PreparedCharacterSkillInputs,
        values: &[SkillValues],
        revision: u64,
    ) -> Result<(), SkillRefreshError> {
        if !self.skills.contains_key(&actor) && self.skills.len() >= self.capacity {
            return Err(SkillRefreshError::Capacity);
        }
        let power = self.skills.get(&actor).map_or(0.5, |s| s.power);
        self.skills.insert(
            actor,
            CombatSkills {
                prepared,
                values: values
                    .iter()
                    .map(|v| {
                        (
                            v.skill,
                            CombatSkill {
                                advancement: v.advancement,
                                base: v.base,
                                current: v.current,
                            },
                        )
                    })
                    .collect(),
                revision,
                registry_revision: None,
                dirty_signature: [0; 2],
                power,
            },
        );
        Ok(())
    }
    pub(crate) fn next_random(
        &mut self,
    ) -> Result<Option<bace_random::RandomStream>, CombatRejection> {
        if self.skills.is_empty() {
            return Ok(None);
        }
        let (root, epoch) = self
            .random
            .as_ref()
            .ok_or(CombatRejection::InvalidRequest)?;
        let operation = self
            .next_attack
            .checked_add(1)
            .ok_or(CombatRejection::Capacity)?;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&epoch.to_le_bytes());
        id[8..].copy_from_slice(&operation.to_le_bytes());
        let stream = root
            .event_stream(id, bace_random::Domain::Combat)
            .map_err(|_| CombatRejection::InvalidRequest)?;
        self.next_attack = operation;
        Ok(Some(stream))
    }
    pub(crate) fn dirty_involves(&self, actor: EntityId) -> bool {
        self.dirty
            .iter()
            .any(|v| v.attacker == actor || v.target == actor)
    }
    pub(crate) fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }
    pub(crate) fn peek_dirty(&self) -> Option<DirtyFightingImpact> {
        self.dirty.front().copied()
    }
    pub(crate) fn take_dirty(&mut self) -> Option<DirtyFightingImpact> {
        self.dirty.pop_front()
    }
}
pub(super) struct ImpactInputs<'a> {
    pub attacker: Option<&'a CombatSkills>,
    pub defender: Option<&'a CombatSkills>,
    pub attacker_player: bool,
    pub defender_player: bool,
    pub defender_in_combat: bool,
    pub defender_reckless_mode: bool,
    pub angle: f32,
    pub power: f32,
    pub height: u32,
    pub base_damage: u32,
    pub power_modifier: f32,
    pub random: Option<&'a bace_random::RandomStream>,
    pub index: usize,
}
pub(super) fn damage_and_dirty(
    input: ImpactInputs<'_>,
) -> Result<(u32, DirtyFightingEffects), CombatRejection> {
    let mut damage = input.base_damage as f32 * input.power_modifier;
    let mut dirty = DirtyFightingEffects {
        spells: [0; 2],
        count: 0,
    };
    if let Some(source) = input.attacker {
        if input.attacker_player {
            damage *=
                recklessness_modifier(source.skill(50), source.attack(), input.power, true, false)
                    .map_err(|_| CombatRejection::InvalidRequest)?;
        }
        let roll = |label: &[u8]| -> Result<f32, CombatRejection> {
            let mut stream = input
                .random
                .ok_or(CombatRejection::InvalidRequest)?
                .fork(label, input.index as u64)
                .map_err(|_| CombatRejection::InvalidRequest)?;
            Ok((stream
                .next_u64()
                .map_err(|_| CombatRejection::InvalidRequest)?
                >> 40) as f32
                / 16_777_216.0)
        };
        damage *= sneak_attack_modifier(SneakAttackInput {
            sneak: source.skill(51),
            deception: source.skill(20),
            attack_skill: source.attack(),
            target_assess_person: input.defender.map_or(0, |s| s.skill(19).current),
            target_is_creature: true,
            angle_degrees: input.angle,
            roll: roll(b"sneak")?,
        })
        .map_err(|_| CombatRejection::InvalidRequest)?;
        dirty = dirty_fighting_effects(
            source.skill(52),
            source.attack(),
            match input.height {
                1 => DirtyAttackHeight::Low,
                2 => DirtyAttackHeight::Medium,
                _ => DirtyAttackHeight::High,
            },
            roll(b"dirty")?,
        )
        .map_err(|_| CombatRejection::InvalidRequest)?;
    }
    if let Some(target) = input.defender {
        if input.defender_player && input.defender_reckless_mode {
            damage *=
                recklessness_modifier(target.skill(50), target.attack(), target.power, true, false)
                    .map_err(|_| CombatRejection::InvalidRequest)?;
        }
        let rating =
            specialized_defense_rating(input.defender_player, DefenseKind::Melee, target.skill(6));
        damage *= defense_rating_modifier(rating).map_err(|_| CombatRejection::InvalidRequest)?;
        if input.defender_in_combat
            && input.angle.abs() <= 90.0
            && let Some(shield) = target.prepared.shield
        {
            damage *= shield_physical_modifier(
                target.skill(48),
                input.angle,
                input.defender_in_combat,
                shield.effective_armor,
            )
            .map_err(|_| CombatRejection::InvalidRequest)?;
        }
    }
    if !damage.is_finite() || damage < 0.0 {
        return Err(CombatRejection::InvalidRequest);
    }
    Ok((
        f64::from(damage).round_ties_even().min(u32::MAX as f64) as u32,
        dirty,
    ))
}
/// Facing comes exclusively from accepted body state. Heading zero is +Y.
pub(super) fn accepted_angle(
    world: &World,
    attacker: EntityId,
    target: EntityId,
) -> Result<f32, CombatRejection> {
    let source = world
        .body(attacker)
        .map_err(|_| CombatRejection::MissingActor)?
        .accepted();
    let target = world
        .body(target)
        .map_err(|_| CombatRejection::MissingActor)?
        .accepted();
    let delta = source.position() - target.position();
    let bearing = (-delta.x).atan2(delta.y);
    let angle = (bearing - target.heading_radians() + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    Ok(angle.to_degrees())
}
