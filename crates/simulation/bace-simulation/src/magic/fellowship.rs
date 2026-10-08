//! Fellowship fanout reads live membership at release. One mana debit and one
//! preflighted owner transaction apply all eligible recipients; no copied roster.
use super::*;
pub(super) struct FellowshipRelease<'a> {
    pub now: f64,
    pub cost: u32,
    pub random: &'a RandomStream,
    pub observers: Option<(&'a crate::characters::Characters, u64)>,
}
impl Magic {
    pub(super) fn release_fellowship(
        &mut self,
        attempt: &Attempt,
        world: &mut World,
        policy: &Combat,
        members: &[EntityId],
        plan: FellowshipRelease<'_>,
    ) -> Result<bool, CastRejection> {
        let FellowshipRelease {
            now,
            cost,
            random,
            observers,
        } = plan;
        if members.is_empty() || members.len() > 9 {
            return Err(CastRejection::InvalidTarget);
        }
        if members
            .iter()
            .enumerate()
            .any(|(i, id)| id.0 == 0 || members[..i].contains(id))
        {
            return Err(CastRejection::InvalidState);
        }
        let actor = attempt.origin.actor();
        let target = attempt.target.unwrap_or(actor);
        let (target_cell, _) = world
            .actor_state(target)
            .map_err(|_| CastRejection::InvalidTarget)?;
        let recipients: Vec<_> = members
            .iter()
            .copied()
            .filter(|id| {
                world
                    .actor_state(*id)
                    .is_ok_and(|(cell, _)| cell.0 >> 16 == target_cell.0 >> 16)
                    && world.combatant(*id).is_some_and(|c| c.health() > 0)
            })
            .collect();
        if self.events.len() + recipients.len() + 1 > self.capacity
            || self.combat.len() + recipients.len() > self.capacity
        {
            return Ok(false);
        }
        if recipients.iter().any(|id| {
            self.registry_clocks
                .get(id)
                .is_some_and(|c| c.reserved || c.error.is_some())
        }) {
            return Ok(false);
        }
        let mut accepted = Vec::new();
        let mut denied = Vec::new();
        let spell = &attempt.prepared.spell;
        for id in recipients {
            match self.spell_permission(policy, actor, id, helpful_spell(spell), world) {
                Ok(()) => {
                    if spell.resistable && id != actor {
                        let mut stream = random
                            .fork(b"fellow_resist", u64::from(id.0))
                            .map_err(|_| CastRejection::InvalidState)?;
                        let skill = attempt.cast_skill;
                        let defense = self.casters.get(&id).map_or(0, |c| c.magic_defense);
                        if bace_magic::resisted(skill, defense, unit(&mut stream)?)
                            .map_err(|_| CastRejection::InvalidState)?
                            .0
                        {
                            denied.push((id, CastRejection::Resisted));
                            continue;
                        }
                    }
                    accepted.push(id);
                }
                Err(reason) => denied.push((id, reason)),
            }
        }
        match &spell.effect {
            SpellEffect::FellowshipBoost {
                vital,
                minimum,
                maximum,
            } => {
                let mut changes = Vec::new();
                for id in accepted {
                    let mut draw = random
                        .fork(b"fellow", u64::from(id.0))
                        .map_err(|_| CastRejection::InvalidState)?;
                    let pool = vital_after_cost(world, id, entity_vital(*vital), actor, cost)?;
                    let change = if self.damage_profiles.contains_key(&actor) {
                        self.native_boost_heal(
                            attempt.origin,
                            id,
                            *vital,
                            (*minimum, *maximum),
                            &mut draw,
                            VitalState {
                                current: pool.current,
                                maximum: pool.maximum,
                            },
                        )?
                    } else {
                        let roll = roll_range(&mut draw, *minimum, *maximum)?;
                        bace_magic::boost(
                            VitalState {
                                current: pool.current,
                                maximum: pool.maximum,
                            },
                            *minimum,
                            *maximum,
                            roll,
                            if roll > 0 {
                                f64::from(self.healing_amount_modifier(id))
                            } else {
                                1.0
                            },
                        )
                        .map_err(|_| CastRejection::InvalidState)?
                    };
                    changes.push(VitalMutation {
                        actor: id,
                        vital: entity_vital(*vital),
                        before: change.before,
                        after: change.after,
                    });
                }
                self.commit_vitals(world, actor, cost, &changes)?;
            }
            SpellEffect::FellowshipEnchantment(spec) => {
                let mut proposals = Vec::new();
                for id in accepted {
                    let registry = self
                        .registries
                        .get(&id)
                        .ok_or(CastRejection::MissingAssets)?;
                    let entry = EnchantmentEntry {
                        spell: spell.id,
                        caster: actor.0,
                        school: spell.school,
                        spec: spec.clone(),
                        start_time: 0.0,
                        is_set_spell: spec.set_id.is_some(),
                        is_level8_aura: false,
                        metadata: self.metadata_for(spell.id, spec),
                    };
                    proposals.push((
                        id,
                        registry
                            .propose_add(entry, now, false)
                            .map_err(|_| CastRejection::Capacity)?,
                    ));
                }
                self.apply_mana(world, actor, cost)?;
                for (id, proposal) in proposals {
                    if self
                        .damage_spell_flags
                        .get(&spell.id)
                        .is_some_and(|flags| flags & 4 == 0)
                    {
                        pk_activity::enchantment(world, policy, actor, id, now);
                    }
                    let update = self
                        .registries
                        .get_mut(&id)
                        .expect("preflighted member")
                        .adopt(proposal)
                        .expect("single-owner fellowship mutation");
                    self.events.push_back(MagicEvent::Enchantment {
                        actor: id,
                        entry: update.entry,
                    });
                }
            }
            SpellEffect::FellowshipDispel(spec) => {
                let mut removals = Vec::new();
                for id in accepted {
                    let registry = self
                        .registries
                        .get(&id)
                        .ok_or(CastRejection::MissingAssets)?;
                    let mut entries = Vec::with_capacity(registry.entries().len());
                    registry
                        .dispel_candidates(spec, &mut entries)
                        .map_err(|_| CastRejection::InvalidState)?;
                    let mut draw = random
                        .fork(b"fellow", u64::from(id.0))
                        .map_err(|_| CastRejection::InvalidState)?;
                    for i in (1..entries.len()).rev() {
                        let j = draw
                            .below((i + 1) as u64)
                            .map_err(|_| CastRejection::InvalidState)?
                            as usize;
                        entries.swap(i, j);
                    }
                    entries.truncate(spec.count as usize);
                    if !entries.is_empty() && registry.revision() == u64::MAX {
                        return Err(CastRejection::InvalidState);
                    }
                    removals.push((id, entries));
                }
                self.apply_mana(world, actor, cost)?;
                for (id, entries) in removals {
                    self.registries
                        .get_mut(&id)
                        .expect("preflighted member")
                        .remove(&entries)
                        .expect("single-owner fellowship removal");
                    if !entries.is_empty() {
                        self.events
                            .push_back(MagicEvent::EnchantmentsRemoved { actor: id, entries });
                    }
                }
            }
            _ => return Err(CastRejection::InvalidState),
        }
        for (target, reason) in denied {
            self.target_rejected(actor, target, spell.id, reason, world, observers);
        }
        Ok(true)
    }
}
