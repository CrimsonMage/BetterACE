//! Requirements are evaluated against the sole accepted character and registry
//! owners. Cold templates provide predicates, never an asserted pass result.
use crate::Kernel;
use bace_entity::{PropertyFamily, PropertyValue};
use bace_gameplay_api::{ProgressionTarget, TraitDetails, VitalId};
use bace_inventory::{
    ActivationFailure, ActivationRequirements, ActivationValues, WieldFailure, WieldPolicy,
    WieldValues,
};
use bace_types::EntityId;
struct Values<'a> {
    kernel: &'a Kernel,
    actor: EntityId,
    skills: Vec<bace_character::SkillValues>,
    attributes: ([u32; 6], [u32; 6]),
    vitals: [(u32, u32); 3],
}
impl<'a> Values<'a> {
    fn prepare(
        kernel: &'a Kernel,
        actor: EntityId,
        candidate: Option<(&bace_magic::EnchantmentRegistry, u32)>,
    ) -> Option<Self> {
        let character = kernel.characters.get(actor)?;
        let prepared = &kernel.combat.skills.get(&actor)?.prepared;
        let registry = candidate
            .map(|c| c.0)
            .or_else(|| kernel.magic.registry(actor))?;
        let skills =
            super::super::skill_refresh::project_skills(character, prepared, Some(registry))
                .ok()?;
        let attributes =
            super::super::skill_modifiers::attributes(character, prepared, Some(registry)).ok()?;

        let inputs = kernel.vital_inputs.get(&actor)?;
        let services = kernel.characters.native_services(actor)?;
        let gear = inputs.equipped_health.iter().try_fold(0u32, |sum, (item, health)| {
            if kernel.inventory.item(*item).is_some_and(|i| matches!(i.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)) {
                sum.checked_add(*health)
            } else { Some(sum) }
        })?;
        let gear = candidate.map_or(gear, |c| c.1);
        let maximum = super::super::live_vitals::project_maxima(
            character,
            registry,
            inputs.formulas,
            gear,
            services.enlightenment,
        )
        .ok()?;
        let mut vitals = [(0, 0); 3];
        for (index, id) in [1, 3, 5].into_iter().enumerate() {
            let projection =
                character.projection(ProgressionTarget::Vital(VitalId::try_from(id).ok()?))?;
            let Some(TraitDetails::Vital { starting_value, .. }) = projection.details else {
                return None;
            };
            let bonus = if index == 0 {
                services.enlightenment.checked_mul(2)?.checked_add(gear)?
            } else {
                0
            };
            let values = bace_character::project_vital_values(bace_character::VitalValueInputs {
                formula: inputs.formulas[index],
                starting_value,
                ranks: u32::from(projection.ranks),
                current_attributes: attributes.0,
                base_bonus: bonus,
                multiplier: 1.,
                vitae: 1.,
                additive: 0.,
            })
            .ok()?;
            vitals[index] = (values.before_multipliers, maximum[index]);
        }
        kernel.world.properties(actor)?;
        Some(Self {
            kernel,
            actor,
            skills,
            attributes,
            vitals,
        })
    }
    fn mapped(&self, key: i32) -> Option<u32> {
        let mut id = u32::try_from(key).ok()?;
        match id {
            1 | 4 | 5 | 9 | 10 | 11 | 13 => {
                id = 45;
                let current = |id| {
                    self.skills
                        .iter()
                        .find(|s| s.skill == id)
                        .map(|s| s.current)
                };
                for candidate in [44, 46] {
                    if current(candidate)? > current(id)? {
                        id = candidate;
                    }
                }
            }
            2 | 3 | 8 | 12 => id = 47,
            _ => {}
        }
        Some(id)
    }
}
impl WieldValues for Values<'_> {
    fn skill(&self, key: i32) -> Option<(u32, u32, u32)> {
        let id = self.mapped(key)?;
        let skill = self.skills.iter().find(|s| s.skill == id)?;
        Some((skill.base, skill.current, skill.advancement as u32))
    }
    fn attribute(&self, key: i32) -> Option<(u32, u32)> {
        let index = usize::try_from(key.checked_sub(1)?).ok()?;
        Some((
            *self.attributes.0.get(index)?,
            *self.attributes.1.get(index)?,
        ))
    }
    fn vital(&self, key: i32) -> Option<(u32, u32)> {
        self.vitals
            .get(match key {
                1 => 0,
                3 => 1,
                5 => 2,
                _ => return None,
            })
            .copied()
    }
    fn level(&self) -> i32 {
        self.kernel
            .characters
            .native_services(self.actor)
            .map_or(1, |s| s.level.min(i32::MAX as u32) as i32)
    }
    fn int_property(&self, key: i32) -> i32 {
        match self
            .kernel
            .world
            .properties(self.actor)
            .and_then(|p| p.get(PropertyFamily::Int, key as u32))
        {
            Some(PropertyValue::Int(v)) => *v,
            _ => 0,
        }
    }
    fn bool_property(&self, key: i32) -> bool {
        matches!(
            self.kernel
                .world
                .properties(self.actor)
                .and_then(|p| p.get(PropertyFamily::Bool, key as u32)),
            Some(PropertyValue::Bool(true))
        )
    }
    fn creature_type(&self) -> i32 {
        self.int_property(2)
    }
}
impl ActivationValues for Values<'_> {
    fn heritage(&self) -> u32 {
        self.int_property(188) as u32
    }
    fn mapped_skill(&self, key: i32) -> Option<u32> {
        self.mapped(key)
    }
    fn cooldown_ready(&self, group: Option<i32>) -> bool {
        let Some(group) = group else {
            return true;
        };
        self.kernel.magic.registry(self.actor).is_some_and(|r| {
            r.entries()
                .iter()
                .find(|e| e.spell == (0x8000 | group as u32))
                .is_none_or(|e| (e.spec.duration - e.start_time.abs()) as f32 == 0.)
        })
    }
}
impl Kernel {
    /// Caller synchronizes registry time and holds the participant before using
    /// this result as part of an immutable valuable-operation proposal.
    pub fn check_inventory_item_activation(
        &self,
        actor: EntityId,
        requirements: &ActivationRequirements,
    ) -> Result<(), ActivationFailure> {
        let values = Values::prepare(self, actor, None).ok_or(ActivationFailure::MissingValue)?;
        bace_inventory::check_item_activation(requirements, &values)
    }
    pub fn check_inventory_item_activation_after(
        &self,
        actor: EntityId,
        requirements: &ActivationRequirements,
        registry: &bace_magic::EnchantmentRegistry,
        gear_health: u32,
    ) -> Result<(), ActivationFailure> {
        let values = Values::prepare(self, actor, Some((registry, gear_health)))
            .ok_or(ActivationFailure::MissingValue)?;
        bace_inventory::check_item_activation(requirements, &values)
    }
    pub fn check_inventory_wield_requirements(
        &self,
        actor: EntityId,
        mut policy: WieldPolicy,
    ) -> Result<(), WieldFailure> {
        let values = Values::prepare(self, actor, None).ok_or(WieldFailure::MissingValue)?;
        policy.actor = actor.0;
        policy.heritage = values.heritage();
        bace_inventory::check_wield_requirements(policy, &values)
    }
}
