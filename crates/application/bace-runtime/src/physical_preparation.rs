//! Thin accepted-asset adapter. Combat owns property/default/equipment semantics;
//! DAT preparation supplies immutable collision and execution-motion inputs.
use bace_combat::preparation::{PhysicalPreparation, prepare_physical};
use bace_dat::{Animation, CollisionSetup, CombatManeuverTable, MotionTable};
use bace_gameplay_api::weapon_combat::PhysicalCombatProfile;
use std::collections::BTreeMap;
pub struct PhysicalAssets<'a> {
    pub setup: &'a CollisionSetup,
    pub motions: &'a MotionTable,
    pub maneuvers: &'a CombatManeuverTable,
    pub animations: &'a BTreeMap<u32, Animation>,
    pub scale: f32,
}
pub fn prepare_physical_from_assets(
    mut input: PhysicalPreparation<'_>,
    assets: PhysicalAssets<'_>,
) -> Result<PhysicalCombatProfile, String> {
    // These references come from the complete accepted/inherited instance, not
    // a packet or a guessed fallback model. Missing preparation remains explicit.
    for (property, actual) in [
        (1, assets.setup.id),
        (2, assets.motions.id),
        (4, assets.maneuvers.id),
    ] {
        if input
            .weenie
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == property)
            .map(|p| p.value)
            != Some(actual)
        {
            return Err(format!(
                "physical asset reference {property} missing or mismatched"
            ));
        }
    }
    let _shape = crate::world_admission::prepare_collision_shape(assets.setup, assets.scale)?;
    let height = assets.setup.height * assets.scale;
    if !height.is_finite() || height <= 0.0 {
        return Err("invalid admitted creature height".into());
    }
    input.height = height;
    input.maneuvers = crate::world_admission::prepare_combat_maneuvers(
        assets.maneuvers,
        assets.motions,
        assets.animations,
    )?;
    prepare_physical(input).map_err(|e| format!("physical preparation: {e:?}"))
}
