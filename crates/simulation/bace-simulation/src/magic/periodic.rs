//! ACE EnchantmentManager.HeartBeat applies periodic effects before expiration.
//! DF bleed is one top category (685); stored StatModValue is damage per tick.
use super::*;
use bace_combat::specialization::{DefenseKind, specialized_defense_rating};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PeriodicKind {
    DirtyFighting,
    Damage,
    Nether,
    Healing,
    NativeBase,
    NativeNether,
    NativeHealing,
}
pub(super) struct PeriodicEffect {
    pub source: EntityId,
    pub amount: f64,
    pub kind: PeriodicKind,
}
pub(super) struct PeriodicPulse {
    pub target: EntityId,
    pub effects: Vec<PeriodicEffect>,
    pub index: usize,
    pub expired: Vec<(u32, u16)>,
    pub applied: bool,
    pub native_pending: Option<(u32, Option<u64>)>,
    pub nonce: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeriodicDefenseProfile {
    pub augmentation_reduction: u32,
    pub dot_resistance: u32,
    /// Pinned GDLE Config.cpp default is 1.0; applied only to player Nether DoT.
    pub void_player_modifier: f64,
}
impl Default for PeriodicDefenseProfile {
    fn default() -> Self {
        Self {
            augmentation_reduction: 0,
            dot_resistance: 0,
            void_player_modifier: 1.0,
        }
    }
}
pub(super) fn periodic_pulse(
    actor: EntityId,
    registry: &EnchantmentRegistry,
) -> Option<PeriodicPulse> {
    let mut effects = Vec::new();
    for (index, entry) in registry.entries().iter().enumerate() {
        if registry.entries()[..index]
            .iter()
            .any(|old| old.spec.category == entry.spec.category)
        {
            continue;
        }
        let top = registry.top(entry.spec.category, 0.0)?;
        let kind = match top.spec.stat_key {
            318 if top.spec.category == 685 => PeriodicKind::DirtyFighting,
            318 => PeriodicKind::Damage,
            330 => PeriodicKind::Nether,
            312 => PeriodicKind::Healing,
            _ => continue,
        };
        effects.push(PeriodicEffect {
            source: EntityId(top.caster),
            amount: f64::from(top.spec.value),
            kind,
        });
    }
    (!effects.is_empty()).then_some(PeriodicPulse {
        target: actor,
        effects,
        index: 0,
        expired: Vec::new(),
        applied: false,
        native_pending: None,
        nonce: 0,
    })
}
impl Magic {
    pub(super) fn periodic_involves(&self, actor: EntityId) -> bool {
        self.periodic
            .iter()
            .any(|p| p.target == actor || p.effects.iter().any(|e| e.source == actor))
    }
    /// ACE SkillHelper sets and GetAttack/DefenseDebuffMod; base skill is unchanged.
    pub(crate) fn dirty_skill_modifier(&self, actor: EntityId, skill: u32) -> i32 {
        let Some(registry) = self.registry(actor) else {
            return 0;
        };
        Self::prepared_dirty_skill_modifier(registry, skill)
    }
    pub(crate) fn prepared_dirty_skill_modifier(registry: &EnchantmentRegistry, skill: u32) -> i32 {
        let category = if matches!(skill, 6 | 7 | 15 | 48) {
            686
        } else if matches!(skill,1..=5|8..=13|33|34|41|43..=47|49) {
            684
        } else {
            return 0;
        };
        let flags = 0x10 | 0x8000 | if category == 686 { 0x20000 } else { 0x10000 };
        registry
            .top(category, 0.0)
            .filter(|e| e.spec.stat_type & flags == flags && e.spec.stat_key == 0)
            .map_or(0, |e| e.spec.value.round_ties_even() as i32)
    }
    /// Dirty Fighting healing resistance is a rating, not a skill penalty.
    pub(crate) fn healing_amount_modifier(&self, actor: EntityId) -> f32 {
        let rating = self
            .registry(actor)
            .and_then(|r| r.top(687, 0.0))
            .filter(|e| e.spec.stat_key == 317 && e.spec.stat_type & 0x9004 == 0x9004)
            .map_or(0, |e| e.spec.value.round_ties_even() as i32);
        if rating < 0 {
            (100.0 - rating as f32) / 100.0
        } else {
            100.0 / (100.0 + rating as f32)
        }
    }
    pub(super) fn drain_periodic(&mut self, world: &mut World, policy: &Combat) {
        for _ in 0..self.capacity {
            let Some(front) = self.periodic.front() else {
                break;
            };
            let effect = &front.effects[front.index];
            let (target_id, source, base_amount, kind) =
                (front.target, effect.source, effect.amount, effect.kind);
            if front.applied {
                if front.index + 1 < front.effects.len() {
                    let front = self.periodic.front_mut().expect("front exists");
                    front.index += 1;
                    front.applied = false;
                    continue;
                }
                if !front.expired.is_empty() && self.events.len() == self.capacity {
                    break;
                }
                let completed = self.periodic.pop_front().expect("front exists");
                if !completed.expired.is_empty() {
                    self.events.push_back(MagicEvent::EnchantmentsRemoved {
                        actor: completed.target,
                        entries: completed.expired,
                    });
                }
                continue;
            }
            if self.events.len() == self.capacity
                || self.combat.len() == self.capacity
                || self.registry_reserved(target_id)
            {
                break;
            }
            let Some(target) = world.combatant(target_id) else {
                self.periodic.front_mut().expect("front exists").applied = true;
                continue;
            };
            if target.health() == 0 {
                self.periodic.front_mut().expect("front exists").applied = true;
                continue;
            }
            if matches!(
                kind,
                PeriodicKind::NativeBase | PeriodicKind::NativeNether | PeriodicKind::NativeHealing
            ) {
                if !self.drain_native_periodic(world) {
                    break;
                }
                continue;
            }
            // ACE resolves the damager in the target's current landblock. A source
            // that has left does not keep causing damage; no source is fabricated.
            let Ok((target_cell, _)) = world.actor_state(target_id) else {
                self.periodic.front_mut().expect("front exists").applied = true;
                continue;
            };
            if !world
                .actor_state(source)
                .is_ok_and(|(cell, _)| cell == target_cell)
            {
                self.periodic.front_mut().expect("front exists").applied = true;
                continue;
            }
            if policy
                .spell_permission(
                    source,
                    target_id,
                    kind == PeriodicKind::Healing && base_amount >= 0.0,
                    world,
                )
                .is_err()
            {
                self.periodic.front_mut().expect("front exists").applied = true;
                continue;
            }
            let rating = self.defense_profiles.get(&target_id).map_or(0, |p| {
                specialized_defense_rating(p.player, DefenseKind::Magic, p.magic_defense)
            });
            let defense = self
                .periodic_profiles
                .get(&target_id)
                .copied()
                .unwrap_or_default();
            let amount = match kind {
                PeriodicKind::DirtyFighting => ((base_amount as f32).max(0.0)
                    * (100.0 / (100.0 + rating as f32)))
                    .round_ties_even() as i64,
                PeriodicKind::Damage => base_amount.max(0.0).floor() as i64,
                PeriodicKind::Nether => {
                    let reduction = 100.0
                        / (100.0
                            + f64::from(defense.augmentation_reduction)
                            + f64::from(defense.dot_resistance));
                    let pvp = if target.profile().player {
                        defense.void_player_modifier
                    } else {
                        1.0
                    };
                    (base_amount.max(0.0) * reduction * pvp + 0.0001).floor() as i64
                }
                PeriodicKind::NativeBase
                | PeriodicKind::NativeNether
                | PeriodicKind::NativeHealing => unreachable!("native periodic branch handled"),
                PeriodicKind::Healing => -(base_amount
                    * f64::from(self.healing_amount_modifier(target_id)))
                .round_ties_even() as i64,
            };
            let before = target.health();
            let maximum = target.profile().maximum_health;
            let mutation = VitalMutation {
                actor: target_id,
                vital: EntityVital::Health,
                before,
                after: (i128::from(before) - i128::from(amount)).clamp(0, i128::from(maximum))
                    as u32,
            };
            let Ok(results) = world.apply_vital_batch(&[mutation], Some(source)) else {
                if let Some(clock) = self.registry_clocks.get_mut(&target_id) {
                    clock.error = Some(RegistryError::InvalidEntry);
                }
                break;
            };
            self.periodic.front_mut().expect("front exists").applied = true;
            for result in results {
                let m = result.mutation;
                self.events.push_back(MagicEvent::Vital {
                    incarnation: world.combatant(target_id).map_or(0, |c| c.incarnation()),
                    actor: target_id,
                    vital: m.vital,
                    before: m.before,
                    after: m.after,
                    revision: result.revision,
                });
                if m.after < m.before {
                    self.combat.push_back(CombatEvent::Damage {
                        target_incarnation: world
                            .combatant(target_id)
                            .map_or(0, |c| c.incarnation()),
                        attacker: Some(source),
                        death_blow: None,
                        target: target_id,
                        amount: m.before - m.after,
                        current: m.after,
                        maximum,
                        killed: m.after == 0,
                        revision: result.revision,
                    });
                }
            }
        }
    }
}

impl Magic {
    pub(crate) fn register_periodic_defense(
        &mut self,
        actor: EntityId,
        profile: PeriodicDefenseProfile,
    ) -> Result<(), CastRejection> {
        if !self.registries.contains_key(&actor)
            || !profile.void_player_modifier.is_finite()
            || !(0.0..=1000.0).contains(&profile.void_player_modifier)
        {
            return Err(CastRejection::InvalidState);
        }
        self.periodic_profiles.insert(actor, profile);
        Ok(())
    }
}
