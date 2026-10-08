use super::*;
use crate::recalls::{PendingBinding, PreparedBindingObject};
use bace_interactions::BindingKind;
impl Kernel {
    pub fn preflight_binding_objects(
        &self,
        objects: &[PreparedBindingObject],
    ) -> Result<(), RecallError> {
        if self.recalls.bindings.len().saturating_add(objects.len()) > 4096 {
            return Err(RecallError::Capacity);
        }
        let mut ids = std::collections::BTreeSet::new();
        for object in objects {
            if object.entity.0 == 0
                || !object.use_radius.is_finite()
                || !(0.0..=100.).contains(&object.use_radius)
                || object.use_message.len() > 4096
                || !ids.insert(object.entity)
                || self.recalls.bindings.contains_key(&object.entity)
            {
                return Err(RecallError::Invalid);
            }
        }
        Ok(())
    }
    pub fn register_binding_object(
        &mut self,
        object: PreparedBindingObject,
    ) -> Result<(), RecallError> {
        self.preflight_binding_objects(std::slice::from_ref(&object))?;
        if self.world.body(object.entity).is_err() {
            return Err(RecallError::Invalid);
        }
        self.recalls.bindings.insert(object.entity, object);
        Ok(())
    }
    pub fn unregister_binding_objects(&mut self, objects: &[EntityId]) -> Result<(), RecallError> {
        let affected: Vec<_> = self
            .recalls
            .pending_bindings
            .iter()
            .filter_map(|(&actor, p)| {
                objects
                    .contains(&p.object)
                    .then_some((actor, p.context, p.motion_owner))
            })
            .collect();
        if self.recalls.events.len().saturating_add(affected.len()) > self.recalls.capacity {
            return Err(RecallError::Capacity);
        }
        for &(actor, _, owner) in &affected {
            if !self.stop_binding_motion(actor, owner) {
                return Err(RecallError::Busy);
            }
        }
        for (actor, context, _) in affected {
            self.recalls.pending_bindings.remove(&actor);
            self.recalls
                .events
                .push_back(RecallEvent::Cancelled { context });
        }
        for object in objects {
            self.recalls.bindings.remove(object);
        }
        Ok(())
    }
    fn binding_in_range(
        &self,
        actor: EntityId,
        object: &PreparedBindingObject,
    ) -> Result<bool, RecallError> {
        let (a, ap) = self
            .world
            .actor_state(actor)
            .map_err(|_| RecallError::Invalid)?;
        let (b, bp) = self
            .world
            .actor_state(object.entity)
            .map_err(|_| RecallError::Invalid)?;
        // Cross-landblock interaction requires a prepared shared frame. Outdoor
        // offsets are authoritative; unrelated dungeon frames cannot be compared.
        if a.0 >> 16 != b.0 >> 16 && (a.0 & 65535 >= 256 || b.0 & 65535 >= 256) {
            return Ok(false);
        }
        let ab = self.world.body(actor).map_err(|_| RecallError::Invalid)?;
        let bb = self
            .world
            .body(object.entity)
            .map_err(|_| RecallError::Invalid)?;
        let ah = ab
            .collision_shape()
            .map_or(ab.collision_radius() * 2., |s| s.height());
        let bh = bb
            .collision_shape()
            .map_or(bb.collision_radius() * 2., |s| s.height());
        let mut offset = bp.position() - ap.position();
        offset.x += (((b.0 >> 24) as i32 - (a.0 >> 24) as i32) * 192) as f32;
        offset.y += ((((b.0 >> 16) & 255) as i32 - ((a.0 >> 16) & 255) as i32) * 192) as f32;
        let reach =
            offset.length_squared().sqrt() - (ab.collision_radius() + bb.collision_radius());
        let dz = if ap.position().z <= bp.position().z {
            bp.position().z - (ap.position().z + ah)
        } else {
            ap.position().z - (bp.position().z + bh)
        };
        let distance = if dz > 0. && reach > 0. {
            f64::from(dz * dz + reach * reach).sqrt()
        } else if dz < 0. && reach < 0. {
            -f64::from(dz * dz + reach * reach).sqrt()
        } else {
            f64::from(reach)
        };
        Ok(distance as f32 <= object.use_radius)
    }
    pub(super) fn start_binding(
        &mut self,
        context: ActionContext,
        object: EntityId,
        animation_seconds: f64,
        style: Option<Arc<bace_motion::PreparedMotionChain>>,
        motion: Arc<bace_motion::PreparedMotionChain>,
    ) -> Result<(), RecallError> {
        if self
            .world
            .combatant(context.actor)
            .is_none_or(|c| c.health() == 0)
        {
            return Err(RecallError::Busy);
        }
        if self.recall_access(context.actor)?.busy
            || self.recalls.pending_bindings.len() >= self.recalls.capacity
        {
            return Err(RecallError::Busy);
        }
        let object = self
            .recalls
            .bindings
            .get(&object)
            .ok_or(RecallError::MissingAssets)?
            .clone();
        let relation = self.allegiance_relation(context.actor);
        bace_interactions::check_binding(
            &self.recall_policy,
            object.kind,
            relation.is_some(),
            relation.map_or(0, |r| r.officer_level),
        )?;
        if !self.binding_in_range(context.actor, &object)? {
            return Err(RecallError::MovedTooFar);
        }
        let delay =
            bace_interactions::recall_delay_ticks(RecallKind::Lifestone, animation_seconds)?;
        let due = self.tick.checked_add(delay).ok_or(RecallError::Invalid)?;
        let motion_owner = self
            .recalls
            .next
            .checked_add(1)
            .ok_or(RecallError::Capacity)?;
        let start_epoch = self
            .world
            .actor_state(context.actor)
            .map_err(|_| RecallError::Stale)?
            .1
            .epoch();
        let has_style = style.is_some();
        self.begin_binding_motion(context.actor, motion_owner, style, motion)?;
        self.recalls.next = motion_owner;
        self.recalls.pending_bindings.insert(
            context.actor,
            PendingBinding {
                context,
                object: object.entity,
                due,
                motion_owner,
                start_epoch,
                completed: None,
                action_sequence: if has_style { 2 } else { 1 },
                action_started: !has_style,
                action_announced: !has_style,
            },
        );
        self.recalls.events.push_back(RecallEvent::BindingStarted {
            context,
            object: object.entity,
            until_tick: due,
        });
        Ok(())
    }
    pub(super) fn step_bindings(&mut self) {
        self.recalls.scratch.clear();
        self.recalls.scratch.extend(
            self.recalls
                .pending_bindings
                .iter()
                .filter_map(|(&id, p)| {
                    (p.due <= self.tick
                        || p.completed == Some(false)
                        || p.action_started && !p.action_announced
                        || !self
                            .world
                            .actor_state(id)
                            .is_ok_and(|(_, state)| state.epoch() == p.start_epoch))
                    .then_some(id)
                })
                .take(32),
        );
        for i in 0..self.recalls.scratch.len() {
            if self.recalls.events.len() >= self.recalls.capacity {
                break;
            }
            let actor = self.recalls.scratch[i];
            let p = &self.recalls.pending_bindings[&actor];
            let (context, object, action_ready) =
                (p.context, p.object, p.action_started && !p.action_announced);
            if action_ready {
                self.recalls
                    .events
                    .push_back(RecallEvent::BindingActionStarted { context, object });
                self.recalls
                    .pending_bindings
                    .get_mut(&actor)
                    .expect("retained binding")
                    .action_announced = true;
                if self.recalls.events.len() >= self.recalls.capacity {
                    continue;
                }
            }
            let p = &self.recalls.pending_bindings[&actor];
            let cancelled = p.completed == Some(false)
                || !self
                    .world
                    .actor_state(actor)
                    .is_ok_and(|(_, state)| state.epoch() == p.start_epoch);
            if !cancelled && p.due > self.tick {
                continue;
            }
            if !self.stop_binding_motion(actor, p.motion_owner) {
                continue;
            }
            if cancelled {
                self.recalls
                    .events
                    .push_back(RecallEvent::Cancelled { context });
                self.recalls.pending_bindings.remove(&actor);
                continue;
            }
            match self.finish_binding(context, object) {
                Err(RecallError::Capacity | RecallError::Busy) => continue,
                Err(error) => self
                    .recalls
                    .events
                    .push_back(RecallEvent::Rejected { context, error }),
                Ok((operation, allegiance, use_message, stamina_after)) => {
                    self.recalls.events.push_back(RecallEvent::BindingStaged {
                        context,
                        object,
                        operation,
                        allegiance,
                        use_message,
                        stamina_after,
                    })
                }
            }
            self.recalls.pending_bindings.remove(&actor);
        }
    }
    fn finish_binding(
        &mut self,
        context: ActionContext,
        object: EntityId,
    ) -> Result<(u64, bool, Arc<str>, Option<u32>), RecallError> {
        if self
            .world
            .combatant(context.actor)
            .is_none_or(|c| c.health() == 0)
        {
            return Err(RecallError::Invalid);
        }
        let object = self
            .recalls
            .bindings
            .get(&object)
            .ok_or(RecallError::MissingAssets)?
            .clone();
        let within = self.binding_in_range(context.actor, &object)?;
        let position = self.accepted_portal_position(context.actor)?;
        let stamina = self
            .world
            .vital(context.actor, bace_entity::EntityVital::Stamina)
            .map_err(|_| RecallError::MissingAssets)?
            .current;
        let effect = bace_interactions::complete_binding(object.kind, position, within, stamina)?;
        match object.kind {
            BindingKind::Allegiance => {
                let result = self
                    .prepare_allegiance_sanctuary(
                        context.actor,
                        bace_gameplay_api::social::AllegianceSanctuary {
                            cell: position.cell,
                            origin: position.origin,
                            rotation: position.rotation,
                        },
                    )
                    .map_err(|e| match e {
                        bace_gameplay_api::social::SocialError::Busy
                        | bace_gameplay_api::social::SocialError::Capacity => RecallError::Busy,
                        _ => RecallError::NoAllegiance,
                    })?;
                Ok((result.operation, true, object.use_message, None))
            }
            BindingKind::Lifestone => self
                .stage_lifestone_binding(
                    context.actor,
                    effect.sanctuary,
                    stamina,
                    effect.stamina_after.expect("lifestone stamina"),
                )
                .map(|op| (op, false, object.use_message, effect.stamina_after)),
        }
    }
}
