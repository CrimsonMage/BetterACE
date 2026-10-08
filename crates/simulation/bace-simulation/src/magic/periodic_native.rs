//! Selected GDLE periodic policy for complete native damage profiles. Legacy
//! synthetic projections retain separately qualified ACE fixture behavior.
use super::periodic::{PeriodicEffect, PeriodicKind, PeriodicPulse};
use super::*;
pub(super) fn native_pulse(
    actor: EntityId,
    registry: &EnchantmentRegistry,
    nonce: u64,
    flags: impl Fn(u32) -> Option<u32>,
) -> Result<Option<PeriodicPulse>, RegistryError> {
    let effects = bace_magic::gdle_periodic_batches(registry, flags)
        .map_err(|_| RegistryError::InvalidEntry)?
        .into_iter()
        .map(|e| PeriodicEffect {
            source: EntityId(e.source.unwrap_or(0)),
            amount: f64::from(e.amount),
            kind: match e.kind {
                bace_magic::NativePeriodicKind::Base => PeriodicKind::NativeBase,
                bace_magic::NativePeriodicKind::Nether => PeriodicKind::NativeNether,
                bace_magic::NativePeriodicKind::Healing => PeriodicKind::NativeHealing,
            },
        })
        .collect::<Vec<_>>();
    Ok((!effects.is_empty()).then_some(PeriodicPulse {
        target: actor,
        effects,
        index: 0,
        expired: Vec::new(),
        applied: false,
        native_pending: None,
        nonce,
    }))
}
impl Magic {
    pub(super) fn drain_native_periodic(&mut self, world: &mut World) -> bool {
        let Some(front) = self.periodic.front() else {
            return true;
        };
        let effect = &front.effects[front.index];
        let actor = front.target;
        let source = world
            .actor_state(effect.source)
            .is_ok()
            .then_some(effect.source);
        let healing = effect.kind == PeriodicKind::NativeHealing;
        let kind = effect.kind;
        let base_amount = effect.amount;
        let pending = front.native_pending;
        let nonce = front.nonce;
        let index = front.index;
        let result = (|| -> Result<bool, CastRejection> {
            if !healing && pending.is_none() {
                if !self.proc_capacity() {
                    return Ok(false);
                }
                let profile = self
                    .damage_profiles
                    .get(&actor)
                    .ok_or(CastRejection::MissingAssets)?;
                let raw = profile.dot_ratings;
                let registry = self
                    .registries
                    .get(&actor)
                    .ok_or(CastRejection::MissingAssets)?;
                let augmentation =
                    bace_magic::enchant_physical_quality(registry, 4, 310, f64::from(raw[0]), true)
                        .map_err(|_| CastRejection::InvalidState)?
                        .1 as u32;
                let dot =
                    bace_magic::enchant_physical_quality(registry, 4, 350, f64::from(raw[1]), true)
                        .map_err(|_| CastRejection::InvalidState)?
                        .1 as u32;
                let policy = self
                    .periodic_profiles
                    .get(&actor)
                    .copied()
                    .unwrap_or_default();
                let amount = bace_magic::gdle_periodic_damage(
                    base_amount as u32,
                    kind == PeriodicKind::NativeNether,
                    augmentation,
                    dot,
                    profile.player,
                    policy.void_player_modifier,
                )
                .map_err(|_| CastRejection::InvalidState)?;
                let cloak = if profile.cloak.is_some() {
                    let mut identity = [0u8; 16];
                    identity[..4].copy_from_slice(&actor.0.to_le_bytes());
                    identity[4..12].copy_from_slice(&nonce.to_le_bytes());
                    identity[12..].copy_from_slice(&(index as u32).to_le_bytes());
                    let mut random = self
                        .random
                        .as_ref()
                        .ok_or(CastRejection::MissingAssets)?
                        .event_stream(identity, Domain::Magic)
                        .and_then(|s| s.fork(b"periodic_proc", self.execution_epoch))
                        .map_err(|_| CastRejection::InvalidState)?;
                    // GDLE CheckForTickingDots leaves DamageEventData.isPvP false.
                    self.cloak_proc(EntityId(0), actor, amount, world, &mut random)?
                } else {
                    bace_magic::MagicCloakResult {
                        damage: amount,
                        cast: None,
                    }
                };
                let wait = cloak
                    .cast
                    .and_then(|proc| self.queue_item_proc(actor, proc, None));
                self.periodic
                    .front_mut()
                    .expect("front exists")
                    .native_pending = Some((cloak.damage, wait));
            }
            let front = self.periodic.front().expect("front retained");
            if front.native_pending.is_some_and(|(_, wait)| {
                wait.is_some_and(|event| !self.completed_item_procs.contains(&event))
            }) {
                return Ok(false);
            }
            let amount = if healing {
                base_amount as u32
            } else {
                front.native_pending.expect("prepared native hit").0
            };
            let vital = world
                .vital(actor, EntityVital::Health)
                .map_err(|_| CastRejection::MissingActor)?;
            let after = if healing {
                vital.current.saturating_add(amount).min(vital.maximum)
            } else {
                vital.current.saturating_sub(amount)
            };
            let results = world
                .apply_vital_batch(
                    &[VitalMutation {
                        actor,
                        vital: EntityVital::Health,
                        before: vital.current,
                        after,
                    }],
                    source,
                )
                .map_err(|_| CastRejection::Busy)?;
            let front = self.periodic.front_mut().expect("front retained");
            if let Some((_, Some(event))) = front.native_pending.take() {
                self.completed_item_procs.remove(&event);
            }
            front.applied = true;
            for result in results {
                let m = result.mutation;
                self.events.push_back(MagicEvent::Vital {
                    incarnation: world.combatant(actor).map_or(0, |c| c.incarnation()),
                    actor,
                    vital: m.vital,
                    before: m.before,
                    after: m.after,
                    revision: result.revision,
                });
                if m.after < m.before {
                    self.combat.push_back(CombatEvent::Damage {
                        target_incarnation: world.combatant(actor).map_or(0, |c| c.incarnation()),
                        attacker: source,
                        death_blow: None,
                        target: actor,
                        amount: m.before - m.after,
                        current: m.after,
                        maximum: vital.maximum,
                        killed: m.after == 0,
                        revision: result.revision,
                    });
                }
            }
            Ok(true)
        })();
        match result {
            Ok(applied) => applied,
            Err(CastRejection::Busy | CastRejection::Capacity) => false,
            Err(_) => {
                if let Some(clock) = self.registry_clocks.get_mut(&actor) {
                    clock.error = Some(RegistryError::InvalidEntry);
                }
                false
            }
        }
    }
}
