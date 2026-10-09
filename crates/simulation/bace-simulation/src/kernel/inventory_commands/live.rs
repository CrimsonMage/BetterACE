//! Accepted movement/animation gates for physical inventory actions. Inspection
//! never consumes an authenticated sequence or reserves a valuable object.
mod movement;
#[cfg(test)]
#[path = "live/tests.rs"]
mod tests;
use super::*;
use crate::{InventoryInspection, InventoryLivePrepared};
use bace_motion::{MotionDomain, MotionExecutionEvent, MotionToken, TurnControl};
use std::collections::BTreeSet;
pub(super) struct Live {
    pub prepared: Box<InventoryLivePrepared>,
    pub control: Option<TurnControl>,
    pub stage: Stage,
    pub deadline: u64,
}
#[derive(Clone, Copy)]
pub(super) enum Stage {
    Approach,
    Animation(MotionToken),
    Callback,
    Returning(MotionToken),
    Failed(E),
}
impl Kernel {
    pub(in crate::kernel) fn inspect_inventory_live(
        &self,
        context: ActionContext,
        request: InventoryRequest,
    ) -> Result<InventoryInspection, E> {
        self.characters
            .can_take_complete(CharacterBinding {
                actor: context.actor,
                account: context.account,
                session: context.session,
            })
            .map_err(|_| E::OwnershipMismatch)?;
        self.inventory_live_available(context.actor)?;
        let body = self.world.body(context.actor).map_err(|_| E::NotBound)?;
        let (source, destination) = ids(request);
        if self.world.corpse(source).is_some() {
            return Err(E::AccessDenied);
        }
        let source_item = self.inventory.item(source).ok_or(E::MissingItem)?;
        if source.0 < 0x80000000 || source_item.place == ItemPlace::Removed {
            return Err(E::AccessDenied);
        }
        let mut rows = BTreeSet::new();
        let source_root = self.inventory_live_root(context.actor, source, &mut rows)?;
        let destination_root = match destination {
            Some(id) => self.inventory_live_root(context.actor, id, &mut rows)?,
            None => None,
        };
        if source_root.is_some() && destination_root.is_some() {
            return Err(E::AccessDenied);
        }
        let target = source_root.or(destination_root);
        // Corpse admission is an explicit Open transaction. Even the victim
        // and killer must own the live viewer before moving its contents.
        if let Some(corpse) = target.filter(|id| self.world.corpse(*id).is_some())
            && self
                .player_deaths
                .corpse_access
                .get(&corpse)
                .is_none_or(|access| access.viewer != Some(context.actor))
        {
            return Err(E::AccessDenied);
        }
        if matches!(
            request,
            InventoryRequest::Drop { .. } | InventoryRequest::SplitToWorld { .. }
        ) && source_root.is_some()
        {
            return Err(E::OwnershipMismatch);
        }
        if matches!(
            request,
            InventoryRequest::Equip { .. } | InventoryRequest::SplitToWield { .. }
        ) {
            return Err(E::InvalidEquip);
        }
        if let Some(root) = source_root {
            for id in self.inventory.world_tree(root)? {
                rows.insert(id);
            }
        }
        if rows.len() > 1024 {
            return Err(E::Capacity);
        }
        Ok(InventoryInspection {
            context,
            request,
            epoch: body.accepted().epoch(),
            motion: self.world.source_motion_state(context.actor),
            target,
            target_corpse: target.is_some_and(|t| self.world.corpse(t).is_some()),
            rows: rows
                .into_iter()
                .map(|id| self.inventory.item(id).cloned().ok_or(E::MissingItem))
                .collect::<Result<_, _>>()?,
        })
    }
    fn inventory_live_root(
        &self,
        actor: EntityId,
        mut id: EntityId,
        rows: &mut BTreeSet<EntityId>,
    ) -> Result<Option<EntityId>, E> {
        for _ in 0..1024 {
            if id == actor {
                return Ok(None);
            }
            if !rows.insert(id) && self.inventory.item(id).is_none() {
                return Err(E::InvalidState);
            }
            let item = self.inventory.item(id).ok_or(E::MissingItem)?;
            match item.place {
                ItemPlace::Contained {
                    container,
                    equipped: 0,
                    ..
                } => id = container,
                ItemPlace::World => {
                    self.world.body(id).map_err(|_| E::MissingGeometry)?;
                    return Ok(Some(id));
                }
                _ => return Err(E::InvalidEquip),
            }
        }
        Err(E::InvalidState)
    }
    fn inventory_live_available(&self, actor: EntityId) -> Result<(), E> {
        if self
            .inventory_commands
            .live
            .values()
            .any(|p| p.prepared.evidence.context.actor == actor)
            || self.magic.busy(actor)
            || self.recall_busy(actor)
            || self.portals.reserved(actor)
            || self.world.motion_busy(actor)
            || self.world.is_in_portal_transit(actor)
            || self.characters.reserved(actor)
            || self.npcs.reserved(actor)
            || self.inventory.reserved(actor)
            || self.world.combatant(actor).is_some_and(|c| c.health() == 0)
        {
            return Err(E::Busy);
        }
        Ok(())
    }
    pub(in crate::kernel) fn start_inventory_live(
        &mut self,
        correlation: u64,
        prepared: Box<InventoryLivePrepared>,
    ) -> Result<InventoryDecision, E> {
        if self.inventory_commands.capacity < 2
            || self.inventory_commands.live.len() >= self.inventory_commands.capacity.min(64)
            || self.inventory_commands.live.contains_key(&correlation)
        {
            return Err(E::Capacity);
        }
        let current =
            self.inspect_inventory_live(prepared.evidence.context, prepared.evidence.request)?;
        if current.epoch != prepared.evidence.epoch
            || current.target != prepared.evidence.target
            || current.motion != prepared.evidence.motion
            || current.rows != prepared.evidence.rows
        {
            return Err(E::InvalidState);
        }
        if prepared.motions.is_empty()
            || prepared.request.drop.is_some()
            || prepared.evidence.context != prepared.request.context
            || prepared.evidence.request != prepared.request.request
        {
            return Err(E::InvalidState);
        }
        if matches!(
            current.request,
            InventoryRequest::Drop { .. } | InventoryRequest::SplitToWorld { .. }
        ) != prepared.drop_shape.is_some()
        {
            return Err(E::MissingGeometry);
        }
        let constructed_source = match current.request {
            InventoryRequest::Move { item, .. } => {
                let mut ancestry = BTreeSet::new();
                let root = self.inventory_live_root(current.context.actor, item, &mut ancestry)?;
                let source_tree = self.inventory.tree_members(item)?;
                let contains_constructed = source_tree
                    .iter()
                    .any(|id| self.constructed_creatures.contains(*id));
                if contains_constructed {
                    let rows: BTreeSet<_> = current.rows.iter().map(|row| row.id).collect();
                    if root.is_none()
                        || root != current.target
                        || !source_tree.iter().all(|id| rows.contains(id))
                    {
                        return Err(E::InvalidState);
                    }
                }
                contains_constructed
            }
            _ => false,
        };
        if prepared.constructed_acquisition != constructed_source {
            return Err(E::InvalidState);
        }
        self.inventory_commands.live.insert(
            correlation,
            Live {
                prepared,
                control: None,
                stage: Stage::Approach,
                deadline: self.tick.checked_add(450).ok_or(E::Overflow)?,
            },
        );
        Ok(InventoryDecision::MotionStarted)
    }
    pub(in crate::kernel) fn step_inventory_live(&mut self) {
        let ids: Vec<_> = self
            .inventory_commands
            .live
            .keys()
            .copied()
            .take(64)
            .collect();
        for id in ids {
            if self.inventory_commands.outcomes.len() + 2 > self.inventory_commands.capacity {
                // A retained movement intent must not keep walking while its
                // controller is unable to publish/recheck the next transition.
                let live = self
                    .inventory_commands
                    .live
                    .get_mut(&id)
                    .expect("selected live inventory");
                if let Some(control) = live.control.take() {
                    let actor = live.prepared.evidence.context.actor;
                    self.world.finish_server_move(actor, control);
                    if let Ok(body) = self.world.body_mut(actor) {
                        body.finish_server_turn(control);
                    }
                }
                continue;
            }
            let mut live = self
                .inventory_commands
                .live
                .remove(&id)
                .expect("selected live inventory");
            let actor = live.prepared.evidence.context.actor;
            let result = self.advance_inventory_live(id, &mut live);
            let done = match result {
                Ok(done) => done,
                Err(error) => {
                    live.stage = Stage::Failed(error);
                    false
                }
            };
            if !done {
                self.inventory_commands.live.insert(id, live);
            } else if let Some(control) = live.control {
                self.world.finish_server_move(actor, control);
                if let Ok(body) = self.world.body_mut(actor) {
                    body.finish_server_turn(control);
                }
            }
        }
    }
    fn advance_inventory_live(&mut self, id: u64, live: &mut Live) -> Result<bool, E> {
        let actor = live.prepared.evidence.context.actor;
        match live.stage {
            Stage::Failed(error) => {
                if let Some(token) = self
                    .world
                    .source_motion_token(actor)
                    .filter(|t| t.domain == MotionDomain::Inventory && t.owner == id)
                {
                    self.world
                        .cancel_motion(actor, token)
                        .map_err(|_| E::Capacity)?;
                    for _ in 0..128 {
                        if self
                            .world
                            .take_motion_event_matching(actor, token)
                            .is_none()
                        {
                            break;
                        }
                    }
                }
                self.inventory_commands
                    .outcomes
                    .push_back(InventoryOutcome {
                        correlation: id,
                        result: Err(error),
                    });
                return Ok(true);
            }
            Stage::Returning(token) => {
                for _ in 0..128 {
                    let Some(event) = self.world.take_motion_event_matching(actor, token) else {
                        break;
                    };
                    if matches!(
                        event.event,
                        MotionExecutionEvent::Completed | MotionExecutionEvent::Cancelled
                    ) {
                        return Ok(true);
                    }
                }
                return Ok(self.world.source_motion_token(actor) != Some(token));
            }
            _ => {}
        }
        if self.tick >= live.deadline
            || self.world.body(actor).is_err()
            || self.world.body(actor).map(|b| b.accepted().epoch()).ok()
                != Some(live.prepared.evidence.epoch)
        {
            return Err(E::InvalidState);
        }
        match live.stage {
            Stage::Approach => {
                if !self.inventory_live_approach(live)? {
                    return Ok(false);
                }
                let motion = self.inventory_live_pickup(live)?;
                let chain = live
                    .prepared
                    .motions
                    .get(&motion)
                    .ok_or(E::MissingGeometry)?
                    .clone();
                let token = MotionToken {
                    domain: MotionDomain::Inventory,
                    owner: id,
                    sequence: 1,
                };
                let style = chain
                    .source_transition()
                    .ok_or(E::MissingGeometry)?
                    .before
                    .style;
                self.world
                    .begin_motion(actor, token, chain)
                    .map_err(|_| E::Busy)?;
                self.inventory_commands
                    .outcomes
                    .push_back(InventoryOutcome {
                        correlation: id,
                        result: Ok(InventoryDecision::Motion(crate::InventoryMotion {
                            actor,
                            style,
                            command: Some(motion),
                        })),
                    });
                live.stage = Stage::Animation(token);
            }
            Stage::Animation(token) => {
                for _ in 0..128 {
                    let Some(event) = self.world.take_motion_event_matching(actor, token) else {
                        break;
                    };
                    match event.event {
                        MotionExecutionEvent::Completed => {
                            live.stage = Stage::Callback;
                            break;
                        }
                        MotionExecutionEvent::Cancelled => return Err(E::InvalidState),
                        _ => {}
                    }
                }
            }
            Stage::Callback => {
                if !self.inventory_live_in_range(live)? {
                    return Err(E::OutOfRange);
                }
                for before in &live.prepared.evidence.rows {
                    if self.inventory.item(before.id) != Some(before) {
                        return Err(E::InvalidState);
                    }
                }
                if self.characters.reserved(actor)
                    || self.magic.busy(actor)
                    || self.portals.reserved(actor)
                    || self.recall_busy(actor)
                {
                    return Err(E::Busy);
                }
                if let Some(shape) = live.prepared.drop_shape.clone() {
                    live.prepared.request.drop =
                        Some(Box::new(self.inventory_live_drop(actor, shape)?));
                }
                live.prepared.request.authority.geometry_ready = true;
                live.prepared.request.authority.in_range = true;
                live.prepared.request.authority.clear_path = true;
                live.prepared.request.authority.source_view = self.inventory_live_source_view(
                    actor,
                    live.prepared.evidence.request,
                    live.prepared.evidence.target,
                )?;
                // Move out the prepared operation only once all fallible cold/physical gates passed.
                let request = InventoryPreparedRequest {
                    context: live.prepared.request.context,
                    request: live.prepared.request.request,
                    authority: live.prepared.request.authority,
                    split: live.prepared.request.split.take(),
                    drop: live.prepared.request.drop.take(),
                };
                self.world
                    .end_inventory_motion(actor, id)
                    .map_err(|_| E::Capacity)?;
                let style = self
                    .world
                    .source_motion_state(actor)
                    .ok_or(E::InvalidState)?
                    .style;
                let operation = if live.prepared.constructed_acquisition {
                    self.prepare_constructed_inventory_operation(request)?
                } else {
                    self.prepare_inventory_operation(request)?
                };
                self.inventory_commands
                    .outcomes
                    .push_back(InventoryOutcome {
                        correlation: id,
                        result: Ok(InventoryDecision::Motion(crate::InventoryMotion {
                            actor,
                            style,
                            command: None,
                        })),
                    });
                self.inventory_commands
                    .outcomes
                    .push_back(InventoryOutcome {
                        correlation: id,
                        result: Ok(InventoryDecision::Proposed(Box::new(operation))),
                    });
                live.stage = Stage::Returning(MotionToken {
                    domain: MotionDomain::Inventory,
                    owner: id,
                    sequence: 2,
                });
            }
            _ => {}
        }
        Ok(false)
    }
    fn inventory_live_source_view(
        &self,
        actor: EntityId,
        request: InventoryRequest,
        target: Option<EntityId>,
    ) -> Result<Option<u64>, E> {
        if let Some(corpse) = target.filter(|id| self.world.corpse(*id).is_some())
            && self
                .player_deaths
                .corpse_access
                .get(&corpse)
                .is_none_or(|access| access.viewer != Some(actor))
        {
            return Err(E::AccessDenied);
        }
        let (source, _) = ids(request);
        let item = self.inventory.item(source).ok_or(E::MissingItem)?;
        let ItemPlace::Contained { container, .. } = item.place else {
            return Ok(None);
        };
        let parent = self
            .inventory
            .container(container)
            .ok_or(E::MissingContainer)?;
        if parent.root_owner == Some(actor) {
            return Ok(None);
        }
        if !parent.open || !parent.accessible {
            return Err(E::AccessDenied);
        }
        Ok(Some(parent.generation))
    }
}
fn ids(request: InventoryRequest) -> (EntityId, Option<EntityId>) {
    match request {
        InventoryRequest::Move {
            item, container, ..
        }
        | InventoryRequest::SplitToContainer {
            item, container, ..
        } => (item, Some(container)),
        InventoryRequest::Merge { source, target, .. } => (source, Some(target)),
        InventoryRequest::Drop { item }
        | InventoryRequest::SplitToWorld { item, .. }
        | InventoryRequest::Equip { item, .. }
        | InventoryRequest::SplitToWield { item, .. } => (item, None),
    }
}
