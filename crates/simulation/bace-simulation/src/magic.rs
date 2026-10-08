//! Single-owner magic integration over admitted prepared spells and accepted world
//! state. Synthetic same-cell geometry remains explicitly separate from stock
//! client / full AC transition qualification.
mod admission;
mod damage;
mod direct;
mod equipment_admission;
mod health_observations;
#[cfg(test)]
mod physical_proc_tests;
mod physical_procs;
mod pk_activity;
mod procs;
pub(crate) use procs::MagicItemProcRequest;
mod actor_program;
mod casts;
mod components;
mod content;
mod server_casting;
pub use actor_program::PreparedActorMagicProgram;
pub use content::{PreparedMagicAssetBatch, PreparedMagicDefinition};
mod generator_enchantments;
mod item_experience;
mod item_targets;
mod player_death;
mod residency;
mod staff;
mod style_entry;
pub use components::MagicResourceCommit;
mod effects;
mod fellowship;
mod monsters;
mod motion;
mod npc_registry_restore;
#[cfg(test)]
mod npc_registry_restore_tests;
mod npc_retirement;
mod peace;
mod termination;
use peace::accepted_peace_style;
use termination::TerminalCast;
#[cfg(test)]
mod object_caster_tests;
mod object_casters;
mod origins;
mod resist_notices;
pub use resist_notices::MagicResistNotice;
mod preparation;
mod recovery;
use monsters::restore_monster_mode;
use origins::origin_identity;
mod validation;
mod vitae;
#[cfg(test)]
mod vitae_tests;
use validation::*;
mod periodic;
mod periodic_native;
pub use periodic::PeriodicDefenseProfile;
pub(crate) mod portals;
mod projectile_launch;
mod projectiles;
mod registry;
mod specializations;
pub use specializations::MagicDefenseProfile;
#[cfg(test)]
mod damage_refresh_tests;
#[cfg(test)]
mod periodic_native_tests;
#[cfg(test)]
mod periodic_tests;
#[cfg(test)]
mod projectile_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod specialization_tests;
#[cfg(test)]
mod target_geometry_tests;
use crate::combat::{Combat, CombatEvent};
use bace_entity::{EntityVital, VitalMutation};
use bace_gameplay_api::{
    ActionContext, ActionResult, CastChange, CastOrigin, CastOutcome, CastRejection, CastRequest,
    ServerCastOutcome,
};
use bace_geometry::Vec3;
use bace_magic::{
    CastDriver, CastError, CastGesture, CastObservation, CastPreparation, CastSignal, DispelSpec,
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, PreparedSpell, ProjectileSpec,
    RegistryError, SpellEffect, Vital, VitalState,
};
use bace_motion::{TurnControl, TurnIntent, heading_delta};
use bace_physics::{ProjectileBody, ProjectileStep};
use bace_random::{Domain, RandomRoot, RandomStream};
use bace_types::{CellId, EntityId};
use bace_world::{OwnedProjectile, World};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedCastGesture {
    pub gesture: CastGesture,
    /// Only used by explicitly enabled synthetic fixtures. Production execution
    /// completes from the World-owned prepared DAT motion chain.
    pub duration_seconds: f64,
    pub motion_chain: Option<Arc<bace_motion::PreparedMotionChain>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedMagicSpell {
    pub spell: PreparedSpell,
    pub gestures: Vec<PreparedCastGesture>,
    pub components: Vec<(u32, u32)>,
    /// Per-WCID destruction modifiers resolved from the component table.
    pub component_modifiers: Vec<(u32, f32)>,
    pub component_loss: f32,
    pub fast_resistable_pk_spell: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MagicCaster {
    pub player: bool,
    pub known_spells: BTreeSet<u32>,
    pub school_skills: [u32; 5],
    pub magic_defense: u32,
    pub mana_conversion: u32,
    pub components_required: bool,
    pub safe_components: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MagicEvent {
    /// Source Remove semantics; recipient/name are frozen at accepted removal.
    EnchantmentExpired {
        actor: EntityId,
        recipient: EntityId,
        spell: u32,
        layer: u16,
        item_name: Option<String>,
        sound: bool,
    },
    Motion {
        actor: EntityId,
        cast: u64,
        sequence: u64,
        motion: u32,
        speed: f32,
    },
    Turning {
        actor: EntityId,
        cast: u64,
        target: EntityId,
    },
    Vital {
        actor: EntityId,
        incarnation: u64,
        vital: EntityVital,
        before: u32,
        after: u32,
        revision: u64,
    },
    Enchantment {
        actor: EntityId,
        entry: EnchantmentEntry,
    },
    EnchantmentsRemoved {
        actor: EntityId,
        entries: Vec<(u32, u16)>,
    },
    ProjectileCreated {
        actor: EntityId,
        launch: Option<Arc<bace_gameplay_api::visibility::AcceptedProjectileLaunch>>,
        effect_intensity: f32,
        source: EntityId,
        spell: u32,
        template: u32,
        position: Vec3,
        velocity: Vec3,
    },
    ProjectileExploded {
        actor: EntityId,
        tick: u64,
        spell: u32,
    },
    ProjectileRemoved {
        actor: EntityId,
        tick: u64,
    },
    /// Accepted interpreter state after the source forced stop. This is a
    /// substate projection, never an action-list command.
    MotionStopped {
        actor: EntityId,
        cast: u64,
        sequence: u64,
        style: u32,
        substate: u32,
        speed: f32,
    },
    Fizzle {
        actor: EntityId,
        intensity: f32,
        movement_incarnation: Option<u64>,
    },
    TargetRejected {
        actor: EntityId,
        target: EntityId,
        spell: u32,
        reason: CastRejection,
        notice: Option<Arc<MagicResistNotice>>,
    },
    MotionHook {
        actor: EntityId,
        cast: u64,
        sequence: u64,
        animation: u32,
        frame: u32,
        kind: u32,
        payload: bace_motion::MotionHookPayload,
    },
    /// Remains pending until the owning inventory service confirms consumption.
    ComponentsRequired {
        cast: u64,
        actor: EntityId,
        requirements: Vec<(u32, u32)>,
        consumed: Vec<(u32, u32)>,
    },
    /// World/content owners implement portal link/recall/summon before confirming.
    PortalRequired {
        cast: u64,
        actor: EntityId,
        target: EntityId,
        effect: bace_magic::PortalEffect,
    },
}
struct Motion {
    sequence: u64,
    motion: u32,
    due: Option<f64>,
    epoch: u16,
    completion: Option<bool>,
}
struct Attempt {
    item_target: Option<item_targets::ItemSpellTarget>,
    cast_skill: u32,
    pending_direct: Option<direct::PendingDirectDamage>,
    style_entry: Option<style_entry::StyleEntry>,
    motion_sequence_offset: u64,
    origin: CastOrigin,
    origin_epoch: u16,
    initial_cast: (CellId, Vec3),
    target: Option<EntityId>,
    prepared: Arc<PreparedMagicSpell>,
    driver: CastDriver,
    random: RandomStream,
    resources: Option<ResourcePlan>,
    motion: Option<Motion>,
    turn_target: Option<EntityId>,
    turn_control: Option<TurnControl>,
    cast: u64,
    components_confirmed: bool,
    component_request_sent: bool,
    component_failure: Option<CastRejection>,
    mana_applied: bool,
    portal_request_sent: bool,
    portal_operation: Option<u64>,
    portal_completion: Option<Result<(), CastRejection>>,
    previous_mode: Option<u32>,
    terminal: Option<TerminalCast>,
    peace_fizzle: Option<(u32, f32, bool)>,
}
struct ResourcePlan {
    cost: u32,
    fizzled: bool,
    school_locked: bool,
    consumed: Vec<(u32, u32)>,
    random: RandomStream,
}
struct PendingProjectileDamage {
    apply_damage: bool,
    target: EntityId,
    damage: u32,
    vital: EntityVital,
    cloak_wait: Option<u64>,
    sigils: Vec<bace_magic::MagicItemProc>,
}
struct Flying {
    proc_parent: Option<(CastOrigin, u8)>,
    pending_damage: Option<PendingProjectileDamage>,
    launch_wand: Option<bace_magic::MagicWand>,
    source: EntityId,
    target: Option<EntityId>,
    spell: Arc<PreparedMagicSpell>,
    random: RandomStream,
    lifetime: bace_magic::SpellProjectileLifetime,
    initial_cast: (CellId, Vec3),
    maximum_range: f32,
    cast_skill: u32,
    resting: bool,
    pending_impact: Option<Option<u32>>,
    life_damage: Option<f32>,
    credited_owner: EntityId,
}
pub(crate) struct Magic {
    spells: BTreeMap<u32, Arc<PreparedMagicSpell>>,
    projectile_shapes: BTreeMap<u32, Arc<bace_physics::CollisionShape>>,
    casters: BTreeMap<EntityId, MagicCaster>,
    object_casters: BTreeMap<EntityId, Arc<bace_content::WeenieV1>>,
    server_mana: BTreeMap<EntityId, bool>,
    spell_categories: BTreeMap<u32, u32>,
    recovery: BTreeMap<EntityId, bace_magic::CastRecoveryClock>,
    component_operations: BTreeMap<u64, MagicResourceCommit>,
    actor_components: BTreeMap<(EntityId, u32), Arc<PreparedMagicSpell>>,
    actor_program_spells: BTreeSet<u32>,
    validated_actor_programs: BTreeSet<(EntityId, u32)>,
    vitae_template: Option<EnchantmentEntry>,
    vitae_sequences: BTreeMap<EntityId, u64>,
    vitae_removals: BTreeMap<EntityId, (f64, u64)>,
    monster_ai: BTreeMap<EntityId, (bace_ai::MonsterSpellcasting, u64)>,
    defense_profiles: BTreeMap<EntityId, MagicDefenseProfile>,
    damage_profiles: BTreeMap<EntityId, bace_magic::MagicDamageProfile>,
    damage_wands: BTreeMap<u32, (bace_magic::MagicWand, Option<EntityId>)>,
    damage_spell_levels: BTreeMap<u32, u32>,
    damage_spell_flags: BTreeMap<u32, u32>,
    physical_procs: BTreeMap<
        bace_gameplay_api::physical_procs::PhysicalHitKey,
        physical_procs::PendingPhysicalProc,
    >,
    item_procs: VecDeque<MagicItemProcRequest>,
    next_item_proc: u64,
    completed_item_procs: BTreeSet<u64>,
    claimed_emote_outcomes: BTreeSet<(EntityId, u64)>,
    item_types: BTreeMap<EntityId, u32>,
    item_spell_qualities: BTreeMap<EntityId, (u32, bool)>,
    spell_target_masks: BTreeMap<u32, u32>,
    item_targets: BTreeMap<EntityId, Option<item_targets::ItemSpellTarget>>,
    attempts: BTreeMap<EntityId, Attempt>,
    instant_continuations: BTreeMap<u64, Attempt>,
    actor_scratch: Vec<EntityId>,
    projectile_scratch: Vec<EntityId>,
    registries: BTreeMap<EntityId, EnchantmentRegistry>,
    registry_clocks: BTreeMap<EntityId, registry::RegistryClock>,
    enchantment_metadata: BTreeMap<u32, EnchantmentMetadata>,
    heartbeat_scratch: Vec<(u32, u16)>,
    current_time: f64,
    event_tick: u64,
    periodic: VecDeque<periodic::PeriodicPulse>,
    periodic_profiles: BTreeMap<EntityId, PeriodicDefenseProfile>,
    flying: BTreeMap<EntityId, Flying>,
    ids: VecDeque<EntityId>,
    events: VecDeque<MagicEvent>,
    outcomes: VecDeque<CastOutcome>,
    server_outcomes: VecDeque<ServerCastOutcome>,
    server_sequences: BTreeMap<(EntityId, u8), u64>,
    combat: VecDeque<CombatEvent>,
    capacity: usize,
    next_cast: u64,
    execution_epoch: u64,
    synthetic_cast_timing: bool,
    random: Option<Arc<RandomRoot>>,
}
impl Magic {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            spells: BTreeMap::new(),
            projectile_shapes: BTreeMap::new(),
            casters: BTreeMap::new(),
            object_casters: BTreeMap::new(),
            server_mana: BTreeMap::new(),
            spell_categories: BTreeMap::new(),
            recovery: BTreeMap::new(),
            component_operations: BTreeMap::new(),
            actor_components: BTreeMap::new(),
            actor_program_spells: BTreeSet::new(),
            validated_actor_programs: BTreeSet::new(),
            monster_ai: BTreeMap::new(),
            vitae_template: None,
            vitae_sequences: BTreeMap::new(),
            vitae_removals: BTreeMap::new(),
            defense_profiles: BTreeMap::new(),
            damage_profiles: BTreeMap::new(),
            damage_wands: BTreeMap::new(),
            damage_spell_levels: BTreeMap::new(),
            damage_spell_flags: BTreeMap::new(),
            item_procs: VecDeque::with_capacity(capacity.max(4) + 16),
            physical_procs: BTreeMap::new(),
            next_item_proc: 0,
            completed_item_procs: BTreeSet::new(),
            claimed_emote_outcomes: BTreeSet::new(),
            item_types: BTreeMap::new(),
            item_spell_qualities: BTreeMap::new(),
            spell_target_masks: BTreeMap::new(),
            item_targets: BTreeMap::new(),
            attempts: BTreeMap::new(),
            instant_continuations: BTreeMap::new(),
            actor_scratch: Vec::new(),
            projectile_scratch: Vec::new(),
            registries: BTreeMap::new(),
            registry_clocks: BTreeMap::new(),
            enchantment_metadata: BTreeMap::new(),
            heartbeat_scratch: Vec::with_capacity(4096),
            current_time: 0.0,
            event_tick: 0,
            periodic: VecDeque::with_capacity(capacity),
            periodic_profiles: BTreeMap::new(),
            flying: BTreeMap::new(),
            ids: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            outcomes: VecDeque::with_capacity(capacity),
            server_outcomes: VecDeque::with_capacity(capacity),
            server_sequences: BTreeMap::new(),
            combat: VecDeque::with_capacity(capacity),
            capacity,
            next_cast: 0,
            execution_epoch: 0,
            synthetic_cast_timing: false,
            random: None,
        }
    }
    pub(crate) fn configure_random(&mut self, root: RandomRoot) -> Result<(), CastRejection> {
        self.configure_random_shared(Arc::new(root))
    }
    pub(crate) fn configure_random_shared(
        &mut self,
        root: Arc<RandomRoot>,
    ) -> Result<(), CastRejection> {
        if !self.attempts.is_empty() || !self.flying.is_empty() {
            return Err(CastRejection::Busy);
        }
        self.random = Some(root);
        Ok(())
    }

    pub(crate) fn required_components(&self, actor: EntityId, spell: u32) -> Option<&[(u32, u32)]> {
        if self.actor_program_spells.contains(&spell)
            && !self.validated_actor_programs.contains(&(actor, spell))
        {
            return None;
        }
        if !self.casters.get(&actor)?.components_required {
            return Some(&[]);
        }
        Some(
            self.actor_components
                .get(&(actor, spell))
                .or_else(|| self.spells.get(&spell))?
                .components
                .as_slice(),
        )
    }
    pub(crate) fn reserves_identity(&self, id: EntityId) -> bool {
        self.ids.contains(&id) || self.flying.contains_key(&id)
    }
    pub(crate) fn projectile_id_capacity(&self) -> (usize, usize) {
        (self.ids.len(), self.capacity)
    }
    pub(crate) fn preflight_projectile_ids(
        &self,
        ids: &[EntityId],
        world: &World,
    ) -> Result<(), CastRejection> {
        if ids.len() > 64 || ids.len() > self.capacity.saturating_sub(self.ids.len()) {
            return Err(CastRejection::Capacity);
        }
        if ids.windows(2).any(|p| p[0] >= p[1])
            || ids
                .iter()
                .any(|id| id.0 == 0 || world.contains_identity(*id) || self.reserves_identity(*id))
        {
            return Err(CastRejection::InvalidState);
        }
        Ok(())
    }
    pub(crate) fn supply_projectile_id(
        &mut self,
        id: EntityId,
        world: &World,
    ) -> Result<(), CastRejection> {
        if id.0 == 0 || world.contains_identity(id) || self.reserves_identity(id) {
            return Err(CastRejection::InvalidState);
        }
        if self.ids.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        self.ids.push_back(id);
        Ok(())
    }
    pub(crate) fn can_accept(&self) -> bool {
        self.events.len() < self.capacity
            && self.outcomes.len() < self.capacity
            && self.server_outcomes.len() < self.capacity
            && !self.registry_clocks.values().any(|clock| {
                clock.error.is_some()
                    || clock.active && !clock.reserved && clock.next_due <= self.current_time
            })
    }
    pub(crate) fn take_event(&mut self) -> Option<MagicEvent> {
        self.events.pop_front()
    }
    pub(crate) fn take_outcome(&mut self) -> Option<CastOutcome> {
        self.outcomes.pop_front()
    }
    pub(crate) fn take_combat_event(&mut self) -> Option<CombatEvent> {
        self.combat.pop_front()
    }
    pub(crate) fn pending_events(&self) -> usize {
        self.events.len()
    }
    pub(crate) fn pending_outcomes(&self) -> usize {
        self.outcomes.len()
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.attempts.is_empty()
            || !self.periodic.is_empty()
            || !self.vitae_removals.is_empty()
            || !self.events.is_empty()
            || !self.outcomes.is_empty()
            || !self.server_outcomes.is_empty()
            || !self.physical_procs.is_empty()
            || !self.item_procs.is_empty()
            || !self.instant_continuations.is_empty()
            || !self.combat.is_empty()
            || !self.flying.is_empty()
            || !self.registries.is_empty()
    }
    pub(crate) fn busy(&self, actor: EntityId) -> bool {
        self.attempts.contains_key(&actor)
    }
    /// Durable-stage/output ownership guard, separate from GDLE windup mode
    /// policy. A paid or committed operation cannot be replaced before delivery.
    pub(crate) fn mode_change_reserved(&self, actor: EntityId) -> bool {
        self.attempts.get(&actor).is_some_and(|attempt| {
            attempt.mana_applied
                || attempt.pending_direct.is_some()
                || attempt.terminal.is_some()
                || attempt.peace_fizzle.is_some()
                || attempt.portal_operation.is_some()
        })
    }
    pub(crate) fn registry(&self, actor: EntityId) -> Option<&EnchantmentRegistry> {
        self.registries.get(&actor)
    }

    fn signal(
        &mut self,
        attempt: &mut Attempt,
        signal: CastSignal,
        world: &mut World,
        now: f64,
    ) -> Result<(), CastRejection> {
        match signal {
            CastSignal::Gesture {
                motion,
                index,
                sequence,
                speed,
                stop_movement,
                ..
            } => {
                let sequence = sequence
                    .checked_add(attempt.motion_sequence_offset)
                    .ok_or(CastRejection::InvalidState)?;
                let prepared = attempt
                    .prepared
                    .gestures
                    .get(index)
                    .filter(|g| g.gesture.motion == motion)
                    .ok_or(CastRejection::MissingAssets)?;
                let actor = attempt.origin.actor();
                let epoch = world
                    .body(actor)
                    .map_err(|_| CastRejection::MissingActor)?
                    .accepted()
                    .epoch();
                let due = if let Some(chain) = &prepared.motion_chain {
                    let token = bace_motion::MotionToken {
                        domain: bace_motion::MotionDomain::Casting,
                        owner: attempt.cast,
                        sequence,
                    };
                    world
                        .begin_motion(actor, token, chain.clone())
                        .map_err(|_| CastRejection::MissingAssets)?;
                    None
                } else {
                    if !self.synthetic_cast_timing
                        || world
                            .body(actor)
                            .is_ok_and(|body| body.collision_shape().is_some())
                    {
                        return Err(CastRejection::MissingAssets);
                    }
                    Some(now + prepared.duration_seconds)
                };
                if stop_movement {
                    world
                        .body_mut(actor)
                        .map_err(|_| CastRejection::MissingActor)?
                        .stop_motion();
                }
                attempt.turn_target = None;
                attempt.motion = Some(Motion {
                    sequence,
                    motion,
                    due,
                    epoch,
                    completion: None,
                });
                self.events.push_back(MagicEvent::Motion {
                    actor: attempt.origin.actor(),
                    cast: attempt.cast,
                    sequence,
                    motion,
                    speed,
                });
            }
            CastSignal::Turn { target, .. } => {
                if !world
                    .body(attempt.origin.actor())
                    .is_ok_and(|body| body.maximum_turn_rate() > 0.0)
                {
                    return Err(CastRejection::MissingAssets);
                }
                attempt.turn_target = Some(EntityId(target));
                let (cell, accepted) = world
                    .actor_state(attempt.origin.actor())
                    .map_err(|_| CastRejection::MissingActor)?;
                let (target_position, _) = world
                    .actor_in_frame(EntityId(target), cell)
                    .map_err(|_| CastRejection::InvalidTarget)?;
                let offset = target_position - accepted.position();
                let delta = heading_delta(accepted.heading_radians(), (-offset.x).atan2(offset.y))
                    .map_err(|_| CastRejection::InvalidState)?;
                let control = world
                    .next_server_control(attempt.origin.actor())
                    .map_err(|_| CastRejection::InvalidState)?;
                world
                    .body_mut(attempt.origin.actor())
                    .map_err(|_| CastRejection::MissingActor)?
                    .begin_server_turn(
                        accepted.epoch(),
                        control,
                        TurnIntent::new(delta.signum()).map_err(|_| CastRejection::InvalidState)?,
                    )
                    .map_err(|_| CastRejection::InvalidState)?;
                // GDLE BeginNextMotion: default MovementParameters with
                // modify_interpreted_state cleared; the accepted control owner
                // supplies renderer evidence for the same physical turn.
                world
                    .bind_movement_goal(
                        attempt.origin.actor(),
                        control,
                        bace_gameplay_api::visibility::ServerMovementGoal::TurnToObject {
                            target: EntityId(target),
                            parameters: bace_gameplay_api::visibility::AcceptedTurnParameters {
                                flags: 0x1F09AE0F,
                                speed: 1.0,
                                desired_heading: 0.0,
                            },
                        },
                    )
                    .map_err(|_| CastRejection::InvalidState)?;
                attempt.turn_control = Some(control);
                self.events.push_back(MagicEvent::Turning {
                    actor: attempt.origin.actor(),
                    cast: attempt.cast,
                    target: EntityId(target),
                });
            }
            CastSignal::PeaceFizzle {
                mana_cost,
                intensity,
                ..
            } => {
                attempt.peace_fizzle = Some((mana_cost, intensity, false));
                self.flush_peace_fizzle(attempt, world, now)?;
            }
            CastSignal::MovementFizzle {
                mana_cost,
                intensity,
                ..
            } => {
                attempt.peace_fizzle = Some((mana_cost, intensity, true));
                self.flush_peace_fizzle(attempt, world, now)?;
            }
            CastSignal::Finished { error, .. } => {
                let result = error.map_or(Ok(CastChange::Completed { cast: attempt.cast }), |e| {
                    Err(rejection(e))
                });
                self.finish_attempt(attempt, result, world, now)?;
            }
            CastSignal::Release { .. } | CastSignal::Waiting => {}
        }
        Ok(())
    }
    pub(crate) fn confirm_components(
        &mut self,
        cast: u64,
        actor: EntityId,
        success: bool,
    ) -> Result<(), CastRejection> {
        if self
            .component_operations
            .values()
            .any(|binding| binding.actor == actor && binding.cast == cast)
        {
            return Err(CastRejection::Busy);
        }
        let attempt = self
            .attempts
            .get_mut(&actor)
            .filter(|a| a.cast == cast && a.component_request_sent)
            .ok_or(CastRejection::InvalidState)?;
        attempt.component_failure = (!success).then_some(CastRejection::MissingComponents);
        attempt.components_confirmed = true;
        Ok(())
    }
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        now: f64,
        policy: &Combat,
        fellowships: &crate::fellowships::Fellowships,
        observers: Option<(&crate::characters::Characters, u64)>,
    ) {
        if let Some((_, tick)) = observers {
            self.event_tick = tick;
        }
        self.drain_motion_events(world);
        self.service_vitae_removals(now);
        self.heartbeat(now);
        self.drain_periodic(world, policy);
        // Preserve stable BTreeMap order while reusing the owner's high-water
        // scratch allocation. At most the admitted 4096 caster identities enter it.
        let mut ids = std::mem::take(&mut self.actor_scratch);
        ids.clear();
        ids.extend(self.attempts.keys().copied());
        for actor in ids.iter().copied() {
            if !self.can_accept() {
                break;
            }
            let Some(mut attempt) = self.attempts.remove(&actor) else {
                continue;
            };
            if attempt.terminal.is_some() {
                self.flush_terminal(&mut attempt, world);
                if attempt.terminal.is_some() {
                    self.attempts.insert(actor, attempt);
                }
                continue;
            }
            if attempt.peace_fizzle.is_some() {
                let _ = self.flush_peace_fizzle(&mut attempt, world, now);
                if attempt.peace_fizzle.is_some() || attempt.terminal.is_some() {
                    self.attempts.insert(actor, attempt);
                }
                continue;
            }
            if attempt.style_entry.is_some() {
                match self.advance_style_entry(&mut attempt, world, now) {
                    Ok(false) => {
                        self.attempts.insert(actor, attempt);
                        continue;
                    }
                    Ok(true) => {}
                    Err(error) => {
                        attempt.driver.cancel();
                        let _ = self.finish_attempt(&mut attempt, Err(error), world, now);
                        if attempt.terminal.is_some() {
                            self.attempts.insert(actor, attempt);
                        }
                        continue;
                    }
                }
            }
            if (attempt.component_request_sent && !attempt.components_confirmed)
                || attempt.portal_request_sent
            {
                self.attempts.insert(actor, attempt);
                continue;
            }
            if let Some(reason) = attempt.component_failure {
                attempt.driver.cancel();
                let _ = self.finish_attempt(&mut attempt, Err(reason), world, now);
                if attempt.terminal.is_some() {
                    self.attempts.insert(actor, attempt);
                }
                continue;
            }
            if attempt.pending_direct.is_some() {
                match self.release(&mut attempt, world, now, policy, fellowships, observers) {
                    Ok(true) => {
                        if let Ok(signal) =
                            attempt.driver.resolve_release(attempt.cast, now, Ok(()))
                        {
                            let _ = self.signal(&mut attempt, signal, world, now);
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        if let Some(pending) = attempt.pending_direct.take()
                            && let Some(wait) = pending.cloak_wait
                        {
                            self.completed_item_procs.remove(&wait);
                        }
                        attempt.driver.cancel();
                        let _ = self.finish_attempt(&mut attempt, Err(error), world, now);
                    }
                }
                if attempt.pending_direct.is_some() || attempt.terminal.is_some() {
                    self.attempts.insert(actor, attempt);
                }
                continue;
            }
            for _ in 0..32 {
                if !self.can_accept() {
                    break;
                }
                // Only a popped World completion can authorize a reentrant
                // zero-count callback. Fixture timers never create this permit.
                let completed = attempt
                    .motion
                    .as_ref()
                    .filter(|m| m.due.is_none() && m.completion == Some(true))
                    .map(|m| {
                        (
                            bace_motion::MotionToken {
                                domain: bace_motion::MotionDomain::Casting,
                                owner: attempt.cast,
                                sequence: m.sequence,
                            },
                            m.epoch,
                        )
                    });
                let skill = attempt.cast_skill;
                let target = self.observed_target(&attempt, world);
                // GDLE update_object delivers HandleMotionDone before Tick/Update.
                // A completed final action may release normally before a later peace
                // Update can run; don't add a mode check to that source callback.
                let callback_due = attempt.motion.as_ref().is_some_and(|motion| {
                    motion.completion.is_some() || motion.due.is_some_and(|due| now >= due)
                });
                let peace = !callback_due
                    && !attempt.origin.instant()
                    && matches!(attempt.origin, CastOrigin::Player(_))
                    && accepted_peace_style(world, actor);
                let observed = target.and_then(|target| {
                    observe(
                        world,
                        actor,
                        if peace { None } else { target },
                        &attempt.prepared.spell,
                        skill,
                    )
                });
                let result = match observed {
                    Err(error) => Err(error),
                    Ok(mut observation) => {
                        if attempt.origin.instant() {
                            observation.peace_mode = false;
                        }
                        if let (Some(target), Some(control)) =
                            (attempt.turn_target, attempt.turn_control)
                            && let Ok((cell, state)) = world.actor_state(actor)
                            && let Ok((target_position, _)) = world.actor_in_frame(target, cell)
                        {
                            let still_owned = world
                                .body(actor)
                                .is_ok_and(|body| body.server_turn() == Some(control));
                            let offset = target_position - state.position();
                            let desired = (-offset.x).atan2(offset.y);
                            let delta =
                                heading_delta(state.heading_radians(), desired).unwrap_or(0.0);
                            if !still_owned {
                                attempt.turn_target = None;
                                attempt.turn_control = None;
                            } else if delta.abs() <= 0.02 {
                                if let Ok(body) = world.body_mut(actor) {
                                    body.finish_server_turn(control);
                                }
                                attempt.turn_target = None;
                                attempt.turn_control = None;
                                observation.turning_to_target = false;
                            } else if let Ok(body) = world.body_mut(actor) {
                                let _ = body.continue_server_turn(
                                    state.epoch(),
                                    control,
                                    TurnIntent::new(delta.signum()).expect("finite signed delta"),
                                );
                                observation.turning_to_target = true;
                            }
                        }
                        let signal = if peace {
                            attempt.driver.update(now, observation).map_err(rejection)
                        } else if let Some(motion) = &attempt.motion
                            && (motion.completion.is_some()
                                || motion.due.is_some_and(|due| now >= due))
                        {
                            let signal = attempt
                                .driver
                                .motion_done(
                                    attempt.cast,
                                    motion.sequence - attempt.motion_sequence_offset,
                                    motion.motion,
                                    motion.completion.unwrap_or(true),
                                    now,
                                    observation,
                                )
                                .map_err(rejection);
                            attempt.motion = None;
                            signal
                        } else {
                            attempt.driver.update(now, observation).map_err(rejection)
                        };
                        signal.and_then(|signal| self.signal(&mut attempt, signal, world, now))
                    }
                };
                if let Err(error) = result {
                    attempt.driver.cancel();
                    let _ = self.finish_attempt(&mut attempt, Err(error), world, now);
                } else if attempt.driver.stage() == bace_magic::CastStage::Release {
                    match self.release(&mut attempt, world, now, policy, fellowships, observers) {
                        Ok(true) => {
                            if let Ok(signal) =
                                attempt.driver.resolve_release(attempt.cast, now, Ok(()))
                            {
                                let _ = self.signal(&mut attempt, signal, world, now);
                            }
                        }
                        Ok(false) => {}
                        Err(error) => {
                            attempt.driver.cancel();
                            let _ = self.finish_attempt(&mut attempt, Err(error), world, now);
                        }
                    }
                }
                let Some((token, epoch)) = completed else {
                    break;
                };
                if world.drain_motion_callbacks(actor, token, epoch).is_err() {
                    break;
                }
                self.drain_motion_events_for(world, Some(&mut attempt));
                if !attempt
                    .motion
                    .as_ref()
                    .is_some_and(|m| m.due.is_none() && m.completion.is_some())
                {
                    break;
                }
            }
            if attempt.driver.active()
                || attempt.terminal.is_some()
                || attempt.peace_fizzle.is_some()
            {
                self.attempts.insert(actor, attempt);
            }
        }
        ids.clear();
        self.actor_scratch = ids;
        self.step_instant_continuations(world, now, policy, fellowships, observers);
        self.step_projectiles_with_observers(world, now, policy, observers);
    }
}

mod region_admission;
