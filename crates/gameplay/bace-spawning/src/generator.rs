//! Pure, bounded incarnation-fenced generator state. See README for source pin.
use crate::placement::{generator_destination, generator_stream};
use bace_gameplay_api::*;
use bace_random::RandomRoot;
use bace_types::EntityId;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorError {
    Definition,
    Capacity,
    Time,
    Random,
    StaleReceipt,
    Receipt,
}
#[derive(Clone, Copy, Debug)]
pub struct GeneratorLimits {
    pub profiles: usize,
    pub pending: usize,
    pub members: usize,
}
impl Default for GeneratorLimits {
    fn default() -> Self {
        Self {
            profiles: 256,
            pending: 1024,
            members: 4096,
        }
    }
}
#[derive(Clone)]
pub(crate) struct ProfileState {
    pub profile: GeneratorProfile,
    pub members: BTreeMap<EntityId, u32>,
    pub queued: usize,
    pub suppressed: usize,
    pub treasure: bool,
    pub first: bool,
    pub available_after: Option<u64>,
    pub invalid: bool,
}
impl ProfileState {
    pub fn count(&self) -> usize {
        let n = self.members.len() + self.queued + self.suppressed;
        if self.treasure { usize::from(n > 0) } else { n }
    }
    pub fn maxed(&self) -> bool {
        self.profile.max_create != -1 && self.count() >= self.profile.max_create as usize
    }
    pub fn available(&self, now: u64) -> bool {
        !self.invalid && self.available_after.is_none_or(|t| now > t)
    }
}
#[derive(Clone)]
pub struct GeneratorMachine {
    pub(crate) definition: Arc<GeneratorDefinition>,
    /// Existing members and queued intents keep their original source identity.
    /// Only newly selected intents use this accepted content identity.
    pub(crate) future_identity: GeneratorIdentity,
    pub(crate) issued_identities: std::collections::BTreeSet<GeneratorIdentity>,
    pub(crate) member_identities: BTreeMap<EntityId, GeneratorIdentity>,
    pub(crate) profiles: Vec<ProfileState>,
    pub(crate) queue: VecDeque<GeneratorSpawnIntent>,
    pub(crate) limits: GeneratorLimits,
    pub(crate) disabled: bool,
    pub(crate) powering: bool,
    pub(crate) staged: bool,
    pub(crate) entered: bool,
    pub(crate) next_update: u64,
    pub(crate) next_regeneration: Option<u64>,
    pub(crate) last_tick: u64,
    pub(crate) occurrence: u64,
    pub(crate) selection: u64,
    pub(crate) has_regenerated: bool,
}
impl GeneratorMachine {
    pub fn new(
        definition: Arc<GeneratorDefinition>,
        clock: GeneratorClock,
        limits: GeneratorLimits,
    ) -> Result<Self, GeneratorError> {
        if limits.profiles == 0
            || limits.profiles > 256
            || limits.pending == 0
            || limits.pending > 65536
            || limits.members == 0
            || limits.members > 65536
        {
            return Err(GeneratorError::Capacity);
        }
        let mut def = (*definition).clone();
        if def.identity.entity.0 == 0
            || def.identity.incarnation == 0
            || def.identity.content_revision == 0
            || def.identity.random_identity == [0; 16]
            || def.profiles.len() > limits.profiles
            || def.initial_count < 0
            || def.maximum_count < 0
            || ![
                def.initial_delay,
                def.regeneration_interval,
                def.regeneration_timestamp,
            ]
            .iter()
            .all(|v| v.is_finite())
            || !def.radius.is_finite()
            || def.radius < 0.0
            || !def
                .location
                .origin
                .iter()
                .chain(&def.location.rotation)
                .all(|v| v.is_finite())
        {
            return Err(GeneratorError::Definition);
        }
        if def.initial_count > 0 && def.maximum_count < def.initial_count {
            def.maximum_count = def.initial_count;
        }
        def.regeneration_interval = def.regeneration_interval.max(0.0);
        let mut ids = std::collections::BTreeSet::new();
        for p in &def.profiles {
            if !ids.insert(p.id)
                || p.weenie_class_id == 0
                || !p.probability.is_finite()
                || (p.probability < 0.0 && p.probability != -1.0)
                || p.init_create < -1
                || p.max_create < -1
                || p.when_create & !7 != 0
                || p.where_create & !127 != 0
                || p.delay.is_some_and(|d| !d.is_finite())
                || p.shade.is_some_and(|v| !v.is_finite())
                || !p
                    .position
                    .origin
                    .iter()
                    .chain(&p.position.rotation)
                    .flatten()
                    .all(|v| v.is_finite())
            {
                return Err(GeneratorError::Definition);
            }
        }
        if def.kind == GeneratorKind::Vendor && !def.vendor_shop_uses_generator {
            def.profiles.retain(|p| p.where_create & 32 == 0);
        }
        let profiles = def
            .profiles
            .iter()
            .cloned()
            .map(|profile| ProfileState {
                profile,
                members: BTreeMap::new(),
                queued: 0,
                suppressed: 0,
                treasure: false,
                first: true,
                available_after: None,
                invalid: false,
            })
            .collect();
        let has_regenerated = def.regeneration_timestamp != 0.0;
        let next_regeneration = (def.regeneration_interval > 0.0).then_some(clock.tick);
        Ok(Self {
            disabled: def.disabled,
            future_identity: def.identity,
            issued_identities: std::iter::once(def.identity).collect(),
            member_identities: BTreeMap::new(),
            definition: Arc::new(def),
            profiles,
            queue: VecDeque::new(),
            limits,
            powering: false,
            staged: false,
            entered: false,
            next_update: clock.tick,
            next_regeneration,
            last_tick: clock.tick,
            occurrence: 0,
            selection: 0,
            has_regenerated,
        })
    }
    pub fn next_wake_tick(&self) -> Option<u64> {
        Some(
            self.next_regeneration
                .map_or(self.next_update, |t| t.min(self.next_update)),
        )
    }
    pub fn definition(&self) -> &GeneratorDefinition {
        &self.definition
    }
    /// A template-only content revision changes future spawn preparation while
    /// preserving source profile order, timers, queued work and owned children.
    pub fn refresh_future_content(
        &mut self,
        next: Arc<GeneratorDefinition>,
    ) -> Result<(), GeneratorError> {
        let mut expected = (*self.definition).clone();
        expected.identity = next.identity;
        if *next != expected
            || next.identity.entity != self.definition.identity.entity
            || next.identity.incarnation != self.definition.identity.incarnation
            || next.identity.content_revision <= self.future_identity.content_revision
            || self.issued_identities.len() >= 4096
        {
            return Err(GeneratorError::Definition);
        }
        self.future_identity = next.identity;
        self.issued_identities.insert(next.identity);
        Ok(())
    }
    pub fn accepts_identity(&self, identity: GeneratorIdentity) -> bool {
        self.issued_identities.contains(&identity)
    }
    pub fn accepts_revision(&self, incarnation: u64, revision: u64) -> bool {
        self.issued_identities.iter().any(|identity| {
            identity.incarnation == incarnation && identity.content_revision == revision
        })
    }
    pub fn next_spawn(&self) -> Option<&GeneratorSpawnIntent> {
        self.queue.front()
    }
    /// Earliest frozen request that the owner can currently admit. Skipped work
    /// retains its deadline, first-spawn flag and random event identity.
    pub fn next_spawn_matching(
        &self,
        mut eligible: impl FnMut(&GeneratorSpawnIntent) -> bool,
    ) -> Option<&GeneratorSpawnIntent> {
        self.queue.iter().find(|intent| eligible(intent))
    }
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
    pub fn current_create(&self) -> usize {
        self.profiles.iter().map(ProfileState::count).sum()
    }
    pub fn disabled(&self) -> bool {
        self.disabled
    }
    pub fn powering_up(&self) -> bool {
        self.powering
    }
    /// Authored EmoteType.Generate invokes the current generator selection; it
    /// does not activate/reset a disabled generator or restart its timers.
    pub fn generate_now(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
    ) -> Result<GeneratorTransition, GeneratorError> {
        self.check_time(clock)?;
        let mut draft = self.clone();
        let mut transition = GeneratorTransition::default();
        draft.generate(clock, root, &mut transition)?;
        draft.last_tick = clock.tick;
        *self = draft;
        Ok(transition)
    }
    pub fn owns_member(&self, entity: EntityId) -> bool {
        self.profiles
            .iter()
            .any(|p| p.members.contains_key(&entity))
    }
    /// Reconcile an exact durable contained creature before the first cold
    /// population tick. The creature keeps its saved identity and equipment;
    /// this only restores the current generator's occupancy.
    pub fn adopt_restored_contained_member(
        &mut self,
        profile_id: u32,
        entity: EntityId,
        template: u32,
    ) -> Result<(), GeneratorError> {
        if entity.0 == 0 || template == 0 || self.owns_member(entity) {
            return Err(GeneratorError::Receipt);
        }
        let index = self
            .profiles
            .iter()
            .position(|p| p.profile.id == profile_id)
            .ok_or(GeneratorError::StaleReceipt)?;
        let profile = &self.profiles[index];
        if profile.profile.weenie_class_id != template
            || profile.profile.where_create & 8 == 0
            || self.profiles.iter().map(|p| p.members.len()).sum::<usize>() >= self.limits.members
        {
            return Err(GeneratorError::Receipt);
        }
        self.profiles[index].members.insert(entity, 1);
        self.member_identities
            .insert(entity, self.definition.identity);
        Ok(())
    }
    pub fn member(&self, entity: EntityId) -> Option<GeneratorSpawnMember> {
        self.profiles.iter().find_map(|p| {
            p.members
                .get(&entity)
                .map(|&contribution| GeneratorSpawnMember {
                    entity,
                    contribution,
                })
        })
    }
    pub fn active_profiles(&self, now: u64) -> Vec<u32> {
        self.profiles
            .iter()
            .filter(|p| p.count() > 0 || !p.available(now))
            .map(|p| p.profile.id)
            .collect()
    }
    pub(crate) fn check_time(&self, clock: GeneratorClock) -> Result<(), GeneratorError> {
        if clock.tick < self.last_tick {
            Err(GeneratorError::Time)
        } else {
            Ok(())
        }
    }
    pub(crate) fn stop(&self, now: u64) -> bool {
        self.current_create() >= self.definition.maximum_count as usize
            || (self.powering && self.current_create() >= self.definition.initial_count as usize)
            || !self
                .profiles
                .iter()
                .any(|p| p.profile.weenie_class_id != 3666 && p.available(now))
            || !self
                .profiles
                .iter()
                .any(|p| p.profile.weenie_class_id != 3666 && !p.maxed() && !p.invalid)
    }
    pub fn total_probability(&self, now: u64) -> f32 {
        let mut total = 0.0;
        let mut last = 0.0;
        for p in &self.profiles {
            let prob = p.profile.probability;
            if prob == -1.0 {
                if !p.maxed() && p.available(now) {
                    return 1.0;
                }
                continue;
            }
            if !p.maxed() && p.available(now) {
                if last > prob {
                    last = 0.0;
                }
                total += prob - last;
            }
            last = prob;
        }
        total
    }
    pub fn adjusted_probability(&self, index: usize, now: u64) -> Option<f32> {
        let target = self.profiles.get(index)?;
        if target.profile.probability == -1.0 {
            return Some(-1.0);
        }
        let mut total = 0.0;
        let mut last = 0.0;
        for p in self.profiles.iter().take(index + 1) {
            let prob = p.profile.probability;
            if prob == -1.0 {
                continue;
            }
            if !p.maxed() && p.available(now) {
                if last > prob {
                    last = 0.0;
                }
                total += prob - last;
            }
            last = prob;
        }
        Some(total)
    }
    pub(crate) fn select(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
        out: &mut GeneratorTransition,
    ) -> Result<(), GeneratorError> {
        if self.stop(clock.tick) {
            return Ok(());
        }
        let mut stream = generator_stream(root, self.future_identity)?
            .fork(b"selection", self.selection)
            .map_err(|_| GeneratorError::Random)?;
        self.selection = self
            .selection
            .checked_add(1)
            .ok_or(GeneratorError::Random)?;
        let unit = (stream.next_u64().map_err(|_| GeneratorError::Random)? >> 11) as f64
            / (1u64 << 53) as f64;
        self.select_roll(clock, root, unit, out)
    }
    fn select_roll(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
        unit: f64,
        out: &mut GeneratorTransition,
    ) -> Result<(), GeneratorError> {
        let roll = unit * f64::from(self.total_probability(clock.tick));
        out.selection_attempts += 1;
        for i in 0..self.profiles.len() {
            let p = &mut self.profiles[i];
            if p.profile.weenie_class_id == 3666 || p.maxed() || !p.available(clock.tick) {
                continue;
            }
            if p.profile.where_create & 64 != 0 {
                p.profile.init_create = p.profile.init_create.min(1);
                p.profile.max_create = p.profile.max_create.min(1);
            }
            let probability = self
                .adjusted_probability(i, clock.tick)
                .ok_or(GeneratorError::Definition)?;
            if roll >= f64::from(probability) && probability != -1.0 {
                continue;
            }
            let p = &self.profiles[i];
            let mut count = if p.profile.init_create == -1 || p.profile.max_create == -1 {
                1
            } else {
                p.profile.init_create as usize
            };
            count = count.min(
                (self.definition.maximum_count as usize).saturating_sub(self.current_create()),
            );
            if p.profile.max_create != -1 {
                count = count.min((p.profile.max_create as usize).saturating_sub(p.count()));
            }
            if self.queue.len() + count > self.limits.pending {
                return Err(GeneratorError::Capacity);
            }
            for _ in 0..count {
                self.occurrence = self
                    .occurrence
                    .checked_add(1)
                    .ok_or(GeneratorError::Random)?;
                let p = &mut self.profiles[i];
                let mut stream = generator_stream(root, self.future_identity)?
                    .fork(b"spawn", self.occurrence)
                    .map_err(|_| GeneratorError::Random)?;
                let mut random_identity = [0; 16];
                random_identity[..8].copy_from_slice(
                    &stream
                        .next_u64()
                        .map_err(|_| GeneratorError::Random)?
                        .to_le_bytes(),
                );
                random_identity[8..].copy_from_slice(
                    &stream
                        .next_u64()
                        .map_err(|_| GeneratorError::Random)?
                        .to_le_bytes(),
                );
                self.queue.push_back(GeneratorSpawnIntent {
                    key: GeneratorSpawnKey {
                        generator: self.future_identity,
                        profile_id: p.profile.id,
                        occurrence: self.occurrence,
                    },
                    profile: p.profile.clone(),
                    destination: generator_destination(&self.definition, &p.profile),
                    first_spawn: p.first,
                    due_tick: clock.tick,
                    random_identity,
                    random_key_version: root.key_version(),
                });
                p.queued += 1;
                out.enqueued += 1;
            }
            if self.profiles[i].profile.probability != -1.0 || self.stop(clock.tick) {
                break;
            }
        }
        Ok(())
    }
    pub(crate) fn generate(
        &mut self,
        clock: GeneratorClock,
        root: &RandomRoot,
        out: &mut GeneratorTransition,
    ) -> Result<(), GeneratorError> {
        if self.disabled {
            return Ok(());
        }
        if self.powering {
            let mut attempts = 0;
            while !self.stop(clock.tick) {
                self.select(clock, root, out)?;
                attempts += 1;
                if attempts > 1000 {
                    out.exhausted_initial_loop = true;
                    break;
                }
            }
            self.powering = false;
        } else {
            self.select(clock, root, out)?;
        }
        Ok(())
    }
    pub fn acknowledge(
        &mut self,
        receipt: GeneratorSpawnReceipt,
    ) -> Result<GeneratorTransition, GeneratorError> {
        let index = self
            .queue
            .iter()
            .position(|q| q.key == receipt.key)
            .ok_or(GeneratorError::StaleReceipt)?;
        if matches!(receipt.result, GeneratorSpawnResult::Blocked(_)) {
            return Ok(GeneratorTransition::default());
        }
        let intent = &self.queue[index];
        let profile_index = self
            .profiles
            .iter()
            .position(|p| p.profile.id == receipt.key.profile_id)
            .ok_or(GeneratorError::StaleReceipt)?;
        let mut out = GeneratorTransition::default();
        match receipt.result {
            GeneratorSpawnResult::Blocked(_) => {}
            GeneratorSpawnResult::Invalid(failure) => {
                self.profiles[profile_index].invalid = true;
                out.effects.push(GeneratorLifecycleEffect::Invalidated {
                    key: receipt.key,
                    failure,
                });
            }
            GeneratorSpawnResult::Completed {
                members,
                materialized,
                failed_placements,
            } => {
                if members.len() > self.limits.members
                    || self
                        .profiles
                        .iter()
                        .map(|p| p.members.len() + p.suppressed)
                        .sum::<usize>()
                        .saturating_add(members.len())
                        .saturating_add(failed_placements as usize)
                        > self.limits.members
                    || (!materialized && (!members.is_empty() || failed_placements > 0))
                {
                    return Err(GeneratorError::Receipt);
                }
                let mut seen = std::collections::BTreeSet::new();
                if members.iter().any(|m| {
                    m.entity.0 == 0
                        || m.contribution == 0
                        || !seen.insert(m.entity)
                        || self
                            .member_identities
                            .get(&m.entity)
                            .is_some_and(|identity| *identity != receipt.key.generator)
                        || self.profiles[profile_index]
                            .members
                            .get(&m.entity)
                            .is_some_and(|v| v.checked_add(m.contribution).is_none())
                }) {
                    return Err(GeneratorError::Receipt);
                }
                if intent.profile.where_create & 64 == 0
                    && members.len() + failed_placements as usize > 1
                {
                    return Err(GeneratorError::Receipt);
                }
                let p = &mut self.profiles[profile_index];
                if materialized && p.profile.where_create & 64 != 0 {
                    p.treasure = true;
                }
                for member in members {
                    *p.members.entry(member.entity).or_default() += member.contribution;
                    self.member_identities
                        .insert(member.entity, receipt.key.generator);
                }
                if intent.first_spawn && failed_placements > 0 {
                    p.suppressed += failed_placements as usize;
                    out.effects
                        .push(GeneratorLifecycleEffect::SuppressedInitial {
                            key: receipt.key,
                            count: failed_placements,
                        });
                }
            }
        }
        self.queue.remove(index);
        let p = &mut self.profiles[profile_index];
        p.queued -= 1;
        p.first = false;
        Ok(out)
    }
}
pub(crate) fn delay_tick(now: u64, seconds: f64) -> Result<u64, GeneratorError> {
    let ticks = (seconds.max(0.0) * 30.0).ceil();
    if !ticks.is_finite() || ticks >= u64::MAX as f64 {
        return Err(GeneratorError::Time);
    }
    now.checked_add(ticks as u64).ok_or(GeneratorError::Time)
}
#[cfg(test)]
mod reference_tests;
