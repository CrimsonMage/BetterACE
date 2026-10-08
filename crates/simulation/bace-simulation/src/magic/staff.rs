//! Pinned SentinelCommands.HandleRun direct registry mutation; no synthetic cast,
//! component use, skill XP or fake spell metadata.
use super::*;
impl Magic {
    pub(crate) fn staff_run(
        &mut self,
        actor: EntityId,
        mode: bace_gameplay_api::staff::StaffRunMode,
        now: f64,
        prepared: Option<&EnchantmentEntry>,
    ) -> Result<String, CastRejection> {
        use bace_gameplay_api::staff::StaffRunMode as Mode;
        const SPELL: u32 = 1644; // Official SpellId.SentinelRun at the pinned baseline.
        self.prepare_registry_time(now)?;
        let registry = self
            .registries
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        let current = registry.entries().iter().find(|entry| entry.spell == SPELL);
        let mode = if mode == Mode::Toggle {
            if current.is_some() {
                Mode::Off
            } else {
                Mode::On
            }
        } else {
            mode
        };
        if mode == Mode::Check {
            return Ok(format!(
                "Run speed boost is currently {}",
                if current.is_some() {
                    "ACTIVE"
                } else {
                    "INACTIVE"
                }
            ));
        }
        if self.registry_reserved(actor) || self.registry_failure(actor).is_some() {
            return Err(CastRejection::Busy);
        }
        if self.events.len() >= self.capacity {
            return Err(CastRejection::Capacity);
        }
        if mode == Mode::Off {
            let Some(entry) = current else {
                return Ok("Run speed boost is currently INACTIVE".into());
            };
            let selected = vec![(entry.spell, entry.spec.layer)];
            self.registries
                .get_mut(&actor)
                .ok_or(CastRejection::MissingActor)?
                .remove(&selected)
                .map_err(|_| CastRejection::InvalidState)?;
            self.events.push_back(MagicEvent::EnchantmentsRemoved {
                actor,
                entries: selected,
            });
            return Ok(String::new());
        }
        let entry = if let Some(entry) = prepared {
            let mut entry = entry.clone();
            if entry.spell != SPELL {
                return Err(CastRejection::MissingAssets);
            }
            entry.caster = actor.0;
            entry
        } else {
            let spell = self
                .spells
                .get(&SPELL)
                .ok_or(CastRejection::MissingAssets)?;
            let SpellEffect::Enchantment(spec) = &spell.spell.effect else {
                return Err(CastRejection::MissingAssets);
            };
            let metadata = *self
                .enchantment_metadata
                .get(&SPELL)
                .ok_or(CastRejection::MissingAssets)?;
            EnchantmentEntry {
                spell: SPELL,
                caster: actor.0,
                school: spell.spell.school,
                spec: spec.clone(),
                start_time: 0.,
                is_set_spell: false,
                is_level8_aura: false,
                metadata,
            }
        };
        let applied = self
            .registries
            .get_mut(&actor)
            .ok_or(CastRejection::MissingActor)?
            .add(entry, now, false)
            .map_err(|_| CastRejection::InvalidState)?;
        self.events.push_back(MagicEvent::Enchantment {
            actor,
            entry: applied.entry,
        });
        Ok("Run forrest, run!".into())
    }
}
impl Magic {
    pub(crate) fn staff_known_spell(&mut self, actor: EntityId, spell: u32, learn: bool) {
        if let Some(caster) = self.casters.get_mut(&actor) {
            if learn {
                caster.known_spells.insert(spell);
            } else {
                caster.known_spells.remove(&spell);
            }
        }
    }
}

impl Magic {
    /// Preflight every registry and output before publishing the first buff.
    pub(crate) fn staff_buffs(
        &mut self,
        caster: EntityId,
        requests: &[(EntityId, u32)],
        now: f64,
        prepared: &std::collections::BTreeMap<u32, EnchantmentEntry>,
    ) -> Result<(), CastRejection> {
        self.prepare_registry_time(now)?;
        if requests.len() > 4096 || requests.len() > self.capacity - self.events.len() {
            return Err(CastRejection::Capacity);
        }
        let mut candidates = std::collections::BTreeMap::new();
        let mut events = Vec::with_capacity(requests.len());
        for &(actor, id) in requests {
            if self.registry_reserved(actor) || self.registry_failure(actor).is_some() {
                return Err(CastRejection::Busy);
            }
            if let std::collections::btree_map::Entry::Vacant(slot) = candidates.entry(actor) {
                let old = self
                    .registries
                    .get(&actor)
                    .ok_or(CastRejection::MissingActor)?;
                slot.insert(
                    EnchantmentRegistry::restore(
                        old.capacity(),
                        old.revision(),
                        old.entries().to_vec(),
                    )
                    .map_err(|_| CastRejection::InvalidState)?,
                );
            }
            let entry = if let Some(entry) = prepared.get(&id) {
                let mut entry = entry.clone();
                entry.caster = caster.0;
                entry
            } else {
                let spell = self.spells.get(&id).ok_or(CastRejection::MissingAssets)?;
                let SpellEffect::Enchantment(spec) = &spell.spell.effect else {
                    return Err(CastRejection::MissingAssets);
                };
                let metadata = *self
                    .enchantment_metadata
                    .get(&id)
                    .ok_or(CastRejection::MissingAssets)?;
                EnchantmentEntry {
                    spell: id,
                    caster: caster.0,
                    school: spell.spell.school,
                    spec: spec.clone(),
                    start_time: 0.,
                    is_set_spell: false,
                    is_level8_aura: false,
                    metadata,
                }
            };
            let applied = candidates
                .get_mut(&actor)
                .expect("inserted candidate")
                .add(entry, now, false)
                .map_err(|_| CastRejection::InvalidState)?;
            events.push(MagicEvent::Enchantment {
                actor,
                entry: applied.entry,
            });
        }
        for (actor, candidate) in candidates {
            self.registries.insert(actor, candidate);
        }
        self.events.extend(events);
        Ok(())
    }
}
