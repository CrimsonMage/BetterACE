//! Bounded item-cast continuations. Kernel validates current inventory ownership
//! before admission; retaining a request never implies a successful proc.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MagicItemProcRequest {
    pub origin: CastOrigin,
    pub target: EntityId,
    pub spell: u32,
    pub parent: Option<CastOrigin>,
    pub depth: u8,
    pub skill_override: Option<u32>,
    pub admitted: bool,
}
impl Magic {
    pub(crate) fn register_damage_spell_flags(
        &mut self,
        spell: u32,
        flags: u32,
    ) -> Result<(), CastRejection> {
        if !self.spells.contains_key(&spell) {
            return Err(CastRejection::UnknownSpell);
        }
        self.damage_spell_flags.insert(spell, flags);
        Ok(())
    }
    pub(crate) fn pending_item_proc_excluding(
        &self,
        inflight: &[CastOrigin],
    ) -> Option<MagicItemProcRequest> {
        self.item_procs
            .iter()
            .enumerate()
            .filter(|(_, p)| !inflight.contains(&p.origin))
            .max_by_key(|(index, p)| (p.depth, std::cmp::Reverse(*index)))
            .map(|(_, p)| *p)
    }
    pub(crate) fn item_proc_request(&self, origin: CastOrigin) -> Option<MagicItemProcRequest> {
        self.item_procs.iter().find(|p| p.origin == origin).copied()
    }
    pub(super) fn proc_parent(&self, origin: CastOrigin) -> Option<(CastOrigin, u8)> {
        self.item_proc_request(origin).map(|p| (origin, p.depth))
    }
    pub(crate) fn confirm_item_proc(
        &mut self,
        expected: MagicItemProcRequest,
    ) -> Result<(), CastRejection> {
        let index = self
            .item_procs
            .iter()
            .position(|p| *p == expected)
            .ok_or(CastRejection::InvalidState)?;
        self.item_procs.remove(index);
        if let Some((_, event)) = expected.origin.server_event()
            && self
                .physical_procs
                .values()
                .any(|p| p.waits.contains(&event))
        {
            self.completed_item_procs.insert(event);
        }
        if let CastOrigin::ItemProc { event, .. } = expected.origin
            && self.periodic.iter().any(|p| {
                p.native_pending
                    .is_some_and(|(_, wait)| wait == Some(event))
            })
        {
            self.completed_item_procs.insert(event);
        }

        if let CastOrigin::ItemProc { event, .. } = expected.origin
            && self
                .attempts
                .values()
                .chain(self.instant_continuations.values())
                .any(|a| {
                    a.pending_direct
                        .as_ref()
                        .is_some_and(|d| d.cloak_wait == Some(event))
                })
        {
            self.completed_item_procs.insert(event);
        }

        if let CastOrigin::ItemProc { event, .. } = expected.origin
            && self.flying.values().any(|f| {
                f.pending_damage
                    .as_ref()
                    .is_some_and(|d| d.cloak_wait == Some(event))
            })
        {
            self.completed_item_procs.insert(event);
        }
        Ok(())
    }
    pub(super) fn proc_involves(&self, actor: EntityId) -> bool {
        self.physical_procs
            .keys()
            .any(|key| key.attacker == actor || key.target == actor)
            || self.item_procs.iter().any(|p| {
                p.origin.actor() == actor
                    || p.target == actor
                    || matches!(p.origin,CastOrigin::ItemProc{item,..}if item==actor)
            })
    }
    pub(super) fn proc_capacity(&self) -> bool {
        self.item_procs.iter().filter(|p| p.depth == 0).count() + 4 <= self.capacity.max(4)
            && self.item_procs.len() + 4 <= self.capacity.max(4) + 16
            && self.next_item_proc <= u64::MAX - 4
    }
    pub(super) fn queue_item_proc(
        &mut self,
        actor: EntityId,
        proc: bace_magic::MagicItemProc,
        parent: Option<(CastOrigin, u8)>,
    ) -> Option<u64> {
        let depth = parent.map_or(0, |(_, depth)| depth.saturating_add(1));
        if depth >= 16
            || self.item_procs.len() >= self.capacity.max(4) + 16
            || self.next_item_proc == u64::MAX
        {
            self.events.push_back(MagicEvent::TargetRejected {
                actor,
                target: EntityId(proc.target),
                spell: proc.spell,
                reason: CastRejection::Capacity,
                notice: None,
            });
            return None;
        }
        self.next_item_proc += 1;
        self.item_procs.push_back(MagicItemProcRequest {
            origin: CastOrigin::ItemProc {
                actor,
                item: EntityId(proc.item),
                event: self.next_item_proc,
            },
            target: EntityId(proc.target),
            spell: proc.spell,
            parent: parent.map(|p| p.0),
            depth,
            skill_override: None,
            admitted: false,
        });
        Some(self.next_item_proc)
    }
    pub(super) fn sigil_procs(
        &self,
        source: EntityId,
        target: Option<EntityId>,
        random: &mut RandomStream,
    ) -> Result<Vec<bace_magic::MagicItemProc>, CastRejection> {
        let Some(profile) = self.damage_profiles.get(&source).filter(|p| p.player) else {
            return Ok(Vec::new());
        };
        if profile.sigils.is_empty() {
            return Ok(Vec::new());
        }
        let draws = [
            f64::from(unit(random)?),
            f64::from(unit(random)?),
            f64::from(unit(random)?),
        ];
        bace_magic::magic_sigil_procs(
            &profile.sigils,
            source.0,
            target.map(|t| t.0),
            &draws,
            bace_magic::MagicProcPolicy::default(),
            |spell| self.damage_spell_flags.get(&spell).copied(),
        )
        .map_err(|_| CastRejection::InvalidState)
    }
    pub(super) fn cloak_proc(
        &self,
        source: EntityId,
        target: EntityId,
        damage: u32,
        world: &World,
        random: &mut RandomStream,
    ) -> Result<bace_magic::MagicCloakResult, CastRejection> {
        let Some(profile) = self.damage_profiles.get(&target).filter(|p| p.player) else {
            return Ok(bace_magic::MagicCloakResult { damage, cast: None });
        };
        let Some(cloak) = profile.cloak else {
            return Ok(bace_magic::MagicCloakResult { damage, cast: None });
        };
        let current_health = world
            .vital(target, EntityVital::Health)
            .map_err(|_| CastRejection::MissingActor)?
            .current;
        let pvp = self.damage_profiles.get(&source).is_some_and(|p| p.player);
        bace_magic::magic_cloak_proc(bace_magic::MagicCloakInput {
            cloak,
            damage,
            current_health,
            player_vs_player: pvp,
            owner: target.0,
            current_enemy: profile.current_enemy,
            roll: f64::from(unit(random)?),
            policy: bace_magic::MagicProcPolicy::default(),
        })
        .map_err(|_| CastRejection::InvalidState)
    }
    pub(crate) fn set_damage_target(&mut self, actor: EntityId, target: Option<EntityId>) {
        if let Some(p) = self.damage_profiles.get_mut(&actor) {
            p.current_enemy = target.map(|t| t.0);
        }
    }
}
impl Magic {
    pub(super) fn commit_projectile_damage(
        &mut self,
        flying: &mut Flying,
        world: &mut World,
        now: f64,
    ) -> bool {
        let Some(pending) = flying.pending_damage.as_ref() else {
            return true;
        };
        if pending
            .cloak_wait
            .is_some_and(|event| !self.completed_item_procs.contains(&event))
            || self.events.len() + 2 > self.capacity
            || self.combat.len() >= self.capacity
            || !self.proc_capacity()
        {
            return false;
        }
        if self.registry_reserved(pending.target) {
            return false;
        }
        if world
            .combatant(pending.target)
            .is_some_and(|c| c.health() == 0)
        {
            if let Some(wait) = pending.cloak_wait {
                self.completed_item_procs.remove(&wait);
            }
            flying.pending_damage = None;
            return true;
        }
        let spec = match &flying.spell.spell.effect {
            SpellEffect::Projectile(p) | SpellEffect::LifeProjectile { projectile: p, .. } => p,
            _ => return false,
        };
        let enchantment = if let Some(enchantment) = &spec.enchantment {
            let Some(registry) = self.registries.get(&pending.target) else {
                return false;
            };
            let metadata = self.metadata_for(flying.spell.spell.id, enchantment);
            match registry.propose_add(
                EnchantmentEntry {
                    spell: flying.spell.spell.id,
                    caster: flying.source.0,
                    school: flying.spell.spell.school,
                    spec: enchantment.clone(),
                    start_time: 0.,
                    is_set_spell: enchantment.set_id.is_some(),
                    is_level8_aura: false,
                    metadata,
                },
                now,
                false,
            ) {
                Ok(proposal) => Some(proposal),
                Err(RegistryError::Capacity | RegistryError::OutputCapacity) => return false,
                Err(error) => {
                    if let Some(clock) = self.registry_clocks.get_mut(&pending.target) {
                        clock.error = Some(error);
                    }
                    return false;
                }
            }
        } else {
            None
        };
        let Ok(vital) = world.vital(pending.target, pending.vital) else {
            return false;
        };
        let mutation = VitalMutation {
            actor: pending.target,
            vital: pending.vital,
            before: vital.current,
            after: vital.current.saturating_sub(pending.damage),
        };
        let results = if pending.apply_damage {
            let Ok(results) = world.apply_vital_batch(&[mutation], Some(flying.credited_owner))
            else {
                return false;
            };
            results
        } else {
            Vec::new()
        };
        let pending = flying
            .pending_damage
            .take()
            .expect("checked damage continuation");
        if let Some(event) = pending.cloak_wait {
            self.completed_item_procs.remove(&event);
        }
        for result in results {
            let m = result.mutation;
            self.events.push_back(MagicEvent::Vital {
                incarnation: world
                    .combatant(pending.target)
                    .map_or(0, |c| c.incarnation()),
                actor: pending.target,
                vital: m.vital,
                before: m.before,
                after: m.after,
                revision: result.revision,
            });
            if pending.vital == EntityVital::Health {
                self.combat.push_back(CombatEvent::Damage {
                    target_incarnation: world
                        .combatant(pending.target)
                        .map_or(0, |c| c.incarnation()),
                    attacker: Some(flying.source),
                    death_blow: None,
                    target: pending.target,
                    amount: m.before - m.after,
                    current: m.after,
                    maximum: vital.maximum,
                    killed: m.after == 0,
                    revision: result.revision,
                });
            }
        }
        if let Some(proposal) = enchantment {
            let update = self
                .registries
                .get_mut(&pending.target)
                .expect("preflighted item registry")
                .adopt(proposal)
                .expect("single-owner registry remained unchanged during vital commit");
            self.events.push_back(MagicEvent::Enchantment {
                actor: pending.target,
                entry: update.entry,
            });
        }
        for proc in pending.sigils {
            self.queue_item_proc(flying.source, proc, flying.proc_parent);
        }
        true
    }
}

