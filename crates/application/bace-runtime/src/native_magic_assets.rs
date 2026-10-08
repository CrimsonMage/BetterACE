//! Cold native spell preparation: verified DAT behavior/gestures, accepted server
//! row, exact component plan and independently prepared projectile geometry.
use crate::magic_preparation::{
    PreparedComponentPlan, prepare_cast_gestures_from_assets, prepare_projectile_motion,
};
use bace_content::{SpellRowV1, WeenieV1};
use bace_dat::{Animation, MotionTable, SpellBase, SpellComponents};
use bace_magic::{NativeProjectileMotion, NativeSpellHeader};
use bace_physics::CollisionShape;
use bace_simulation::{PreparedMagicAssetBatch, PreparedMagicDefinition, PreparedMagicSpell};
use std::{collections::BTreeMap, sync::Arc};
pub struct NativeMagicPreparation<'a> {
    pub id: u32,
    pub base: &'a SpellBase,
    pub row: &'a SpellRowV1,
    pub components: &'a SpellComponents,
    pub component_plan: &'a PreparedComponentPlan,
    pub motion_table: &'a MotionTable,
    pub animations: &'a BTreeMap<u32, Animation>,
    pub motion_context: crate::world_admission::MotionChainRequest<'a>,
    pub projectile: Option<(&'a WeenieV1, Arc<CollisionShape>)>,
}
#[derive(Clone, Debug)]
pub struct PreparedNativeMagicSpell {
    pub definition: PreparedMagicDefinition,
    pub projectile: Option<(u32, Arc<CollisionShape>)>,
}
pub fn prepare_native_magic_spell(
    input: NativeMagicPreparation<'_>,
) -> Result<PreparedNativeMagicSpell, String> {
    let base = input.base;
    if input.id != input.row.id
        || input.id == 0
        || base.component_loss != input.component_plan.component_loss
    {
        return Err("native spell/component identity mismatch".into());
    }
    let mut prepared =
        prepare_instant_native_magic_spell(input.id, base, input.row, input.projectile)?;
    let gestures = prepare_cast_gestures_from_assets(
        base,
        input.components,
        input.motion_table,
        input.animations,
        input.motion_context,
    )
    .map_err(|e| format!("native spell motion: {e:?}"))?;
    prepared.definition.spell.gestures = gestures;
    prepared.definition.spell.components = input.component_plan.requirements.clone();
    prepared.definition.spell.component_modifiers =
        input.component_plan.destruction_modifiers.clone();
    prepared.definition.spell.component_loss = input.component_plan.component_loss;
    Ok(prepared)
}
/// Called by composition before admitting casts; all related indexes publish as
/// one simulation-owner batch. A rejected batch leaves prior definitions intact.
pub fn register_native_magic_spells(
    kernel: &mut bace_simulation::Kernel,
    spells: Vec<PreparedNativeMagicSpell>,
) -> Result<(), String> {
    if spells.len() > 16384 {
        return Err("native spell batch capacity".into());
    }
    let mut definitions = Vec::with_capacity(spells.len());
    let mut shapes = BTreeMap::new();
    for spell in spells {
        if let Some((id, shape)) = spell.projectile {
            if let Some(existing) = shapes.get(&id) {
                if !Arc::ptr_eq(existing, &shape) {
                    return Err("conflicting native projectile preparation".into());
                }
            } else {
                shapes.insert(id, shape);
            }
        }
        definitions.push(spell.definition);
    }
    kernel
        .register_magic_asset_batch(PreparedMagicAssetBatch {
            definitions,
            projectile_shapes: shapes.into_iter().collect(),
        })
        .map_err(|e| format!("native spell registration: {e:?}"))
}

/// Server instant definitions have no account formula or animation program.
/// Callers must use an explicit trusted server origin; player casts still require
/// their separately admitted actor-bound program and resource requirements.
pub fn prepare_instant_native_magic_spell(
    id: u32,
    base: &SpellBase,
    row: &SpellRowV1,
    projectile: Option<(&WeenieV1, Arc<CollisionShape>)>,
) -> Result<PreparedNativeMagicSpell, String> {
    if id == 0 || id != row.id {
        return Err("native server spell identity".into());
    }
    let motion = if let Some((template, shape)) = projectile.as_ref() {
        if row.wcid != Some(template.weenie_id) {
            return Err("native spell projectile template mismatch".into());
        }
        let motion = prepare_projectile_motion(template, shape)
            .map_err(|e| format!("native projectile preparation: {e:?}"))?;
        Some(NativeProjectileMotion {
            template: template.weenie_id,
            radius: motion.nominal_radius,
            speed: motion.speed,
            gravity: motion.gravity,
        })
    } else {
        None
    };
    let definition = bace_magic::prepare_native_spell(
        NativeSpellHeader {
            id,
            school: base.school,
            category: base.category,
            flags: base.flags,
            base_mana: base.base_mana,
            power: base.power,
            range_constant: base.range_constant,
            range_modifier: base.range_modifier,
            meta_type: base.meta_type,
            enchantment: base.enchantment,
            portal_lifetime: base.portal_lifetime,
        },
        row,
        motion,
    )
    .map_err(|e| format!("native spell {}: {e:?}", id))?;

    Ok(PreparedNativeMagicSpell {
        definition: PreparedMagicDefinition {
            spell: PreparedMagicSpell {
                spell: definition.spell,
                gestures: Vec::new(),
                components: Vec::new(),
                component_modifiers: Vec::new(),
                component_loss: base.component_loss,
                fast_resistable_pk_spell: definition.fast_resistable_pk_spell,
            },
            metadata: definition.metadata,
            formula_level: bace_magic::spell_formula_level(&base.formula),
            category: base.category,
            flags: base.flags,
            target_mask: base.non_component_target_type,
        },
        projectile: projectile.map(|(template, shape)| (template.weenie_id, shape)),
    })
}
/// The exact currently executable equipment references accompany their native
/// definitions. Admission checks this dependency set before profile adoption.
#[derive(Clone, Debug)]
pub struct PreparedProcMagic {
    pub required_spells: Vec<u32>,
    pub batch: PreparedMagicAssetBatch,
}
