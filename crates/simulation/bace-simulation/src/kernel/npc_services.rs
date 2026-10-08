//! Borrowed live owner queries; no cached player/inventory copies.
use super::*;
use bace_gameplay_api::{
    NpcContext, NpcFailure, NpcPropertyFamily, NpcQuery, NpcQueryValue, NpcSubject, NpcValue,
};
pub(super) struct NativeReadServices<'a> {
    pub inventory: &'a crate::inventory::Inventory,
    pub combat: &'a crate::combat::Combat,
    pub magic: &'a crate::magic::Magic,
    pub portals: &'a super::portals::PortalServices,
    pub housing: &'a crate::housing::Housing,
}
impl crate::npc::NpcServiceView for NativeReadServices<'_> {
    fn experience_admission_supported(&self, actor: EntityId) -> Result<(), NpcFailure> {
        if self.inventory.container(actor).is_none() {
            return Err(NpcFailure::MissingContent);
        }
        if self.inventory.equipped_items(actor).next().is_some() {
            return Err(NpcFailure::Unsupported);
        }
        self.experience_recipient_supported(actor)
    }
    fn experience_recipient_supported(&self, actor: EntityId) -> Result<(), NpcFailure> {
        let registry = self
            .magic
            .registry(actor)
            .ok_or(NpcFailure::MissingContent)?;
        if registry.entries().iter().any(|e| e.spell == 666) {
            return Err(NpcFailure::Unsupported);
        }
        Ok(())
    }

    fn reserved(&self, actor: EntityId) -> bool {
        self.inventory.reserved(actor)
            || self.portals.reserved(actor)
            || self.housing.reserved(actor)
    }
    fn experience_modifier(&self, actor: EntityId) -> Result<f32, NpcFailure> {
        let registry = self
            .magic
            .registry(actor)
            .ok_or(NpcFailure::MissingContent)?;
        let bonus = registry
            .entries()
            .iter()
            .rev()
            .filter(|e| e.spec.category == 615)
            .max_by_key(|e| e.spec.power)
            .map_or(0.0, |e| {
                if e.spec.stat_type & 0x4000 != 0 {
                    e.spec.value - 1.0
                } else {
                    e.spec.value
                }
            });
        let value = 1.0 + bonus;
        if !value.is_finite() || value < 0.0 {
            return Err(NpcFailure::InvalidInput);
        }
        Ok(value)
    }

    fn query(
        &self,
        context: NpcContext,
        query: &NpcQuery,
        raw: Option<u32>,
    ) -> Result<NpcQueryValue, NpcFailure> {
        let value = match query {
            NpcQuery::ItemCount { template } => NpcValue::Int64(
                i64::try_from(
                    self.inventory
                        .count(context.target.ok_or(NpcFailure::MissingActor)?, *template),
                )
                .map_err(|_| NpcFailure::InvalidInput)?,
            ),
            NpcQuery::PackSpace { containers } => NpcValue::Int64(i64::from(
                self.inventory
                    .free_slots(context.target.ok_or(NpcFailure::MissingActor)?, *containers)
                    .map_err(|_| NpcFailure::MissingContent)?,
            )),
            NpcQuery::Property {
                subject,
                family,
                stat,
            } => {
                let actor = match subject {
                    NpcSubject::Source => context.source,
                    NpcSubject::Target => context.target.ok_or(NpcFailure::MissingActor)?,
                    _ => return Err(NpcFailure::Unsupported),
                };
                let skills = self
                    .combat
                    .skills
                    .get(&actor)
                    .ok_or(NpcFailure::Unsupported)?;
                if *family == NpcPropertyFamily::Attribute {
                    let modifier = skills
                        .prepared
                        .attribute_modifiers
                        .get(stat.checked_sub(1).ok_or(NpcFailure::InvalidInput)? as usize)
                        .ok_or(NpcFailure::InvalidInput)?;
                    let base = raw.ok_or(NpcFailure::MissingContent)?;
                    let current = (base as f32 * modifier.multiplier + modifier.additive as f32)
                        .round()
                        .max(if base >= 10 { 10.0 } else { 1.0 });
                    if !current.is_finite() || current > i32::MAX as f32 {
                        return Err(NpcFailure::InvalidInput);
                    }
                    NpcValue::Unsigned(current as u32)
                } else {
                    let Some(skill) = skills.values.get(stat) else {
                        return Ok(NpcQueryValue::Absent);
                    };
                    match family {
                        NpcPropertyFamily::Skill => NpcValue::Unsigned(skill.current),
                        NpcPropertyFamily::RawSkill => NpcValue::Unsigned(skill.base),
                        NpcPropertyFamily::SkillAdvancement => {
                            NpcValue::Unsigned(skill.advancement as u32)
                        }
                        _ => return Err(NpcFailure::Unsupported),
                    }
                }
            }
            _ => return Err(NpcFailure::Unsupported),
        };
        Ok(NpcQueryValue::Value(value))
    }
}
impl Kernel {
    pub fn step_native_npcs(&mut self) -> Vec<(EntityId, bace_emotes::NativeError)> {
        let services = NativeReadServices {
            inventory: &self.inventory,
            combat: &self.combat,
            magic: &self.magic,
            portals: &self.portals,
            housing: &self.housing,
        };
        self.npcs.step(
            &mut self.world,
            &mut self.characters,
            &mut self.fellowships,
            self.tick,
            &services,
        )
    }
}
