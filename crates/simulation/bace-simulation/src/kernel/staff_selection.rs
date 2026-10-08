//! Source Player.HandleActionQueryHealth/QueryItemMana. Query state is ephemeral.
use super::*;
use bace_gameplay_api::{
    selection::{PreparedItemManaQuery, TargetQueryEvent, TargetQueryKind, TargetQueryResponse},
    staff::{StaffError as E, StaffEvent},
};
impl Kernel {
    pub fn query_staff_target(
        &mut self,
        context: ActionContext,
        kind: TargetQueryKind,
        target: EntityId,
        mana: Option<PreparedItemManaQuery>,
    ) -> Result<(), E> {
        if !self.staff.room(2) {
            return Err(E::Capacity);
        }
        if !self.staff.selections.contains_key(&context.actor) {
            return Err(E::NotBound);
        }
        let response = match kind {
            TargetQueryKind::Health => {
                if mana.is_some() {
                    return Err(E::Invalid);
                }
                if target.0 == 0
                    || !self.target_in_landblock_scope(context.actor, target)?
                    || self.world.combatant(target).is_none()
                {
                    None
                } else {
                    let pool = self
                        .world
                        .vital(target, bace_entity::EntityVital::Health)
                        .map_err(|_| E::MissingTarget)?;
                    Some(TargetQueryResponse::Health {
                        target,
                        fraction: pool.current as f32 / pool.maximum as f32,
                    })
                }
            }
            TargetQueryKind::ItemMana => {
                if target.0 == 0 || !self.inventory.owned(context.actor, target) {
                    if mana.is_some() {
                        return Err(E::Stale);
                    }
                    None
                } else {
                    let prepared = mana.ok_or(E::Invalid)?;
                    if self
                        .inventory
                        .item(target)
                        .is_none_or(|item| item.revision != prepared.revision)
                    {
                        return Err(E::Stale);
                    }
                    if let Some(item) = self
                        .equipment_mana
                        .players
                        .get(&context.actor)
                        .and_then(|p| p.items.get(&target))
                        && (item.current != prepared.current || item.maximum != prepared.maximum)
                    {
                        return Err(E::Stale);
                    }
                    let (fraction, success) = match (prepared.current, prepared.maximum) {
                        (Some(current), Some(maximum)) => (current as f32 / maximum as f32, 1),
                        _ => (0., 0),
                    };
                    Some(TargetQueryResponse::ItemMana {
                        target,
                        fraction,
                        success,
                    })
                }
            }
        };
        if kind == TargetQueryKind::Health {
            self.world
                .validate_health_subscription(context.actor, response.map(|_| target))
                .map_err(|_| E::Capacity)?;
        }
        self.authorize_staff(context, 0, false)?;
        if kind == TargetQueryKind::Health {
            self.world
                .set_health_subscription(context, response.map(|_| target))
                .expect("subscription preflight");
        }
        let selection = self
            .staff
            .selections
            .get_mut(&context.actor)
            .expect("query preflight");
        match kind {
            TargetQueryKind::Health => selection.health = response.map(|_| target),
            TargetQueryKind::ItemMana => selection.mana = (target.0 != 0).then_some(target),
        }
        self.staff.push(StaffEvent::TargetQuery(TargetQueryEvent {
            context,
            response,
        }));
        Ok(())
    }
    pub(super) fn drain_health_observations(&mut self) {
        for _ in 0..4096 {
            if !self.staff.room(1) {
                break;
            }
            let Some(event) = self.world.take_health_observation() else {
                break;
            };
            self.staff.push(StaffEvent::TargetQuery(TargetQueryEvent {
                context: event.context,
                response: Some(TargetQueryResponse::Health {
                    target: event.target,
                    fraction: event.current as f32 / event.maximum as f32,
                }),
            }));
        }
    }
    pub(super) fn target_in_landblock_scope(
        &self,
        actor: EntityId,
        target: EntityId,
    ) -> Result<bool, E> {
        let source = (self
            .world
            .actor_state(actor)
            .map_err(|_| E::MissingTarget)?
            .0
            .0
            >> 16) as u16;
        let Ok((cell, _)) = self.world.actor_state(target) else {
            return Ok(false);
        };
        let destination = (cell.0 >> 16) as u16;
        if source == destination {
            return Ok(true);
        }
        // ACE Landblock.GetObject searches the loaded adjacent outdoor blocks.
        let Some(region) = self.region_residency.state(source) else {
            return Err(E::MissingGeometry);
        };
        let Some(other) = self.region_residency.state(destination) else {
            return Ok(false);
        };
        Ok(!region.dungeon
            && !other.dungeon
            && i32::from(source >> 8).abs_diff(i32::from(destination >> 8)) <= 1
            && i32::from(source & 255).abs_diff(i32::from(destination & 255)) <= 1)
    }
}

#[cfg(test)]
mod tests;
