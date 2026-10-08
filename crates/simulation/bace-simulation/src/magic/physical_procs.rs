//! Source-ordered physical proc continuations. Combat owns the frozen contact;
//! Magic retains each exact phase until Combat acknowledges its receipt.
use super::*;
use bace_gameplay_api::physical_procs::{PhysicalHitKey, PhysicalProcReceipt, PhysicalProcRequest};

pub(super) struct PendingPhysicalProc {
    request: PhysicalProcRequest,
    pub(super) waits: Vec<u64>,
    sigils_pending: bool,
    damage: Option<u32>,
}
impl Magic {
    pub(crate) fn advance_physical_proc(
        &mut self,
        request: &PhysicalProcRequest,
        world: &World,
    ) -> Result<Option<PhysicalProcReceipt>, CastRejection> {
        let key = request.key();
        validate(request)?;
        if let Some(pending) = self.physical_procs.get(&key) {
            if pending.request != *request {
                return Err(CastRejection::InvalidState);
            }
        } else {
            if self.physical_procs.len() >= self.capacity || !self.proc_capacity() {
                return Err(CastRejection::Capacity);
            }
            let mut pending = PendingPhysicalProc {
                request: request.clone(),
                waits: Vec::with_capacity(3),
                sigils_pending: matches!(request, PhysicalProcRequest::Attack { .. }),
                damage: None,
            };
            match request {
                PhysicalProcRequest::Attack { weapon, .. } => {
                    if let Some((item, spell)) = weapon {
                        self.queue_physical_spell(&mut pending, Some(*item), *spell);
                    }
                }
                PhysicalProcRequest::Dirty {
                    weapon,
                    spells,
                    count,
                    ..
                } => {
                    for spell in &spells[..*count] {
                        self.queue_physical_spell(&mut pending, *weapon, *spell);
                    }
                }
                PhysicalProcRequest::Cloak { damage, roll, .. } => {
                    let result = self.physical_cloak(key, *damage, *roll, world)?;
                    pending.damage = Some(result.damage);
                    if let Some(proc) = result.cast
                        && let Some(wait) = self.queue_item_proc(key.target, proc, None)
                    {
                        pending.waits.push(wait);
                    }
                }
            }
            self.physical_procs.insert(key, pending);
        }
        let pending = &self.physical_procs[&key];
        if pending
            .waits
            .iter()
            .any(|id| !self.completed_item_procs.contains(id))
        {
            return Ok(None);
        }
        if pending.sigils_pending {
            if !self.proc_capacity() {
                return Ok(None);
            }
            let PhysicalProcRequest::Attack {
                sigil_target,
                sigil_rolls,
                ..
            } = request
            else {
                return Err(CastRejection::InvalidState);
            };
            let procs = match self.damage_profiles.get(&key.attacker).filter(|p| p.player) {
                Some(profile) => bace_magic::magic_sigil_procs(
                    &profile.sigils,
                    key.attacker.0,
                    sigil_target.map(|target| target.0),
                    sigil_rolls,
                    bace_magic::MagicProcPolicy::default(),
                    |spell| self.damage_spell_flags.get(&spell).copied(),
                )
                .map_err(|_| CastRejection::InvalidState)?,
                None => Vec::new(),
            };
            let mut pending = self
                .physical_procs
                .remove(&key)
                .ok_or(CastRejection::InvalidState)?;
            for wait in pending.waits.drain(..) {
                self.completed_item_procs.remove(&wait);
            }
            pending.sigils_pending = false;
            for proc in procs {
                if let Some(wait) = self.queue_item_proc(key.attacker, proc, None) {
                    pending.waits.push(wait);
                }
            }
            self.physical_procs.insert(key, pending);
        }
        let pending = &self.physical_procs[&key];
        Ok(pending
            .waits
            .iter()
            .all(|id| self.completed_item_procs.contains(id))
            .then_some(PhysicalProcReceipt {
                key,
                phase: request.phase(),
                damage: pending.damage,
            }))
    }
    pub(crate) fn acknowledge_physical_proc(
        &mut self,
        receipt: &PhysicalProcReceipt,
    ) -> Result<(), CastRejection> {
        let pending = self
            .physical_procs
            .get(&receipt.key)
            .ok_or(CastRejection::InvalidState)?;
        if pending.request.phase() != receipt.phase
            || pending.damage != receipt.damage
            || pending.sigils_pending
            || pending
                .waits
                .iter()
                .any(|id| !self.completed_item_procs.contains(id))
        {
            return Err(CastRejection::InvalidState);
        }
        let pending = self
            .physical_procs
            .remove(&receipt.key)
            .ok_or(CastRejection::InvalidState)?;
        for wait in pending.waits {
            self.completed_item_procs.remove(&wait);
        }
        Ok(())
    }
    fn queue_physical_spell(
        &mut self,
        pending: &mut PendingPhysicalProc,
        item: Option<EntityId>,
        spell: u32,
    ) {
        let key = pending.request.key();
        if let Some(item) = item {
            if let Some(wait) = self.queue_item_proc(
                key.attacker,
                bace_magic::MagicItemProc {
                    item: item.0,
                    target: key.target.0,
                    spell,
                },
                None,
            ) {
                if matches!(pending.request, PhysicalProcRequest::Dirty { .. })
                    && key.kind == bace_gameplay_api::weapon_combat::PhysicalKind::Missile
                {
                    // Ammunition calls weapon.TryCastSpell; its Dirty-specific
                    // TryBeginCast branch overrides Resolve's skill with zero.
                    self.item_procs
                        .back_mut()
                        .expect("new exact item proc")
                        .skill_override = Some(0);
                }
                pending.waits.push(wait);
            }
        } else {
            // Dirty Fighting without a weapon is an actual actor spell source.
            self.next_item_proc += 1;
            let event = self.next_item_proc;
            self.item_procs.push_back(MagicItemProcRequest {
                origin: CastOrigin::PhysicalProc {
                    actor: key.attacker,
                    event,
                },
                target: key.target,
                spell,
                parent: None,
                depth: 0,
                skill_override: None,
                admitted: false,
            });
            pending.waits.push(event);
        }
    }
    fn physical_cloak(
        &self,
        key: PhysicalHitKey,
        damage: u32,
        roll: f64,
        world: &World,
    ) -> Result<bace_magic::MagicCloakResult, CastRejection> {
        let unchanged = bace_magic::MagicCloakResult { damage, cast: None };
        let Some(profile) = self.damage_profiles.get(&key.target).filter(|p| p.player) else {
            return Ok(unchanged);
        };
        let Some(cloak) = profile.cloak else {
            return Ok(unchanged);
        };
        let current_health = world
            .vital(key.target, EntityVital::Health)
            .map_err(|_| CastRejection::MissingActor)?
            .current;
        // A preceding weapon/sigil spell may already have killed the target.
        if current_health == 0 {
            return Ok(unchanged);
        }
        bace_magic::magic_cloak_proc(bace_magic::MagicCloakInput {
            cloak,
            damage,
            current_health,
            player_vs_player: self
                .damage_profiles
                .get(&key.attacker)
                .is_some_and(|p| p.player),
            owner: key.target.0,
            current_enemy: profile.current_enemy,
            roll,
            policy: bace_magic::MagicProcPolicy::default(),
        })
        .map_err(|_| CastRejection::InvalidState)
    }
}
fn validate(request: &PhysicalProcRequest) -> Result<(), CastRejection> {
    let key = request.key();
    if key.attacker.0 == 0 || key.target.0 == 0 || key.operation == 0 {
        return Err(CastRejection::InvalidState);
    }
    let valid_roll = |roll: &f64| roll.is_finite() && (0.0..1.0).contains(roll);
    let valid = match request {
        PhysicalProcRequest::Attack {
            weapon,
            sigil_target,
            sigil_rolls,
            ..
        } => {
            weapon.is_none_or(|(item, spell)| item.0 != 0 && spell != 0)
                && sigil_target.is_none_or(|target| target == key.target)
                && sigil_rolls.iter().all(valid_roll)
        }
        PhysicalProcRequest::Dirty {
            weapon,
            spells,
            count,
            ..
        } => {
            *count <= 2
                && weapon.is_none_or(|item| item.0 != 0)
                && spells[..(*count).min(2)].iter().all(|spell| *spell != 0)
        }
        PhysicalProcRequest::Cloak { roll, .. } => valid_roll(roll),
    };
    if valid {
        Ok(())
    } else {
        Err(CastRejection::InvalidState)
    }
}
