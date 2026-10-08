//! Portal services own links and bounded durable proposals; World alone owns the
//! stationary portal bodies and accepted destination changes.
mod commands;
mod completion;
mod preparation;
use super::*;
use bace_entity::{EntityVital, VitalMutation};
use bace_interactions::{
    PortalAccess, PortalAnchor, PortalError, PortalLinkMutation, PortalLinks, PortalPosition,
};
use bace_magic::PortalEffect;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq)]
pub enum PortalServiceEffect {
    Sanctuary {
        link: PortalLinkMutation,
        character: Box<bace_character::CharacterServiceChange>,
        stamina: VitalMutation,
    },
    Link(PortalLinkMutation),
    Teleport(Vec<bace_world::WorldTeleport>),
    Summon {
        entity: EntityId,
        template: u32,
        original_template: u32,
        origin: PortalPosition,
        destination: PortalPosition,
        lifetime: f64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalServiceOrigin {
    Emote { ticket: u64 },
    Spell,
    Recall(bace_interactions::RecallKind),
    Binding,
    Death,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PortalServiceTicket {
    pub origin: PortalServiceOrigin,
    pub operation: u64,
    pub cast: u64,
    pub actor: EntityId,
    pub cast_actor: EntityId,
    pub before_revision: u64,
    pub after_revision: u64,
    pub participants: Vec<(EntityId, u64, u64)>,
    pub mana: Option<VitalMutation>,
    pub effect: PortalServiceEffect,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortalServiceReceipt {
    pub operation: u64,
    pub actor: EntityId,
    pub after_revision: u64,
    pub revisions: Vec<(EntityId, u64)>,
}
pub(super) struct PortalServices {
    anchors: BTreeMap<EntityId, Arc<PortalAnchor>>,
    templates: BTreeMap<u32, Arc<bace_interactions::PortalTemplate>>,
    links: BTreeMap<EntityId, PortalLinks>,
    access: BTreeMap<EntityId, PortalAccess>,
    pending: BTreeMap<u64, PendingPortalService>,
    shapes: BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
    spawned: BTreeMap<EntityId, f64>,
    awaiting: BTreeMap<EntityId, PortalCompletion>,
    events: VecDeque<PortalServiceEvent>,
    proposals: VecDeque<u64>,
    pub(super) resolutions: VecDeque<crate::PortalResolutionOutcome>,
    ids: VecDeque<EntityId>,
    next: u64,
    capacity: usize,
}
impl PortalServices {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            anchors: BTreeMap::new(),
            templates: BTreeMap::new(),
            links: BTreeMap::new(),
            access: BTreeMap::new(),
            pending: BTreeMap::new(),
            shapes: BTreeMap::new(),
            spawned: BTreeMap::new(),
            awaiting: BTreeMap::new(),
            events: VecDeque::with_capacity(capacity),
            proposals: VecDeque::with_capacity(capacity),
            resolutions: VecDeque::with_capacity(capacity),
            ids: VecDeque::new(),
            next: 0,
            capacity,
        }
    }
    pub(super) fn reserved(&self, actor: EntityId) -> bool {
        self.pending.values().any(|p|p.ticket.actor==actor||p.ticket.cast_actor==actor||matches!(&p.ticket.effect,PortalServiceEffect::Teleport(targets) if targets.iter().any(|t|t.actor==actor)))
    }
    pub(super) fn reserves_identity(&self, actor: EntityId) -> bool {
        self.ids.contains(&actor)||self.anchors.contains_key(&actor)||self.pending.values().any(|p|matches!(p.ticket.effect,PortalServiceEffect::Summon{entity,..} if entity==actor))
    }
    pub(super) fn has_state(&self) -> bool {
        !self.resolutions.is_empty()
            || !self.pending.is_empty()
            || !self.proposals.is_empty()
            || !self.events.is_empty()
            || !self.spawned.is_empty()
            || !self.awaiting.is_empty()
    }
}

struct PortalCompletion {
    operation: u64,
    epoch: u16,
    client_ready: bool,
    destination_ready: bool,
}
struct PendingPortalService {
    ticket: PortalServiceTicket,
    committed: bool,
    due: Option<f64>,
    prepared_anchor: Option<bace_entity::Actor>,
    blocked: bool,
    reserved_registries: Vec<EntityId>,
    mana_applied: bool,
    completed: Option<bool>,
}
/// Immutable accepted transform and source cloak policy at a portal boundary.
/// Output must never substitute a later read of the moving actor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalAcceptedView {
    pub actor: EntityId,
    pub position: PortalPosition,
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub epoch: u16,
    pub cloaked: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PortalServiceEvent {
    Hidden {
        operation: u64,
        actors: Vec<EntityId>,
        views: Vec<PortalAcceptedView>,
    },
    Teleported {
        operation: u64,
        actors: Vec<EntityId>,
        views: Vec<PortalAcceptedView>,
    },
    Materialized {
        operation: u64,
        actor: EntityId,
        view: PortalAcceptedView,
    },
    Summoned {
        operation: u64,
        entity: EntityId,
        template: u32,
    },
    Removed {
        entity: EntityId,
    },
    Linked {
        operation: u64,
        actor: EntityId,
    },
    Blocked {
        operation: u64,
        actor: EntityId,
    },
    AbortedAfterCommit {
        operation: u64,
        actor: EntityId,
    },
}
impl Kernel {
    fn accepted_portal_view(&self, actor: EntityId) -> PortalAcceptedView {
        let (_, state) = self
            .world
            .actor_state(actor)
            .expect("accepted portal actor");
        let velocity = state.velocity();
        PortalAcceptedView {
            actor,
            position: self
                .accepted_portal_position(actor)
                .expect("accepted portal position"),
            velocity: [velocity.x, velocity.y, velocity.z],
            grounded: state.grounded(),
            epoch: state.epoch(),
            cloaked: self
                .world
                .properties(actor)
                .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 128))
                .is_some_and(|p| matches!(p, bace_entity::PropertyValue::Int(2))),
        }
    }
    pub(in crate::kernel) fn validate_player_portal_admission(
        &self,
        actor: EntityId,
    ) -> Result<(), PortalError> {
        if self.portals.reserved(actor) || self.portals.links.contains_key(&actor) {
            return Err(PortalError::Conflict);
        }
        if actor.0 == 0 {
            return Err(PortalError::Invalid);
        }
        if self.portals.links.len() >= 4096 {
            return Err(PortalError::Capacity);
        }
        Ok(())
    }
    pub fn register_portal_links(
        &mut self,
        actor: EntityId,
        links: PortalLinks,
        access: PortalAccess,
    ) -> Result<(), PortalError> {
        if self.portals.reserved(actor) {
            return Err(PortalError::Conflict);
        }
        if self.characters.get(actor).is_none() {
            return Err(PortalError::Invalid);
        }
        if self.portals.links.len() >= 4096 && !self.portals.links.contains_key(&actor) {
            return Err(PortalError::Capacity);
        }
        self.portals.links.insert(actor, links);
        self.portals.access.insert(actor, access);
        Ok(())
    }
    pub fn register_portal_template(
        &mut self,
        anchor: PortalAnchor,
        shape: Arc<bace_physics::CollisionShape>,
    ) -> Result<(), PortalError> {
        anchor.validate()?;
        self.register_portal_definition(anchor.definition(), shape)
    }
    /// Cold referenced metadata has no fabricated world entity or accepted pose.
    pub(super) fn preflight_portal_definitions(
        &self,
        templates: &std::collections::BTreeSet<u32>,
    ) -> Result<(), PortalError> {
        if self.portals.templates.len()
            + templates
                .iter()
                .filter(|id| !self.portals.templates.contains_key(id))
                .count()
            > 65536
        {
            Err(PortalError::Capacity)
        } else {
            Ok(())
        }
    }
    pub fn register_portal_definition(
        &mut self,
        definition: bace_interactions::PortalTemplate,
        shape: Arc<bace_physics::CollisionShape>,
    ) -> Result<(), PortalError> {
        definition.validate()?;
        if self.portals.templates.len() >= 65536
            && !self.portals.templates.contains_key(&definition.template)
        {
            return Err(PortalError::Capacity);
        }
        self.portals.shapes.insert(definition.template, shape);
        self.portals
            .templates
            .insert(definition.template, Arc::new(definition));
        Ok(())
    }
    pub fn register_portal_anchor(
        &mut self,
        anchor: PortalAnchor,
        actor: bace_entity::Actor,
    ) -> Result<(), (PortalError, Box<bace_entity::Actor>)> {
        if anchor.validate().is_err()
            || anchor.entity != actor.id.0
            || anchor.position.cell != actor.cell.0
            || anchor.position.origin
                != [
                    actor.body.accepted().position().x,
                    actor.body.accepted().position().y,
                    actor.body.accepted().position().z,
                ]
            || self.portals.anchors.len() >= 65536
        {
            return Err((PortalError::Invalid, Box::new(actor)));
        }
        self.world
            .insert_anchor(actor)
            .map_err(|(_, actor)| (PortalError::Invalid, actor))?;
        self.portals
            .anchors
            .insert(EntityId(anchor.entity), Arc::new(anchor));
        Ok(())
    }
    pub(super) fn supply_portal_ids(
        &mut self,
        ids: &[EntityId],
    ) -> Result<(usize, usize), bace_gameplay_api::InventoryRejection> {
        use bace_gameplay_api::InventoryRejection as E;
        let capacity = self.portals.capacity.min(128);
        if ids.len() > 64 || ids.len() > capacity.saturating_sub(self.portals.ids.len()) {
            return Err(E::Capacity);
        }
        if ids.windows(2).any(|p| p[0] >= p[1])
            || ids.iter().any(|id| {
                id.0 == 0
                    || self.world.contains_identity(*id)
                    || self.population.reserves_identity(*id)
                    || self.generator_reserves_identity(*id)
                    || self.magic.reserves_identity(*id)
                    || self.combat.reserves_projectile(*id)
                    || self.pets.reserves_identity(*id)
                    || self.inventory.item(*id).is_some()
                    || self.portals.reserves_identity(*id)
            })
        {
            return Err(E::InvalidState);
        }
        self.portals.ids.extend(ids.iter().copied());
        Ok((self.portals.ids.len(), capacity))
    }
    pub fn supply_portal_id(&mut self, id: EntityId) -> Result<(), PortalError> {
        if id.0 == 0
            || self.world.contains_identity(id)
            || (self.population.reserves_identity(id) || self.generator_reserves_identity(id))
            || self.magic.reserves_identity(id)
            || self.combat.reserves_projectile(id)
            || self.pets.reserves_identity(id)
            || self.inventory.item(id).is_some()
            || self.portals.reserves_identity(id)
        {
            return Err(PortalError::Invalid);
        }
        if self.portals.ids.len() >= self.portals.capacity {
            return Err(PortalError::Capacity);
        }
        self.portals.ids.push_back(id);
        Ok(())
    }
    pub fn peek_portal_resolution(&self) -> Option<&crate::PortalResolutionOutcome> {
        self.portals.resolutions.front()
    }
    pub fn take_portal_resolution(&mut self) -> Option<crate::PortalResolutionOutcome> {
        self.portals.resolutions.pop_front()
    }
    pub fn take_portal_proposal(&mut self) -> Option<PortalServiceTicket> {
        let id = self.portals.proposals.pop_front()?;
        self.portals.pending.get(&id).map(|p| p.ticket.clone())
    }
    pub fn pending_portal_proposal(&self, operation: u64) -> Option<&PortalServiceTicket> {
        self.portals.pending.get(&operation).map(|p| &p.ticket)
    }
    pub fn retry_portal_proposal(&mut self, operation: u64) -> Result<(), PortalError> {
        let pending = self
            .portals
            .pending
            .get(&operation)
            .ok_or(PortalError::Invalid)?;
        if pending.committed || matches!(pending.ticket.origin, PortalServiceOrigin::Emote { .. }) {
            return Err(PortalError::Conflict);
        }
        if !self.portals.proposals.contains(&operation) {
            self.portals.proposals.push_back(operation);
        }
        Ok(())
    }
    pub fn take_portal_event(&mut self) -> Option<PortalServiceEvent> {
        self.portals.events.pop_front()
    }
    pub fn has_portal_state(&self) -> bool {
        self.portals.has_state()
    }
    pub fn portal_reserved(&self, actor: EntityId) -> bool {
        self.portals.reserved(actor)
    }
    pub fn portal_links(&self, actor: EntityId) -> Option<&PortalLinks> {
        self.portals.links.get(&actor)
    }
    pub(in crate::kernel) fn live_portal_access(
        &self,
        actor: EntityId,
    ) -> Result<PortalAccess, PortalError> {
        let mut access = *self
            .portals
            .access
            .get(&actor)
            .ok_or(PortalError::Invalid)?;
        if let Some(properties) = self.world.properties(actor) {
            if let Some(bace_entity::PropertyValue::Int(level)) =
                properties.get(bace_entity::PropertyFamily::Int, 25)
            {
                access.level = u32::try_from(*level).map_err(|_| PortalError::Invalid)?;
            }
            if let Some(bace_entity::PropertyValue::Int(pk)) =
                properties.get(bace_entity::PropertyFamily::Int, 134)
            {
                access.pk_status = u32::try_from(*pk).map_err(|_| PortalError::Invalid)?;
            }
        }
        if let Some(state) = self.player_deaths.states.get(&actor) {
            access.pk_status = state.pk_status;
        }
        access.teleporting = self.world.is_in_portal_transit(actor);
        access.pk_recent |= self.pk_timer_active(actor);
        access.vitae = self
            .magic
            .registry(actor)
            .is_some_and(|r| r.entries().iter().any(|e| e.spell == 666));
        Ok(access)
    }
}

impl Kernel {
    /// Called after the canonical NPC character-service receipt. No actor pose is
    /// copied back into this saved anchor and absent loaded projections are harmless.
    pub fn synchronize_portal_sanctuary(
        &mut self,
        actor: EntityId,
        position: Option<PortalPosition>,
    ) -> Result<(), PortalError> {
        if self.portals.reserved(actor) {
            return Err(PortalError::Conflict);
        }
        if let Some(links) = self.portals.links.get_mut(&actor) {
            links.synchronize_sanctuary(position)?;
        }
        Ok(())
    }
}
