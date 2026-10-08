//! Exact durable NPC registry recovery is prepared before any world publication.
//! Initial equipment spell scheduling must be suppressed by the cold restore owner.
use super::*;
pub(super) struct RestoredCombat {
    pub registries: Vec<(EntityId, bace_magic::EnchantmentRegistry)>,
    pub physical: Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    pub values: Vec<bace_character::SkillValues>,
    pub attributes: [u32; 6],
}
impl Kernel {
    pub(super) fn prepare_npc_registry_restore(
        &self,
        actor: EntityId,
        input: &PreparedNpcCombatAssets,
    ) -> Result<Option<RestoredCombat>, E> {
        let Some(saved) = &input.restored_registries else {
            return Ok(None);
        };
        if self.npc_combat_assets.contains_key(&actor) || self.combat.active(actor) {
            return Err(E::Busy);
        }
        let expected: std::collections::BTreeSet<_> = std::iter::once(actor)
            .chain(input.registry_items.iter().copied())
            .collect();
        if expected.len() != saved.len() {
            return Err(E::InvalidInput);
        }
        let mut registries = std::collections::BTreeMap::new();
        for entry in saved {
            if !expected.contains(&entry.entity) || registries.contains_key(&entry.entity) {
                return Err(E::InvalidInput);
            }
            let registry = bace_magic::EnchantmentRegistry::restore(
                512,
                entry.revision,
                entry.entries.clone(),
            )
            .map_err(|_| E::InvalidInput)?;
            registries.insert(entry.entity, registry);
        }
        self.magic
            .preflight_empty_registry_restore(expected.iter().copied())
            .map_err(magic_error)?;
        let (values, attributes) = npc_values(input, registries.get(&actor))?;
        let raw = input.bind_physical_source(actor)?;
        let mut qualities = raw.qualities.clone();
        for q in &mut qualities {
            let registry = registries.get(&EntityId(q.entity)).ok_or(E::MissingActor)?;
            use bace_combat::preparation::PhysicalQualityFamily as F;
            let (details, value) = match q.family {
                F::Int => {
                    bace_magic::enchant_physical_quality(registry, 4, q.stat, q.details.raw, true)
                }
                F::Float => {
                    bace_magic::enchant_physical_quality(registry, 8, q.stat, q.details.raw, false)
                }
                F::BodyArmor => bace_magic::enchant_body_quality(registry, q.stat, q.details.raw)
                    .map(|d| (d, q.details.raw)),
            }
            .map_err(|_| E::InvalidInput)?;
            q.details = details;
            q.value = value;
        }
        let mut current = (*input.physical).clone();
        current.skills = values
            .iter()
            .map(|s| {
                (
                    s.skill,
                    PhysicalSkill {
                        advancement: s.advancement as u32,
                        current: s.current,
                    },
                )
            })
            .collect();
        current.skills.sort_by_key(|v| v.0);
        current.strength = attributes[0];
        current.quickness = attributes[2];
        current.coordination = attributes[3];
        let physical =
            bace_combat::preparation::refresh_physical_from_source(&raw, &current, &qualities)
                .map_err(|_| E::InvalidInput)?;
        Ok(Some(RestoredCombat {
            registries: registries.into_iter().collect(),
            physical: Arc::new(physical),
            values,
            attributes,
        }))
    }
}
