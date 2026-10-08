//! Pure preparation from verified immutable DAT tables and authoritative account /
//! inventory projections. No database, clocks or simulation-owned state access.
use bace_dat::{DualDidMapper, SpellBase, SpellComponents};
use bace_magic::MagicSchool;
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedComponentPlan {
    pub component_ids: Vec<u32>,
    pub requirements: Vec<(u32, u32)>,
    pub destruction_modifiers: Vec<(u32, f32)>,
    pub component_loss: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicPreparationError {
    MissingMotion,
    InvalidSchool,
    InvalidFormula,
    MissingComponent,
    MissingComponentMapping,
    InvalidComponent,
    Capacity,
}
/// ACE HasFoci inspects top-level inventory and infused augmentation, not arbitrary
/// nearby objects or an asserted client flag. Account text is pre-encoded CP1252.
pub fn prepare_component_plan(
    base: &SpellBase,
    components: &SpellComponents,
    mapper: &DualDidMapper,
    account_cp1252: &[u8],
    infused: [bool; 5],
    top_level_templates: &[u32],
) -> Result<PreparedComponentPlan, MagicPreparationError> {
    if top_level_templates.len() > 4096 {
        return Err(MagicPreparationError::Capacity);
    }
    let (school, foci) = match base.school {
        1 => (MagicSchool::War, 15271),
        2 => (MagicSchool::Life, 15270),
        3 => (MagicSchool::Item, 15269),
        4 => (MagicSchool::Creature, 15268),
        5 => (MagicSchool::Void, 43173),
        _ => return Err(MagicPreparationError::InvalidSchool),
    };
    let use_foci = infused[school as usize - 1] || top_level_templates.contains(&foci);
    let formula = if use_foci {
        bace_magic::foci_formula(&base.formula)
            .map_err(|_| MagicPreparationError::InvalidFormula)?
    } else {
        base.formula_for_account(account_cp1252)
            .map_err(|_| MagicPreparationError::InvalidFormula)?
    };
    if formula.len() > 12 || !base.component_loss.is_finite() || base.component_loss < 0.0 {
        return Err(MagicPreparationError::InvalidFormula);
    }
    let mut requirements = Vec::with_capacity(formula.len());
    let mut modifiers = Vec::with_capacity(formula.len());
    for id in &formula {
        let component = components
            .components
            .get(id)
            .ok_or(MagicPreparationError::MissingComponent)?;
        let template = *mapper
            .client_ids
            .get(id)
            .filter(|id| **id != 0)
            .ok_or(MagicPreparationError::MissingComponentMapping)?;
        if !component.modifier.is_finite() || component.modifier < 0.0 {
            return Err(MagicPreparationError::InvalidComponent);
        }
        requirements.push((template, 1));
        modifiers.push((template, component.modifier));
    }
    Ok(PreparedComponentPlan {
        component_ids: formula,
        requirements,
        destruction_modifiers: modifiers,
        component_loss: base.component_loss,
    })
}
/// GDLE AddMotionsForSpell consumes the base decoded formula, independently of
/// foci/account component consumption. The resolver must use verified MotionTable
/// links and animation clips; no fallback duration substitutes for missing data.
pub fn prepare_cast_gestures(
    base: &SpellBase,
    components: &SpellComponents,
    mut resolve: impl FnMut(u32, f32) -> Option<std::sync::Arc<bace_motion::PreparedMotionChain>>,
) -> Result<Vec<bace_simulation::PreparedCastGesture>, MagicPreparationError> {
    if base.formula.len() > 8 {
        return Err(MagicPreparationError::InvalidFormula);
    }
    let mut gestures = Vec::new();
    for id in base.formula.iter().take_while(|id| **id != 0) {
        let component = components
            .components
            .get(id)
            .ok_or(MagicPreparationError::MissingComponent)?;
        if component.category == 0 && base.flags & 0x4000 != 0 {
            continue;
        }
        let mut motion = component.gesture;
        if motion == 0 || motion & 0xffff == 0 {
            continue;
        }
        if motion == 0x1000012f {
            motion = 0x13000132;
        }
        if !component.time.is_finite() || !(0.0..=8.0).contains(&component.time) {
            return Err(MagicPreparationError::InvalidComponent);
        }
        let chain = resolve(motion, 2.0).ok_or(MagicPreparationError::MissingMotion)?;
        if chain.motion != motion || chain.speed != 2.0 {
            return Err(MagicPreparationError::MissingMotion);
        }
        gestures.push(bace_simulation::PreparedCastGesture {
            gesture: bace_magic::CastGesture {
                motion,
                minimum_seconds: f64::from(component.time),
            },
            duration_seconds: 0.0,
            motion_chain: Some(chain),
        });
    }
    Ok(gestures)
}
/// Production composition through the verified DAT MotionTable/Animation factory.
/// `context` is the server's admitted style/locomotion/scale projection; only its
/// action and action speed are replaced for each source-selected cast gesture.
pub fn prepare_cast_gestures_from_assets(
    base: &SpellBase,
    components: &SpellComponents,
    table: &bace_dat::MotionTable,
    animations: &std::collections::BTreeMap<u32, bace_dat::Animation>,
    context: crate::world_admission::MotionChainRequest<'_>,
) -> Result<Vec<bace_simulation::PreparedCastGesture>, MagicPreparationError> {
    let mut state = context;
    prepare_cast_gestures(base, components, |action, action_speed| {
        let chain = crate::world_admission::prepare_motion_chain(
            table,
            animations,
            crate::world_admission::MotionChainRequest {
                action,
                action_speed,
                ..state
            },
        )
        .ok()?;
        if action & 0x10000000 == 0 && action & 0x40000000 != 0 {
            state.current_motion = action;
            state.current_speed = action_speed;
        }
        Some(chain)
    })
}

/// GDLE LaunchProjectileSpell reads the prepared projectile weenie's Float26
/// (MaximumVelocity, default5) and Bool14 (GravityStatus). FastCast does not
/// multiply velocity; its separate casting controls must not alter this profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedProjectileMotion {
    pub speed: f32,
    pub gravity: f32,
    pub nominal_radius: f32,
}
pub fn prepare_projectile_motion(
    template: &bace_content::WeenieV1,
    shape: &bace_physics::CollisionShape,
) -> Result<PreparedProjectileMotion, MagicPreparationError> {
    let speed = template
        .properties
        .floats
        .iter()
        .find(|p| p.id == 26)
        .map_or(5.0, |p| p.value) as f32;
    let gravity = if template
        .properties
        .bools
        .iter()
        .any(|p| p.id == 14 && p.value)
    {
        9.8
    } else {
        0.0
    };
    let nominal_radius = shape
        .nominal_radius()
        .ok_or(MagicPreparationError::MissingMotion)?;
    if !speed.is_finite()
        || speed <= 0.0
        || speed > 1000.0
        || shape.spheres().len() != 1
        || shape.spheres()[0].center != bace_geometry::Vec3::ZERO
        || shape.obstacles().any(|(_, height)| height.is_some())
    {
        return Err(MagicPreparationError::InvalidComponent);
    }
    Ok(PreparedProjectileMotion {
        speed,
        gravity,
        nominal_radius,
    })
}
