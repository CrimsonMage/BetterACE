//! Single-owner synthetic population/PvE integration. Immutable blueprint inputs
//! must be prepared off-thread; production AC geometry/content remains gated.
mod ace;
mod admission;
mod appraisal;
mod appraisal_profile;
mod corpse_expiry;
mod death_lifecycle;
mod native;
mod npc_home;
mod npc_retirement;
mod owned_loot;
mod shared_rewards;
use crate::characters::Characters;
use crate::combat::{Combat, CombatEvent};
pub use ace::AceCreatureLootPolicy;
pub use appraisal::{
    AppraisalCreatureKind, AppraisalResponseKind, AppraisalRoll, AppraisalSelectionError,
    AppraisalSelectionResult, AppraisalTargetCheck, AppraisalWake, evaluate_appraisal_roll,
    select_appraisal,
};
pub use appraisal_profile::AppraisalSourceTables;
use bace_ai::{Awareness, MonsterLeash, ReturnHome, TargetCandidate};
use bace_character::ExperienceCredit;
use bace_combat::{DamageShare, kill_rewards};
use bace_entity::{Actor, Combatant, CombatantProfile};
use bace_gameplay_api::CombatRequest;
use bace_geometry::Vec3;
use bace_loot::{CreateEntry, select_create_list};
use bace_motion::{Capabilities, MotionIntent};
use bace_physics::Body;
use bace_spawning::SpawnSchedule;
use bace_types::{CellId, EntityId};
use bace_world::{CorpseState, World};
pub use native::{NativeDeathLoot, NativeLootPolicy};
use std::collections::{BTreeMap, VecDeque};

// Deliberate input hardening: at 30 Hz this permits over four years, while
// rejecting effectively infinite timers which cannot be repaired after admission.
const MAX_TIMER_TICKS: u64 = u32::MAX as u64;

