mod bindings;
mod motion;
use super::*;
use crate::recalls::{
    PendingRecall, PreparedRecallHouse, PreparedRecallLocations, RecallCommand, RecallEvent,
};
use bace_interactions::{PortalPosition, RecallAccess, RecallError, RecallKind};
use std::sync::Arc;
impl Kernel {
    pub(super) fn capture_recall_destinations(
        &self,
        binding: CharacterBinding,
    ) -> crate::RecallDestinationSnapshot {
        let context = ActionContext {
            session: binding.session,
            account: binding.account,
            actor: binding.actor,
            sequence: 0,
        };
        let location = |kind| self.resolve_recall_destination(context, kind, 0);
        crate::RecallDestinationSnapshot {
            lifestone: location(RecallKind::Lifestone),
            house: location(RecallKind::House),
            marketplace: location(RecallKind::Marketplace),
            allegiance_hometown: location(RecallKind::AllegianceHometown),
            allegiance_housing: location(RecallKind::AllegianceHousing),
            pk_arena: self
                .recalls
                .locations
                .as_ref()
                .map(|p| p.pk_arena)
                .ok_or(RecallError::MissingAssets),
            pkl_arena: self
                .recalls
                .locations
                .as_ref()
                .map(|p| p.pkl_arena)
                .ok_or(RecallError::MissingAssets),
        }
    }

