//! Immutable per-death content/RNG context; ordinary loot never uses a player's
//! rare stream. Contexts are frozen before generation and retained for retry.
use super::*;
use bace_gameplay_api::{RareDecision, RareKillContext};
use bace_loot::{LootGraph, LootScratch, RareEvaluator};
use bace_random::RandomRoot;
use std::sync::Arc;
#[derive(Clone)]
pub struct NativeLootPolicy {
    pub graph: Arc<LootGraph>,
    pub graph_revision: [u8; 32],
    pub content_generation: [u8; 32],
    pub rare: Option<Arc<RareEvaluator>>,
    pub rare_profile_revision: Option<[u8; 32]>,
    pub creature_level: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NativeDeathLoot {
    /// Some identifies pinned ACE death-table output, not a native loot graph.
    pub source_items: Option<Vec<bace_content::WeenieV1>>,
    pub source_parents: Option<Vec<Option<usize>>>,
    pub event_id: [u8; 16],
    pub key_version: u32,
    pub graph_id: u32,
    pub graph_revision: [u8; 32],
    pub content_generation: [u8; 32],
    pub rare_profile_revision: Option<[u8; 32]>,
    pub generated: Vec<bace_loot::LootDrop>,
    pub rare: Option<RareDecision>,
}
#[derive(Clone)]
pub(super) struct Context {
    policy: NativeLootPolicy,
    pub(super) event: [u8; 16],
}
pub(super) struct NativeLoot {
    pub(super) root: Option<Arc<RandomRoot>>,
    pub(super) aces: BTreeMap<EntityId, super::ace::AceContext>,
    pub(super) epoch: Option<u64>,
    pub(super) contexts: BTreeMap<EntityId, Context>,
    respawns: BTreeMap<u32, NativeLootPolicy>,
    pub(super) events: BTreeMap<EntityId, [u8; 16]>,
    scratch: LootScratch,
}
impl NativeLoot {
    /// ACE exits the NoCorpse branch before GenerateTreasure for an Olthoi
    /// killer. Keep the durable event/content fence without drawing loot RNG or
    /// evaluating a rare award.
    pub(super) fn prepare_empty_no_corpse(
        &self,
        actor: EntityId,
    ) -> Result<Option<NativeDeathLoot>, PveError> {
        let Some(root) = &self.root else {
            return Ok(None);
        };
        let (event_id, graph_id, graph_revision, content_generation, rare_profile_revision) =
            if let Some(ctx) = self.aces.get(&actor) {
                (
                    ctx.event,
                    ctx.policy.profile_id,
                    ctx.policy.profile_revision,
                    ctx.policy.content_generation,
                    ctx.policy.rare_profile_revision,
                )
            } else if let Some(ctx) = self.contexts.get(&actor) {
                (
                    ctx.event,
                    ctx.policy.graph.id(),
                    ctx.policy.graph_revision,
                    ctx.policy.content_generation,
                    ctx.policy.rare_profile_revision,
                )
            } else {
                return Err(PveError::InvalidProfile);
            };
        Ok(Some(NativeDeathLoot {
            source_items: Some(vec![]),
            source_parents: Some(vec![]),
            event_id,
            key_version: root.key_version(),
            graph_id,
            graph_revision,
            content_generation,
            rare_profile_revision,
            generated: vec![],
            rare: None,
        }))
    }
    pub(super) fn enabled(&self) -> bool {
        self.root.is_some()
    }
    pub(super) fn new() -> Self {
        Self {
            root: None,
            aces: BTreeMap::new(),
            epoch: None,
            contexts: BTreeMap::new(),
            respawns: BTreeMap::new(),
            events: BTreeMap::new(),
            scratch: LootScratch::default(),
        }
    }
    pub(super) fn prepare(
        &mut self,
        actor: EntityId,
        owner: Option<EntityId>,
        world: &World,
        characters: &Characters,
        tick: u64,
    ) -> Result<Option<NativeDeathLoot>, PveError> {
        if self.aces.contains_key(&actor) {
            return self.prepare_ace(actor, owner, world, characters, tick);
        }
        let Some(context) = self.contexts.get(&actor) else {
            return if self.root.is_some() {
                Err(PveError::InvalidProfile)
            } else {
                Ok(None)
            };
        };
        let root = self.root.as_ref().ok_or(PveError::InvalidProfile)?;
        let mut generated = Vec::with_capacity(257);
        context
            .policy
            .graph
            .generate(root, context.event, &mut generated, &mut self.scratch)
            .map_err(|_| PveError::InvalidProfile)?;
        let rare = if let (Some(evaluator), Some(owner)) = (&context.policy.rare, owner) {
            if world.combatant(owner).is_some_and(|c| c.profile().player) {
                let state = characters.rare(owner).ok_or(PveError::MissingActor)?;
                let level = match world
                    .properties(owner)
                    .and_then(|p| p.get(bace_entity::PropertyFamily::Int, 25))
                {
                    Some(bace_entity::PropertyValue::Int(level)) => u32::try_from(*level)
                        .ok()
                        .filter(|v| *v > 0)
                        .ok_or(PveError::InvalidProfile)?,
                    _ => return Err(PveError::InvalidProfile),
                };
                let now = self
                    .epoch
                    .ok_or(PveError::InvalidProfile)?
                    .checked_add(tick / 30)
                    .ok_or(PveError::Overflow)?;
                let decision = evaluator
                    .evaluate(
                        root,
                        state,
                        RareKillContext {
                            character: owner.0,
                            player_level: level,
                            creature_level: context.policy.creature_level,
                            lootable_monster_death: true,
                            now_unix_seconds: now,
                        },
                    )
                    .map_err(|_| PveError::InvalidProfile)?;
                if let Some(award) = decision.award {
                    generated.push(bace_loot::LootDrop {
                        template: award.template,
                        stack: 1,
                        node: "$rare".into(),
                        mutations: Vec::new(),
                    });
                }
                Some(decision)
            } else {
                None
            }
        } else {
            None
        };
        Ok(Some(NativeDeathLoot {
            source_items: None,
            source_parents: None,
            event_id: context.event,
            key_version: root.key_version(),
            graph_id: context.policy.graph.id(),
            graph_revision: context.policy.graph_revision,
            content_generation: context.policy.content_generation,
            rare_profile_revision: context.policy.rare_profile_revision,
            generated,
            rare,
        }))
    }
    pub(super) fn retired(&mut self, source: EntityId) {
        self.contexts.remove(&source);
        self.aces.remove(&source);
        self.events.remove(&source);
    }
    pub(super) fn committed(&mut self, source: EntityId) {
        if let Some(context) = self.contexts.remove(&source) {
            self.respawns.insert(source.0, context.policy);
        }
    }
    pub(super) fn respawn_ready(&self, generator: u32, id: EntityId) -> bool {
        !self.respawns.contains_key(&generator) || self.events.contains_key(&id)
    }
    pub(super) fn respawned(&mut self, generator: u32, id: EntityId) {
        if let Some(policy) = self.respawns.remove(&generator) {
            let event = self.events.remove(&id).expect("reserved native event");
            self.contexts.insert(id, Context { policy, event });
        }
    }
}
impl Population {
    pub(crate) fn configure_native_loot(
        &mut self,
        root: Arc<RandomRoot>,
        epoch: u64,
    ) -> Result<(), PveError> {
        if !self.native.aces.is_empty()
            || !self.native.contexts.is_empty()
            || !self.native.respawns.is_empty()
            || !self.pending.is_empty()
            || !self.random.is_empty()
            || epoch > i64::MAX as u64
        {
            return Err(PveError::InvalidProfile);
        }
        self.native.root = Some(root);
        self.native.epoch = Some(epoch);
        Ok(())
    }
    pub(crate) fn native_loot(
        &mut self,
        actor: EntityId,
        policy: NativeLootPolicy,
        event: [u8; 16],
    ) -> Result<(), PveError> {
        if !self.npcs.contains_key(&actor)
            || self.native.root.is_none()
            || self.native.aces.contains_key(&actor)
            || self.native.contexts.contains_key(&actor)
            || self.pending.values().any(|p| p.proposal.victim == actor)
        {
            return Err(PveError::InvalidProfile);
        }
        if event == [0; 16]
            || policy.graph_revision == [0; 32]
            || policy.content_generation == [0; 32]
            || policy.creature_level == 0
            || policy.rare.is_some() != policy.rare_profile_revision.is_some()
            || policy.rare_profile_revision == Some([0; 32])
            || self.native.aces.values().any(|c| c.event == event)
            || self.native.contexts.values().any(|c| c.event == event)
            || self.native.events.values().any(|e| *e == event)
        {
            return Err(PveError::InvalidProfile);
        }
        self.native
            .contexts
            .insert(actor, Context { policy, event });
        Ok(())
    }
    pub(crate) fn native_spawn_id(
        &mut self,
        id: EntityId,
        event: [u8; 16],
        world: &World,
    ) -> Result<(), PveError> {
        if event == [0; 16]
            || self.native.contexts.values().any(|c| c.event == event)
            || self.native.events.values().any(|e| *e == event)
        {
            return Err(PveError::InvalidProfile);
        }
        if !self.ids.contains(&id) {
            self.feed_id(id, world)?;
        } else if self.native.events.contains_key(&id) {
            return Err(PveError::Duplicate);
        }
        self.native.events.insert(id, event);
        Ok(())
    }
}
