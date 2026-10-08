//! Immutable, entity-independent NPC template combat inputs. The simulation binds
//! actor-quality keys only after a real instance identity has been reserved.
use bace_combat::preparation::{
    PhysicalEquipmentSource, PhysicalQualityFamily, PhysicalQualityProjection,
    PreparedPhysicalRefreshSource,
};
use bace_gameplay_api::{
    SkillAdvancement,
    weapon_combat::{PhysicalCombatProfile, PhysicalQuality},
};
use bace_types::EntityId;
use std::sync::Arc;
#[derive(Clone, Debug)]
pub struct PreparedNpcQuality {
    pub family: PhysicalQualityFamily,
    pub stat: u32,
    pub part: Option<u32>,
    pub details: PhysicalQuality,
    pub value: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct PreparedNpcSkill {
    pub skill: u32,
    pub advancement: SkillAdvancement,
    pub ranks: u16,
    pub initial_level: u32,
}
#[derive(Clone, Debug)]
pub struct PreparedNpcRegistryRestore {
    pub entity: EntityId,
    pub revision: u64,
    pub entries: Vec<bace_magic::EnchantmentEntry>,
}
#[derive(Clone, Debug)]
pub struct PreparedNpcCombatAssets {
    pub caster: crate::MagicCaster,
    pub server_magic: crate::PreparedMagicAssetBatch,
    pub registry_items: Vec<EntityId>,
    pub restored_registries: Option<Vec<PreparedNpcRegistryRestore>>,
    pub damage: bace_magic::MagicDamageProfile,
    pub source: Arc<bace_content::WeenieV1>,
    pub properties: bace_entity::EntityProperties,
    pub physical: Arc<PhysicalCombatProfile>,
    pub equipment: Vec<PhysicalEquipmentSource>,
    pub actor_qualities: Vec<PreparedNpcQuality>,
    pub equipment_qualities: Vec<PhysicalQualityProjection>,
    pub skill_inputs: crate::PreparedCharacterSkillInputs,
    pub skills: Vec<PreparedNpcSkill>,
    pub base_attributes: [u32; 6],
}
impl PreparedNpcCombatAssets {
    pub fn bind_physical_source(
        &self,
        actor: EntityId,
    ) -> Result<Arc<PreparedPhysicalRefreshSource>, crate::SkillRefreshError> {
        if actor.0 == 0
            || actor.0 == u32::MAX
            || self.equipment.iter().any(|e| e.entity == actor.0)
            || self.actor_qualities.len() + self.equipment_qualities.len() > 32768
        {
            return Err(crate::SkillRefreshError::InvalidInput);
        }
        let qualities = self
            .actor_qualities
            .iter()
            .map(|q| PhysicalQualityProjection {
                entity: actor.0,
                family: q.family,
                stat: q.stat,
                part: q.part,
                details: q.details,
                value: q.value,
            })
            .chain(self.equipment_qualities.iter().copied())
            .collect();
        Ok(Arc::new(PreparedPhysicalRefreshSource {
            actor: actor.0,
            weenie: self.source.clone(),
            equipment: self.equipment.clone(),
            qualities,
            profile: self.physical.clone(),
        }))
    }
}
/// Immutable native definitions retain provenance; the world owns all live
/// scalar qualities. This is a cold/changed-source projection, not a save or actor.
pub fn overlay_npc_source_qualities(
    source: &bace_content::WeenieV1,
    qualities: &bace_entity::EntityProperties,
) -> Result<bace_content::WeenieV1, String> {
    use bace_entity::{PropertyFamily as F, PropertyValue as V};
    if qualities
        .retained_bytes()
        .is_none_or(|n| n > 2 * 1024 * 1024)
    {
        return Err("NPC scalar quality byte capacity".into());
    }
    let mut value = source.clone();
    let p = &mut value.properties;
    p.bools.clear();
    p.ints.clear();
    p.int64s.clear();
    p.floats.clear();
    p.strings.clear();
    for (family, id, field) in qualities.snapshot().1 {
        match (family, field) {
            (F::Bool, V::Bool(value)) => p.bools.push(bace_content::Property { id, value }),
            (F::Int, V::Int(value)) => p.ints.push(bace_content::Property { id, value }),
            (F::Int64, V::Int64(value)) => p.int64s.push(bace_content::Property { id, value }),
            (F::Float, V::Float(value)) => p.floats.push(bace_content::Property { id, value }),
            (F::String, V::String(value)) => p.strings.push(bace_content::Property { id, value }),
            _ => {}
        }
    }
    value
        .validate(Default::default())
        .map_err(|e| e.to_string())?;
    Ok(value)
}