impl Magic {
    pub(super) fn effective_cast_skill(
        &self,
        origin: CastOrigin,
        spell: &PreparedSpell,
        world: &World,
    ) -> Result<u32, CastRejection> {
        if let Some(skill) = self
            .item_proc_request(origin)
            .and_then(|request| request.skill_override)
        {
            return Ok(skill);
        }
        if self
            .spell_categories
            .get(&spell.id)
            .is_some_and(|category| (683..=686).contains(category))
            || matches!(&spell.effect, SpellEffect::Enchantment(e) if (683..=686).contains(&e.category))
        {
            return Ok(1);
        }
        if let CastOrigin::ItemProc { actor, item, .. } = origin
            && let Some(profile) = self.damage_profiles.get(&actor)
        {
            let prepared = profile
                .proc_items
                .iter()
                .find(|p| p.item == item.0)
                .ok_or(CastRejection::MissingAssets)?;
            let mut level = bace_magic::magic_item_skill(prepared, Some(spell.school), 0);
            if prepared.spellcraft.is_some()
                && let Some(registry) = self.registries.get(&item)
            {
                level =
                    bace_magic::enchant_physical_quality(registry, 4, 106, f64::from(level), true)
                        .map_err(|_| CastRejection::InvalidState)?
                        .1 as u32;
            }
            return Ok(level);
        }
        let actor = origin.actor();
        let spellcraft = if let Some(properties) = world.properties(actor) {
            match properties.get(bace_entity::PropertyFamily::Int, 106) {
                Some(bace_entity::PropertyValue::Int(value)) => {
                    Some(u32::try_from(*value).map_err(|_| CastRejection::InvalidState)?)
                }
                _ => None,
            }
        } else {
            self.damage_profiles.get(&actor).and_then(|p| p.spellcraft)
        };
        if let Some(mut skill) = spellcraft {
            if let Some(registry) = self.registries.get(&actor) {
                skill =
                    bace_magic::enchant_physical_quality(registry, 4, 106, f64::from(skill), true)
                        .map_err(|_| CastRejection::InvalidState)?
                        .1 as u32;
            }
            return Ok(skill);
        }
        self.casters
            .get(&origin.actor())
            .map(|c| c.school_skills[spell.school as usize - 1])
            .ok_or(CastRejection::MissingAssets)
    }
}