    fn recall_allegiance_hometown(&self, actor: EntityId) -> Result<PortalPosition, RecallError> {
        if self.allegiance_relation(actor).is_none() {
            return Err(RecallError::NoAllegiance);
        }
        let (_, _, position) = self
            .allegiance_hometown(actor)
            .ok_or(RecallError::NoHometown)?;
        Ok(PortalPosition {
            cell: position.cell,
            origin: position.origin,
            rotation: position.rotation,
        })
    }
    fn recall_allegiance_house(&self, actor: EntityId) -> Result<PortalPosition, RecallError> {
        let relation = self
            .allegiance_relation(actor)
            .ok_or(RecallError::NoAllegiance)?;
        let anchor = self
            .recalls
            .houses
            .values()
            .find(|anchor| {
                self.housing
                    .state(anchor.house)
                    .is_some_and(|h| h.owner == Some(relation.monarch))
            })
            .ok_or(RecallError::NoMansion)?;
        if !matches!(anchor.house_type, 2 | 3) {
            return Err(RecallError::WrongHouseType);
        }
        let house = self
            .housing
            .state(anchor.house)
            .ok_or(RecallError::NoMansion)?;
        if house.allegiance_monarch.is_none() {
            return Err(RecallError::MansionClosed);
        }
        Ok(anchor.destination)
    }
    pub fn configure_recall_random(
        &mut self,
        root: Arc<bace_random::RandomRoot>,
        epoch: u64,
    ) -> Result<(), RecallError> {
        if epoch == 0 || self.recalls.has_state() || self.recalls.random.is_some() {
            return Err(RecallError::Invalid);
        }
        self.recalls.random = Some(root);
        self.recalls.epoch = epoch;
        Ok(())
    }
    pub fn register_recall_locations(
        &mut self,
        locations: Arc<PreparedRecallLocations>,
    ) -> Result<(), RecallError> {
        if self.recalls.has_state() {
            return Err(RecallError::Busy);
        }
        for position in std::iter::once(&locations.marketplace)
            .chain(&locations.pk_arena)
            .chain(&locations.pkl_arena)
        {
            position.validate().map_err(|_| RecallError::Invalid)?;
        }
        self.recalls.locations = Some(locations);
        Ok(())
    }
    pub fn register_recall_house(&mut self, house: PreparedRecallHouse) -> Result<(), RecallError> {
        house
            .destination
            .validate()
            .map_err(|_| RecallError::Invalid)?;
        if house.house.0 == 0 || house.slumlord.0 == 0 || !(1..=4).contains(&house.house_type) {
            return Err(RecallError::Invalid);
        }
        if self.recalls.houses.len() >= 4096 || self.recalls.houses.contains_key(&house.house) {
            return Err(RecallError::Capacity);
        }
        self.recalls.houses.insert(house.house, house);
        Ok(())
    }
    pub fn recall_busy(&self, actor: EntityId) -> bool {
        self.recalls.pending.contains_key(&actor)
            || self.recalls.pending_bindings.contains_key(&actor)
    }
    pub fn has_recall_state(&self) -> bool {
        self.recalls.has_state()
    }
    pub fn peek_recall_event(&self) -> Option<&RecallEvent> {
        self.recalls.events.front()
    }
    pub fn take_recall_event(&mut self) -> Option<RecallEvent> {
        self.recalls.events.pop_front()
    }
    pub fn apply_recall_command(&mut self, command: RecallCommand) -> Result<(), RecallError> {
        if self.recalls.events.len() >= self.recalls.capacity {
            return Err(RecallError::Capacity);
        }
        match command {
            RecallCommand::StartPrepared {
                context,
                kind,
                before_revision,
                animation_seconds,
                style,
                motion,
            } => {
                if !self.characters.binding_matches(CharacterBinding {
                    actor: context.actor,
                    account: context.account,
                    session: context.session,
                }) {
                    self.recalls.events.push_back(RecallEvent::Rejected {
                        context,
                        error: RecallError::Stale,
                    });
                    return Ok(());
                }
                if self
                    .characters
                    .get(context.actor)
                    .is_none_or(|c| c.revision() != before_revision)
                {
                    self.recalls
                        .events
                        .push_back(RecallEvent::Retry { context });
                    return Ok(());
                }
                let result = self
                    .characters
                    .authorize(context, self.world.body(context.actor).is_ok())
                    .map_err(|_| RecallError::Stale)
                    .and_then(|()| {
                        self.start_recall(context, kind, animation_seconds, Some((style, motion)))
                    });
                if let Err(error) = result {
                    self.recalls
                        .events
                        .push_back(RecallEvent::Rejected { context, error });
                }
                Ok(())
            }
            RecallCommand::UseBinding {
                context,
                object,
                before_revision,
                animation_seconds,
                style,
                motion,
            } => {
                if !self.characters.binding_matches(CharacterBinding {
                    actor: context.actor,
                    account: context.account,
                    session: context.session,
                }) {
                    self.recalls.events.push_back(RecallEvent::Rejected {
                        context,
                        error: RecallError::Stale,
                    });
                    return Ok(());
                }
                if self
                    .characters
                    .get(context.actor)
                    .is_none_or(|c| c.revision() != before_revision)
                {
                    self.recalls
                        .events
                        .push_back(RecallEvent::Retry { context });
                    return Ok(());
                }
                if self
                    .characters
                    .authorize(context, self.world.body(context.actor).is_ok())
                    .is_err()
                {
                    self.recalls.events.push_back(RecallEvent::Rejected {
                        context,
                        error: RecallError::Stale,
                    });
                    return Ok(());
                }
                if let Err(error) =
                    self.start_binding(context, object, animation_seconds, style, motion)
                {
                    self.recalls
                        .events
                        .push_back(RecallEvent::Rejected { context, error });
                }
                Ok(())
            }
            RecallCommand::Cancel { actor } => {
                if !self.stop_recall_motion(actor) {
                    return Err(RecallError::Busy);
                }
                if let Some(pending) = self.recalls.pending.remove(&actor) {
                    self.recalls.events.push_back(RecallEvent::Cancelled {
                        context: pending.context,
                    });
                }
                if let Some(pending) = self.recalls.pending_bindings.remove(&actor) {
                    self.recalls.events.push_back(RecallEvent::Cancelled {
                        context: pending.context,
                    });
                }
                Ok(())
            }
            RecallCommand::Start {
                context,
                kind,
                animation_seconds,
            } => {
                self.characters
                    .authorize(context, self.world.body(context.actor).is_ok())
                    .map_err(|_| RecallError::Stale)?;
                match self.start_recall(context, kind, animation_seconds, None) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        self.recalls
                            .events
                            .push_back(RecallEvent::Rejected { context, error });
                        Ok(())
                    }
                }
            }
        }
    }
    pub(in crate::kernel) fn accepted_portal_position(
        &self,
        actor: EntityId,
    ) -> Result<PortalPosition, RecallError> {
        let (cell, state) = self
            .world
            .actor_state(actor)
            .map_err(|_| RecallError::Invalid)?;
        let (sin, cos) = (state.heading_radians() * 0.5).sin_cos();
        let p = state.position();
        Ok(PortalPosition {
            cell: cell.0,
            origin: [p.x, p.y, p.z],
            rotation: [cos, 0., 0., sin],
        })
    }
    fn recall_access(&self, actor: EntityId) -> Result<RecallAccess, RecallError> {
        let p = self
            .live_portal_access(actor)
            .map_err(|_| RecallError::MissingAssets)?;
        let recalls_disabled = self
            .world
            .properties(actor)
            .and_then(|p| p.get(bace_entity::PropertyFamily::Bool, 107))
            .is_some_and(|p| matches!(p, bace_entity::PropertyValue::Bool(true)));
        Ok(RecallAccess {
            olthoi: p.olthoi,
            pk_status: p.pk_status,
            pk_recent: p.pk_recent,
            recalls_disabled,
            busy: self.recall_busy(actor)
                || self.magic.busy(actor)
                || self.world.motion_busy(actor)
                || self.portals.reserved(actor)
                || self.world.is_in_portal_transit(actor)
                || self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.housing.reserved(actor)
                || self.npcs.reserved(actor),
            suicide_in_progress: false,
        })
    }
    fn start_recall(
        &mut self,
        context: ActionContext,
        kind: RecallKind,
        animation_seconds: f64,
        motion: Option<crate::recalls::PreparedRecallChains>,
    ) -> Result<(), RecallError> {
        if self.recalls.pending.len() >= self.recalls.capacity {
            return Err(RecallError::Capacity);
        }
        if self
            .world
            .combatant(context.actor)
            .is_none_or(|c| c.health() == 0)
        {
            return Err(RecallError::Busy);
        }
        bace_interactions::check_recall(
            &self.recall_policy,
            kind,
            self.recall_access(context.actor)?,
        )?;
        let delay = bace_interactions::recall_delay_ticks(kind, animation_seconds)?;
        let due = self.tick.checked_add(delay).ok_or(RecallError::Invalid)?;
        let ordinal = self
            .recalls
            .next
            .checked_add(1)
            .ok_or(RecallError::Capacity)?;
        let destination = self.resolve_recall_destination(context, kind, ordinal)?;
        // Geometry must already be prepared; content/asset failure consumes no mana or RNG ordinal.
        let movement = self
            .portal_teleport(context.actor, destination)
            .map_err(|_| RecallError::MissingAssets)?;
        self.world
            .validate_teleport_batch(&[movement])
            .map_err(|_| RecallError::MissingAssets)?;
        let start = self.accepted_portal_position(context.actor)?;
        let start_epoch = self
            .world
            .actor_state(context.actor)
            .map_err(|_| RecallError::Stale)?
            .1
            .epoch();
        let mana_change = if kind == RecallKind::Lifestone {
            let before = self
                .world
                .vital(context.actor, bace_entity::EntityVital::Mana)
                .map_err(|_| RecallError::MissingAssets)?
                .current;
            Some(bace_entity::VitalMutation {
                actor: context.actor,
                vital: bace_entity::EntityVital::Mana,
                before,
                after: before / 2,
            })
        } else {
            None
        };
        if let Some(change) = mana_change {
            self.world
                .validate_vital_batch(&[change], None)
                .map_err(|_| RecallError::Busy)?;
        }
        let combat_mode_changed = motion.is_some()
            && self
                .world
                .combatant(context.actor)
                .is_some_and(|c| c.mode() != 1);
        let motion = motion
            .map(|(style, chain)| {
                self.begin_recall_motion(context.actor, ordinal, kind, style, chain)
            })
            .transpose()?;
        if let Some(change) = mana_change {
            // Beginning a motion cannot alter a vital, revision or reservation;
            // this batch was checked on the same simulation owner above.
            self.world
                .apply_vital_batch(&[change], None)
                .expect("recall vital batch preflighted before motion admission");
        }
        let mana_after = mana_change.map(|c| c.after);
        self.recalls.next = ordinal;
        self.recalls.pending.insert(
            context.actor,
            PendingRecall {
                context,
                kind,
                start,
                start_epoch,
                due,
                destination,
                motion,
            },
        );
        self.recalls.events.push_back(RecallEvent::Started {
            context,
            kind,
            motion: kind.motion(),
            until_tick: due,
            mana_after,
            combat_mode_changed,
        });
        Ok(())
    }
    fn resolve_recall_destination(
        &self,
        context: ActionContext,
        kind: RecallKind,
        ordinal: u64,
    ) -> Result<PortalPosition, RecallError> {
        match kind {
            RecallKind::Lifestone => self
                .portal_links(context.actor)
                .and_then(|p| p.position(4))
                .ok_or(RecallError::NoSanctuary),
            RecallKind::House => {
                let actor = bace_housing::HousingActor {
                    actor: context.actor,
                    account: context.account.0,
                    level: 0,
                    monarch: false,
                    allegiance_rank: 0,
                    account_age_seconds: 0,
                    previous_purchase: 0,
                    owns_house: false,
                    in_range: false,
                };
                let house = self.housing.query(actor).ok_or(RecallError::NoHouse)?;
                self.recalls
                    .houses
                    .get(&house.house)
                    .map(|h| h.destination)
                    .ok_or(RecallError::MissingAssets)
            }
            RecallKind::Marketplace => self
                .recalls
                .locations
                .as_ref()
                .map(|l| l.marketplace)
                .ok_or(RecallError::MissingAssets),
            RecallKind::PkArena | RecallKind::PklArena => {
                let locations = self
                    .recalls
                    .locations
                    .as_ref()
                    .ok_or(RecallError::MissingAssets)?;
                let root = self
                    .recalls
                    .random
                    .as_ref()
                    .ok_or(RecallError::MissingAssets)?;
                let mut id = [0; 16];
                id[..8].copy_from_slice(&self.recalls.epoch.to_le_bytes());
                id[8..].copy_from_slice(&ordinal.to_le_bytes());
                let mut random = root
                    .event_stream(id, bace_random::Domain::Recall)
                    .and_then(|r| r.fork(b"actor", u64::from(context.actor.0)))
                    .map_err(|_| RecallError::Invalid)?;
                let index = random.below(5).map_err(|_| RecallError::Invalid)? as usize;
                Ok(if kind == RecallKind::PkArena {
                    locations.pk_arena[index]
                } else {
                    locations.pkl_arena[index]
                })
            }
            RecallKind::AllegianceHometown => self.recall_allegiance_hometown(context.actor),
            RecallKind::AllegianceHousing => self.recall_allegiance_house(context.actor),
        }
    }
    pub(in crate::kernel) fn step_recalls(&mut self) {
        self.step_recall_motions();
        self.step_bindings();
        self.recalls.scratch.clear();
        self.recalls.scratch.extend(
            self.recalls
                .pending
                .iter()
                .filter_map(|(&id, p)| (p.due <= self.tick).then_some(id))
                .take(32),
        );
        for i in 0..self.recalls.scratch.len() {
            if self.recalls.events.len() == self.recalls.capacity {
                break;
            }
            let actor = self.recalls.scratch[i];
            if !self.stop_recall_motion(actor) {
                continue;
            }
            let p = &self.recalls.pending[&actor];
            let result = self.accepted_portal_position(actor).and_then(|position| {
                if bace_interactions::recall_moved_too_far(p.start, position)? {
                    Err(RecallError::MovedTooFar)
                } else {
                    Ok(())
                }
            });
            let result = result.and_then(|()| match p.kind {
                RecallKind::AllegianceHometown => self.recall_allegiance_hometown(actor),
                RecallKind::AllegianceHousing => self.recall_allegiance_house(actor),
                _ => Ok(p.destination),
            });
            let context = p.context;
            let kind = p.kind;
            let result = result.and_then(|destination| {
                self.stage_command_recall(actor, kind, destination)
                    .map_err(|e| match e {
                        bace_interactions::PortalError::Capacity
                        | bace_interactions::PortalError::Conflict => RecallError::Capacity,
                        bace_interactions::PortalError::MissingAssets => RecallError::MissingAssets,
                        _ => RecallError::Invalid,
                    })
            });
            match result {
                Err(RecallError::Capacity | RecallError::MissingAssets) => continue,
                Err(error) => self
                    .recalls
                    .events
                    .push_back(RecallEvent::Rejected { context, error }),
                Ok(operation) => self.recalls.events.push_back(RecallEvent::Staged {
                    context,
                    kind,
                    operation,
                }),
            }
            self.recalls.pending.remove(&actor);
        }
    }
}
