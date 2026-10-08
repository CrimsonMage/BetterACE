//! Indexed allegiance owner with one durable transition in flight; unrelated world work proceeds.
use bace_allegiance::{AllegianceClock, AllegianceCredit, AllegiancePatch, AllegianceRegistry};
use bace_gameplay_api::social::SocialError;
use bace_types::EntityId;
#[derive(Clone, Debug, PartialEq)]
pub struct AllegianceTicket {
    pub operation: u64,
    pub actor: EntityId,
    pub patch: AllegiancePatch,
    pub credits: Vec<AllegianceCredit>,
    pub player_changes: Vec<(EntityId, bace_character::EarnedExperienceChange)>,
    pub rare: Option<bace_gameplay_api::RareDecision>,
    pub npc: Option<crate::NpcProposal>,
    pub item_experience: Vec<(crate::ItemExperienceReward, Option<crate::InventoryTicket>)>,
    pub vitae: Vec<crate::PlayerVitaeRecovery>,
    pub vitals: Vec<bace_entity::VitalMutation>,
    pub quest_messages: Vec<(EntityId, u64)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CachedAllegianceSkills {
    pub level: u32,
    pub leadership: u32,
    pub loyalty: u32,
}
pub(crate) struct Allegiances {
    pub(crate) registry: AllegianceRegistry,
    pub(crate) clock: AllegianceClock,
    pub(crate) pending: Option<AllegianceTicket>,
    pub(crate) submitted: bool,
    pub(crate) next: u64,
    pub(crate) checkpoint: Option<f64>,
    pub(crate) cached_skills: std::collections::BTreeMap<EntityId, CachedAllegianceSkills>,
    pub(crate) cache_operation: Option<u64>,
    pub(crate) ids: std::collections::VecDeque<u32>,
    pub(crate) capacity: usize,
    pub(crate) context: Option<bace_gameplay_api::ActionContext>,
    pub(crate) listeners: std::collections::BTreeSet<EntityId>,
    pub(crate) level_table: Option<std::sync::Arc<bace_character::CharacterLevelTable>>,
    pub(crate) registries: Vec<EntityId>,
    pub(crate) pve_operation: Option<u64>,
    pub(crate) experience_events:
        std::collections::VecDeque<bace_gameplay_api::experience::ExperienceEvent>,
}
impl Allegiances {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            registry: AllegianceRegistry::new(capacity.max(1)).expect("bounded kernel capacity"),
            clock: AllegianceClock::new(0.0).expect("zero clock"),
            pending: None,
            submitted: false,
            next: 0,
            checkpoint: None,
            cached_skills: Default::default(),
            cache_operation: None,
            ids: Default::default(),
            capacity,
            context: None,
            listeners: Default::default(),
            level_table: None,
            registries: Vec::new(),
            pve_operation: None,
            experience_events: Default::default(),
        }
    }
    pub(crate) fn propose(
        &mut self,
        actor: EntityId,
        patch: AllegiancePatch,
        credits: Vec<AllegianceCredit>,
    ) -> Result<AllegianceTicket, SocialError> {
        if self.pending.is_some() {
            return Err(SocialError::Busy);
        }
        self.registry.validate_patch(&patch)?;
        let operation = self.next.checked_add(1).ok_or(SocialError::Overflow)?;
        let ticket = AllegianceTicket {
            operation,
            actor,
            patch,
            credits,
            player_changes: Vec::new(),
            rare: None,
            npc: None,
            item_experience: Vec::new(),
            vitae: Vec::new(),
            vitals: Vec::new(),
            quest_messages: Vec::new(),
        };
        self.next = operation;
        self.pending = Some(ticket.clone());
        self.submitted = false;
        Ok(ticket)
    }
    pub(crate) fn take(&mut self) -> Option<AllegianceTicket> {
        if self.submitted || self.pve_operation.is_some() {
            return None;
        }
        let ticket = self.pending.clone()?;
        self.submitted = true;
        Some(ticket)
    }
    pub(crate) fn has_state(&self) -> bool {
        self.pending.is_some()
            || !self.cached_skills.is_empty()
            || self.registry.nodes().next().is_some()
            || !self.experience_events.is_empty()
    }
    pub(crate) fn validate_receipt(&self, ticket: &AllegianceTicket) -> Result<(), SocialError> {
        if !self.submitted || self.pending.as_ref() != Some(ticket) {
            return Err(SocialError::Stale);
        }
        self.registry.validate_patch(&ticket.patch)
    }
    pub(crate) fn adopt(&mut self, ticket: &AllegianceTicket) -> Result<(), SocialError> {
        self.validate_receipt(ticket)?;
        if let Some(now) = self.checkpoint {
            self.clock.due(now)?.ok_or(SocialError::Invalid)?;
        }
        self.registry.adopt(ticket.patch.clone())?;
        if let Some(now) = self.checkpoint.take() {
            self.clock.adopt(now)?;
        }
        if self.cache_operation == Some(ticket.operation) {
            for (_, after) in &ticket.patch.nodes {
                if let Some(after) = after {
                    let value = CachedAllegianceSkills {
                        level: after.level,
                        leadership: after.leadership,
                        loyalty: after.loyalty,
                    };
                    if self.cached_skills.get(&after.character) == Some(&value) {
                        self.cached_skills.remove(&after.character);
                    }
                }
            }
            self.cache_operation = None;
        }
        self.pending = None;
        self.submitted = false;
        Ok(())
    }
}

impl AllegianceTicket {
    pub fn player_revision(&self, actor: EntityId) -> Option<u64> {
        let c = &self.player_changes.iter().find(|(id, _)| *id == actor)?.1;
        let auxiliary = self
            .rare
            .as_ref()
            .is_some_and(|r| r.character == actor.0 && r.previous != r.next)
            || self.vitae.iter().any(|p| {
                p.actor == actor
                    && (p.before != p.after
                        || p.registry.before_revision() != p.registry.after_revision())
            })
            || self.item_experience.iter().any(|(r, _)| {
                r.registries
                    .iter()
                    .any(|p| p.actor == actor && p.before != p.after)
            });
        if auxiliary && c.experience.before_revision == c.experience.after_revision {
            c.experience.after_revision.checked_add(1)
        } else {
            Some(c.experience.after_revision)
        }
    }
}
