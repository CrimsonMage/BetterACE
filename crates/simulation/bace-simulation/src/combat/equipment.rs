//! Canonical inventory roster fencing at physical observation points.
use super::*;
use crate::inventory::Inventory;
use bace_inventory::ItemPlace;
impl Combat {
    pub(crate) fn proposed_equipment_current(
        &self,
        profile: &bace_gameplay_api::weapon_combat::PhysicalCombatProfile,
        actor: EntityId,
        inventory: &Inventory,
    ) -> bool {
        equipment_current(profile, actor, inventory)
    }
    pub(crate) fn validate_physical_equipment(
        &self,
        actor: EntityId,
        inventory: &Inventory,
    ) -> Result<(), CombatRejection> {
        let Some(profile) = self.physical.get(&actor) else {
            return Ok(());
        };
        if equipment_current(profile, actor, inventory) {
            Ok(())
        } else {
            Err(CombatRejection::MissingCombatProfile)
        }
    }
    pub(crate) fn apply_with_equipment(
        &mut self,
        world: &mut World,
        actor: EntityId,
        request: CombatRequest,
        now: f64,
        inventory: &Inventory,
    ) -> Result<CombatChange, CombatRejection> {
        if matches!(
            request,
            CombatRequest::TargetedMelee { .. } | CombatRequest::TargetedMissile { .. }
        ) {
            self.validate_physical_equipment(actor, inventory)?;
        }
        self.apply(world, actor, request, now)
    }
    pub(crate) fn step_with_equipment(
        &mut self,
        world: &mut World,
        now: f64,
        inventory: &Inventory,
        characters: &crate::characters::Characters,
    ) {
        self.step_internal_observed(world, now, Some(inventory), Some(characters));
    }
}
pub(super) fn equipment_current(
    profile: &bace_gameplay_api::weapon_combat::PhysicalCombatProfile,
    actor: EntityId,
    inventory: &Inventory,
) -> bool {
    let mut actual = inventory.equipped_items(actor);
    for stamp in &profile.equipment {
        let Some(item) = actual.next() else {
            return false;
        };
        if item.id.0 != stamp.entity
            || item.revision != stamp.revision
            || !matches!(item.place,ItemPlace::Contained{container,equipped,..}if container==actor&&equipped==stamp.location)
        {
            return false;
        }
    }
    actual.next().is_none()
}