#[derive(Clone, Debug, PartialEq)]
pub struct NpcLootEntry {
    pub template: u32,
    pub destination: u32,
    pub probability: f32,
    pub stack: u32,
}
#[derive(Clone, Debug)]
pub struct NpcBlueprint {
    pub cell: CellId,
    pub position: Vec3,
    pub radius: f32,
    pub capabilities: Capabilities,
    pub combat: CombatantProfile,
    pub visual_range: f32,
    pub think_interval: u64,
    pub corpse_template: u32,
    pub xp_override: Option<i32>,
    pub loot: Vec<NpcLootEntry>,
    pub death_animation_ticks: u64,
    pub respawn_ticks: u64,
    pub corpse_decay_ticks: u64,
}
/// Immutable collision and DAT locomotion prepared before owner admission.
#[derive(Clone, Debug)]
pub struct PreparedNpcGeometry {
    pub shape: std::sync::Arc<bace_physics::CollisionShape>,
    pub locomotion: std::sync::Arc<bace_motion::AnimatedLocomotion>,
    pub heading: f32,
    pub maximum_turn_rate: f32,
    pub run_rate: f32,
}
pub(crate) struct PreparedNpcPhysical {
    pub geometry: PreparedNpcGeometry,
    pub resources: [bace_entity::VitalPool; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedNpcOrigin {
    pub generator: EntityId,
    pub incarnation: u64,
    pub content_revision: u64,
    pub profile: u32,
    pub child_incarnation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeneratedNpcDeath {
    pub actor: EntityId,
    pub origin: GeneratedNpcOrigin,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LootDrop {
    pub template: u32,
    pub stack: u32,
}
/// Valuable-operation proposal only. Item IDs and durable inventory mutations
/// belong to the persistence coordinator, which acknowledges the same operation.
#[derive(Clone, Debug, PartialEq)]
pub struct DeathProposal {
    pub operation: u64,
    pub native: Option<NativeDeathLoot>,
    pub victim: EntityId,
    pub owner: Option<EntityId>,
    /// Copied from the simulation's accepted physical state at the kill.
    /// Synthetic legacy fixtures may omit it; durable native death may not.
    pub position: Option<bace_content::Position>,
    pub no_corpse: bool,
    pub olthoi_killer: bool,
    pub corpse_template: u32,
    pub corpse_decay_ticks: u64,
    pub drops: Vec<LootDrop>,
    pub experience: Vec<(EntityId, i64)>,
    pub experience_state: Vec<(EntityId, ExperienceCredit)>,
    pub social: Option<crate::AllegianceTicket>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PveError {
    Capacity,
    InvalidProfile,
    Duplicate,
    MissingGeometry,
    MissingActor,
    InvalidRandom,
    UnknownOperation,
    InvalidReceipt,
    Overflow,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PveEvent {
    CorpseCreated {
        operation: u64,
        corpse: EntityId,
    },
    NoCorpseWorldDropsCreated {
        operation: u64,
        victim: EntityId,
        roots: Vec<EntityId>,
    },
    Respawned {
        previous: EntityId,
        actor: EntityId,
    },
    CorpseDecayed {
        corpse: EntityId,
    },
}
struct Npc {
    /// Source IsNPC suppresses automatic combat acquisition; interaction/emote
    /// movement remains with its existing owner.
    combat_ai: bool,
    /// Pinned ACE PropertyInt.Tolerance (67). The Appraise bit prevents spawn
    /// acquisition until an accepted appraisal wakes this creature.
    tolerance: u32,
    awake: bool,
    geometry: Option<PreparedNpcGeometry>,
    origin: Option<GeneratedNpcOrigin>,
    blueprint: NpcBlueprint,
    next_think: u64,
    sequence: u32,
    home: Option<(CellId, Vec3)>,
    home_heading: Option<f32>,
    returning_since: Option<u64>,
    leash: MonsterLeash,
    next_attack: u64,
    target: Option<EntityId>,
    thought_at: u64,
}
struct PendingDeath {
    death_motion: Option<(bace_motion::MotionToken, u16)>,
    shared_waiting: bool,
    origin: Option<GeneratedNpcOrigin>,
    proposal: DeathProposal,
    blueprint: NpcBlueprint,
    ready: u64,
    submitted: bool,
    prepared: Option<PreparedPveDeath>,
}
pub(crate) struct PreparedPveDeath {
    pub(crate) forest: crate::PreparedWorldRegionItems,
    pub(crate) roots: Vec<(Actor, Option<CorpseState>)>,
}
pub(crate) struct Population {
    generated_deaths: VecDeque<GeneratedNpcDeath>,
    npcs: BTreeMap<EntityId, Npc>,
    pending: BTreeMap<u64, PendingDeath>,
    proposals: VecDeque<DeathProposal>,
    events: VecDeque<PveEvent>,
    random: VecDeque<f32>,
    ids: VecDeque<EntityId>,
    respawns: SpawnSchedule,
    respawn_blueprints: BTreeMap<u32, NpcBlueprint>,
    decays: SpawnSchedule,
    operation: u64,
    capacity: usize,
    native: native::NativeLoot,
    thought_at: u64,
    melee_candidates: Vec<(EntityId, EntityId)>,
    native_error: Option<PveError>,
}
impl Population {
    pub(crate) fn new(capacity: usize) -> Self {
        let capacity = capacity.min(4096);
        Self {
            generated_deaths: VecDeque::with_capacity(capacity),
            npcs: BTreeMap::new(),
            pending: BTreeMap::new(),
            proposals: VecDeque::with_capacity(capacity),
            events: VecDeque::with_capacity(capacity),
            random: VecDeque::with_capacity(65536),
            ids: VecDeque::with_capacity(capacity),
            respawns: SpawnSchedule::new(capacity).expect("valid kernel capacity"),
            respawn_blueprints: BTreeMap::new(),
            decays: SpawnSchedule::new(capacity).expect("valid kernel capacity"),
            operation: 0,
            capacity,
            native: native::NativeLoot::new(),
            thought_at: u64::MAX,
            melee_candidates: Vec::with_capacity(capacity),
            native_error: None,
        }
    }
    pub(crate) fn feed_random(&mut self, draws: &[f32]) -> Result<(), PveError> {
        if self.native.enabled() {
            return Err(PveError::InvalidRandom);
        }
        if draws
            .iter()
            .any(|d| !d.is_finite() || !(0.0..1.0).contains(d))
        {
            return Err(PveError::InvalidRandom);
        }
        if draws.len() > 65536 - self.random.len() {
            return Err(PveError::Capacity);
        }
        self.random.extend(draws);
        Ok(())
    }
    pub(crate) fn feed_id(&mut self, id: EntityId, world: &World) -> Result<(), PveError> {
        if id.0 == 0 || world.contains_identity(id) || self.reserves_identity(id) {
            return Err(PveError::Duplicate);
        }
        if self.ids.len() == self.capacity {
            return Err(PveError::Capacity);
        }
        self.ids.push_back(id);
        Ok(())
    }
    pub(crate) fn reserves_identity(&self, id: EntityId) -> bool {
        self.ids.contains(&id)
            || self.npcs.contains_key(&id)
            || self.respawn_blueprints.contains_key(&id.0)
    }
    pub(crate) fn spawn(
        &mut self,
        actor: EntityId,
        blueprint: NpcBlueprint,
        world: &mut World,
        now: u64,
    ) -> Result<(), PveError> {
        if self.reserves_identity(actor) {
            return Err(PveError::Duplicate);
        }
        self.admit(actor, blueprint, world, now, None)
    }
    /// Internal admission after the population owner selected its reserved ID.
    /// Ordinary admissions cannot bypass the public reservation check above.
    fn admit(
        &mut self,
        actor: EntityId,
        blueprint: NpcBlueprint,
        world: &mut World,
        now: u64,
        prepared: Option<(PreparedNpcPhysical, GeneratedNpcOrigin)>,
    ) -> Result<(), PveError> {
        let (geometry, origin, resources) = match prepared {
            Some((physical, origin)) => (
                Some(physical.geometry),
                Some(origin),
                Some(physical.resources),
            ),
            None => (None, None, None),
        };
        if self.npcs.len() >= self.capacity {
            return Err(PveError::Capacity);
        }
        if actor.0 == 0 || self.npcs.contains_key(&actor) || world.contains_identity(actor) {
            return Err(PveError::Duplicate);
        }
        if blueprint.combat.player
            || blueprint.corpse_template == 0
            || blueprint.think_interval == 0
            || [
                blueprint.think_interval,
                blueprint.death_animation_ticks,
                blueprint.respawn_ticks,
                blueprint.corpse_decay_ticks,
            ]
            .iter()
            .any(|delay| *delay > MAX_TIMER_TICKS || now.checked_add(*delay).is_none())
            || !blueprint.visual_range.is_finite()
            || blueprint.visual_range <= 0.0
            || blueprint.visual_range > 192.0
            || blueprint.loot.len() > 256
            || blueprint.xp_override.is_some_and(|xp| xp < 0)
            || blueprint.loot.iter().any(|row| {
                row.stack == 0
                    || !row.probability.is_finite()
                    || !(0.0..=1.0).contains(&row.probability)
            })
        {
            return Err(PveError::InvalidProfile);
        }
        let mut combatant =
            Combatant::new(blueprint.combat.clone()).map_err(|_| PveError::InvalidProfile)?;
        if let Some(resources) = resources {
            combatant = combatant
                .with_resources(Some(resources[0]), Some(resources[1]))
                .map_err(|_| PveError::InvalidProfile)?;
        }
        let mut body = if let Some(prepared) = &geometry {
            if !prepared.run_rate.is_finite() || !(0.0..=20.0).contains(&prepared.run_rate) {
                return Err(PveError::InvalidProfile);
            }
            prepared
                .locomotion
                .profile
                .interpret(
                    bace_motion::LocomotionControls::default(),
                    prepared.run_rate,
                )
                .map_err(|_| PveError::InvalidProfile)?;
            world
                .prepare_geometry_body(bace_physics::GeometrySpawn {
                    cell: blueprint.cell.0,
                    position: blueprint.position,
                    shape: prepared.shape.clone(),
                    capabilities: blueprint.capabilities,
                    heading: prepared.heading,
                    maximum_turn_rate: prepared.maximum_turn_rate,
                })
                .map_err(|_| PveError::MissingGeometry)?
        } else {
            Body::spawn(
                world
                    .scene(blueprint.cell)
                    .map_err(|_| PveError::MissingGeometry)?,
                blueprint.position,
                blueprint.radius,
                blueprint.capabilities,
            )
            .map_err(|_| PveError::MissingGeometry)?
        };
        if let Some(prepared) = &geometry {
            body.adopt_animated_style(prepared.locomotion.clone(), true)
                .map_err(|_| PveError::InvalidProfile)?;
            body.refresh_locomotion(&prepared.locomotion.profile, prepared.run_rate)
                .map_err(|_| PveError::InvalidProfile)?;
        }
        world
            .insert(Actor {
                id: actor,
                cell: blueprint.cell,
                body,
            })
            .map_err(|_| PveError::MissingGeometry)?;
        world
            .register_combatant(actor, combatant)
            .expect("new actor validated before insertion");
        self.npcs.insert(
            actor,
            Npc {
                combat_ai: true,
                tolerance: 0,
                awake: false,
                geometry,
                origin,
                blueprint,
                next_think: now,
                sequence: 0,
                home: None,
                home_heading: None,
                returning_since: None,
                leash: MonsterLeash::default(),
                next_attack: now,
                target: None,
                thought_at: u64::MAX,
            },
        );
        Ok(())
    }
    pub(crate) fn set_leash(
        &mut self,
        actor: EntityId,
        leash: MonsterLeash,
    ) -> Result<(), PveError> {
        if !leash.valid() {
            return Err(PveError::InvalidProfile);
        }
        let npc = self.npcs.get_mut(&actor).ok_or(PveError::MissingActor)?;
        npc.leash = leash;
        Ok(())
    }
    pub(crate) fn think(
        &mut self,
        world: &mut World,
        combat: &mut Combat,
        tick: u64,
        casting: impl Fn(EntityId) -> bool,
    ) {
        self.thought_at = tick;
        self.melee_candidates.clear();
        for (&actor, npc) in &mut self.npcs {
            if !npc.combat_ai
                || (!npc.awake && npc.tolerance & (1 | 2 | 8 | 64) != 0)
                || world.actor_region_dormant(actor)
                || tick < npc.next_think
                || world.combatant(actor).is_none_or(|c| c.health() == 0)
            {
                continue;
            }
            npc.next_think = tick.saturating_add(npc.blueprint.think_interval);
            npc.thought_at = tick;
            if casting(actor) {
                continue;
            }
            let mut nearest: Option<(EntityId, f32)> = None;
            let awareness = Awareness {
                current_target: None,
                visual_range_squared: npc.blueprint.visual_range * npc.blueprint.visual_range,
                chase_range_squared: npc.leash.chase_range * npc.leash.chase_range,
                target_locked: false,
                monsters_only: false,
            };
            let Some((_, cell, accepted)) = world.states().find(|(id, _, _)| *id == actor) else {
                continue;
            };
            // GDLE establishes home only from accepted walkable physics state.
            if npc.home.is_none() {
                if !accepted.grounded() {
                    continue;
                }
                npc.home = Some((cell, accepted.position()));
                npc.home_heading = Some(accepted.heading_radians());
            }
            let (home_cell, home) = npc.home.expect("established from accepted physics");
            // Local coordinates from different cells are not distances in a
            // shared frame. Missing cross-cell navigation must not move toward
            // unrelated coordinates or change the stored home identity.
            let home_distance = if cell == home_cell {
                (accepted.position() - home).length_squared()
            } else {
                f32::INFINITY
            };
            if npc.leash.exceeded_home(home_distance) && npc.returning_since.is_none() {
                npc.returning_since = Some(tick);
            }
            if let Some(started) = npc.returning_since {
                combat.cancel(actor);
                let decision = npc.leash.returning(home_distance, started, tick);
                let mut direction = Vec3::ZERO;
                match decision {
                    ReturnHome::Arrived => {
                        npc.returning_since = None;
                        npc.awake = false;
                    }
                    ReturnHome::Travel => {
                        if cell == home_cell {
                            direction = (home - accepted.position()).horizontal_clamped();
                        }
                    }
                    ReturnHome::Teleport => {
                        if world.teleport(actor, home_cell, home).is_ok() {
                            npc.returning_since = None;
                        }
                    }
                }
                npc.target = None;
                if let Ok(body) = world.body_mut(actor) {
                    npc.sequence = npc.sequence.wrapping_add(1);
                    drive_npc(body, npc, direction);
                }
                continue;
            }
            for (candidate, candidate_cell, state) in world.states() {
                if cell != candidate_cell
                    || world
                        .combatant(candidate)
                        .is_none_or(|s| !s.profile().player || s.health() == 0)
                {
                    continue;
                }
                let distance = (state.position() - accepted.position()).length_squared();
                if distance >= npc.leash.chase_range * npc.leash.chase_range {
                    continue;
                }
                let eligible = awareness
                    .eligible(&TargetCandidate {
                        actor: candidate.0,
                        distance_squared: distance,
                        attackable: true,
                        has_targeting_tactic: false,
                        teleporting: false,
                        same_faction: false,
                        retaliate: false,
                        player_or_combat_pet: true,
                    })
                    .unwrap_or(false);
                if eligible
                    && (npc.target == Some(candidate)
                        || nearest.is_none_or(|(_, old)| distance < old))
                {
                    nearest = Some((candidate, distance));
                    if npc.target == Some(candidate) {
                        break;
                    }
                }
            }
            let mut direction = Vec3::ZERO;
            npc.target = nearest.map(|(target, _)| target);
            if npc.target.is_some() {
                npc.awake = true;
            }
            if let Some((target, _)) = nearest {
                let range = combat
                    .physical_profile(actor)
                    .and_then(|p| p.missile)
                    .and_then(|m| bace_combat::physical::npc_missile_range(f64::from(m.speed)).ok())
                    .unwrap_or(npc.blueprint.combat.melee_range);
                let (in_range, clear) = world
                    .attack_geometry(actor, target, range)
                    .unwrap_or((false, false));
                if in_range && clear {
                    if !combat.active(actor) && tick >= npc.next_attack {
                        self.melee_candidates.push((actor, target));
                    }
                } else if !combat.active(actor)
                    && let Ok(target_body) = world.body(target)
                {
                    direction = (target_body.accepted().position() - accepted.position())
                        .horizontal_clamped();
                }
            }
            if nearest.is_none()
                && home_distance >= npc.leash.arrival_range * npc.leash.arrival_range
            {
                npc.returning_since = Some(tick);
                combat.cancel(actor);
            }
            if let Ok(body) = world.body_mut(actor) {
                npc.sequence = npc.sequence.wrapping_add(1);
                drive_npc(body, npc, direction);
            }
        }
    }
    pub(crate) fn native_error(&self) -> Option<PveError> {
        self.native_error
    }
    pub(crate) fn take_proposal(&mut self) -> Option<DeathProposal> {
        self.proposals.pop_front()
    }
    pub(crate) fn take_event(&mut self) -> Option<PveEvent> {
        self.events.pop_front()
    }
    pub(crate) fn pending(&self) -> usize {
        self.pending.len()
    }
    pub(crate) fn has_state(&self) -> bool {
        !self.npcs.is_empty()
            || !self.pending.is_empty()
            || !self.events.is_empty()
            || !self.generated_deaths.is_empty()
            || self.respawns.pending() != 0
            || self.decays.pending() != 0
    }
}

impl Population {
    pub(crate) fn magic_targets(&self) -> impl Iterator<Item = (EntityId, Option<EntityId>)> + '_ {
        self.npcs
            .iter()
            .filter(|(_, npc)| npc.thought_at == self.thought_at)
            .map(|(actor, npc)| (*actor, npc.target))
    }
}

impl Population {
    pub(crate) fn start_melee_candidates(
        &mut self,
        world: &mut World,
        combat: &mut Combat,
        inventory: &crate::inventory::Inventory,
        tick: u64,
        casting: impl Fn(EntityId) -> bool,
    ) {
        for &(actor, target) in &self.melee_candidates {
            if casting(actor) || combat.active(actor) {
                continue;
            }
            let missile = combat.physical_profile(actor).and_then(|p| p.missile);
            if combat
                .apply_with_equipment(
                    world,
                    actor,
                    CombatRequest::ChangeMode(if missile.is_some() { 4 } else { 2 }),
                    tick as f64 / 30.0,
                    inventory,
                )
                .is_err()
            {
                continue;
            }
            if combat
                .apply_with_equipment(
                    world,
                    actor,
                    if missile.is_some() {
                        CombatRequest::TargetedMissile {
                            target,
                            height: 2,
                            accuracy: 0.5,
                        }
                    } else {
                        CombatRequest::TargetedMelee {
                            target,
                            height: 2,
                            power: 0.5,
                        }
                    },
                    tick as f64 / 30.0,
                    inventory,
                )
                .is_ok()
                && let Some(npc) = self.npcs.get_mut(&actor)
            {
                npc.next_attack = tick.saturating_add(
                    missile.map_or(75, |m| ((m.duration_seconds + 1.0) * 30.0).ceil() as u64),
                );
            }
        }
        self.melee_candidates.clear();
    }
}

impl Population {
    pub(crate) fn spawn_generated(
        &mut self,
        actor: EntityId,
        blueprint: NpcBlueprint,
        physical: PreparedNpcPhysical,
        origin: GeneratedNpcOrigin,
        world: &mut World,
        now: u64,
    ) -> Result<(), PveError> {
        if self.reserves_identity(actor) {
            return Err(PveError::Duplicate);
        }
        if origin.generator.0 == 0 || origin.incarnation == 0 || origin.child_incarnation == 0 {
            return Err(PveError::InvalidProfile);
        }
        self.admit(actor, blueprint, world, now, Some((physical, origin)))
    }
    pub(crate) fn generated_death(&self) -> Option<GeneratedNpcDeath> {
        self.generated_deaths.front().copied()
    }
    pub(crate) fn acknowledge_generated_death(&mut self, death: GeneratedNpcDeath) -> bool {
        if self.generated_deaths.front() != Some(&death) {
            return false;
        }
        self.generated_deaths.pop_front();
        true
    }
    pub(crate) fn generated_origin(&self, actor: EntityId) -> Option<GeneratedNpcOrigin> {
        self.npcs.get(&actor).and_then(|n| n.origin)
    }
    pub(crate) fn can_remove_generated(
        &self,
        actor: EntityId,
        origin: GeneratedNpcOrigin,
    ) -> Result<(), PveError> {
        if self.pending.values().any(|p| p.proposal.victim == actor) {
            return Err(PveError::InvalidReceipt);
        }
        if self.npcs.get(&actor).and_then(|n| n.origin) != Some(origin) {
            return Err(PveError::MissingActor);
        }
        Ok(())
    }
    pub(crate) fn remove_generated(
        &mut self,
        actor: EntityId,
        origin: GeneratedNpcOrigin,
        world: &mut World,
    ) -> Result<(), PveError> {
        self.can_remove_generated(actor, origin)?;
        self.npcs.remove(&actor);
        self.native.retired(actor);
        world.remove(actor);
        Ok(())
    }
}
fn drive_npc(body: &mut Body, npc: &Npc, direction: Vec3) {
    // Scripted approach/turn and physical attack approach have an explicit
    // owner. Ordinary AI cannot cancel that owner by emitting another intent.
    if body.server_move().is_some() || body.server_turn().is_some() {
        return;
    }
    let result = if let Some(geometry) = &npc.geometry {
        let heading = body.accepted().heading_radians();
        let moving = direction.length_squared() > 0.0001;
        let desired = if moving {
            (-direction.x).atan2(direction.y)
        } else {
            heading
        };
        let delta = (desired - heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let controls = bace_motion::LocomotionControls {
            forward: if moving && delta.abs() < 0.5 {
                1.0
            } else {
                0.0
            },
            turn: (delta / 0.25).clamp(-1.0, 1.0),
            run: moving,
            sidestep: 0.0,
        };
        body.submit_animated_locomotion(
            body.accepted().epoch(),
            npc.sequence,
            body.animated_locomotion()
                .cloned()
                .unwrap_or_else(|| geometry.locomotion.clone()),
            controls,
            geometry.run_rate,
        )
        .map(|_| ())
    } else {
        MotionIntent::new(direction, false)
            .map_err(bace_physics::PhysicsError::from)
            .and_then(|intent| body.submit_intent(body.accepted().epoch(), npc.sequence, intent))
    };
    if result.is_err() {
        body.stop_motion();
    }
}

#[cfg(test)]
mod generator_tests;
