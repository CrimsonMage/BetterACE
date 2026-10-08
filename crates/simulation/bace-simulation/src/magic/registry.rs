//! Registry ownership and active-time scheduling. No persistence or wall clocks.
use super::*;

pub(super) struct RegistryClock {
    pub active: bool,
    pub reserved: bool,
    pub next_due: f64,
    pub error: Option<RegistryError>,
}

impl Magic {
    /// The caller authenticates/adopts the target identity. Items need neither
    /// mana nor caster capabilities. Rejection returns the complete registry.
    pub(crate) fn register_registry(
        &mut self,
        actor: EntityId,
        registry: EnchantmentRegistry,
        active: bool,
        now: f64,
    ) -> Result<(), (CastRejection, EnchantmentRegistry)> {
        let error = if actor.0 == 0 || actor.0 == u32::MAX || self.registries.contains_key(&actor) {
            Some(CastRejection::InvalidState)
        } else if self.registries.len() >= 4096 {
            Some(CastRejection::Capacity)
        } else if !now.is_finite()
            || now < self.current_time
            || !(now + 5.0).is_finite()
            || now + 5.0 <= now
        {
            Some(CastRejection::InvalidState)
        } else {
            None
        };
        if let Some(error) = error {
            return Err((error, registry));
        }
        self.registries.insert(actor, registry);
        self.registry_clocks.insert(
            actor,
            RegistryClock {
                active,
                reserved: false,
                next_due: now + 5.0,
                error: None,
            },
        );
        Ok(())
    }

    pub(crate) fn registry_reserved(&self, actor: EntityId) -> bool {
        self.registry_clocks
            .get(&actor)
            .is_some_and(|clock| clock.reserved)
    }

    pub(crate) fn has_caster(&self, actor: EntityId) -> bool {
        self.casters.contains_key(&actor)
    }

    /// Refresh prepared skill values without replacing ownership or spell state.
    pub(crate) fn refresh_caster_skills(
        &mut self,
        actor: EntityId,
        school_skills: [u32; 5],
        magic_defense: u32,
        mana_conversion: u32,
    ) -> Result<(), CastRejection> {
        if school_skills
            .iter()
            .chain([&magic_defense, &mana_conversion])
            .any(|v| *v > i32::MAX as u32)
        {
            return Err(CastRejection::InvalidState);
        }
        let caster = self
            .casters
            .get_mut(&actor)
            .ok_or(CastRejection::MissingActor)?;
        caster.school_skills = school_skills;
        caster.magic_defense = magic_defense;
        caster.mana_conversion = mana_conversion;
        Ok(())
    }

    /// Mark an already-admitted target active/inactive. Adapters pass root
    /// activity for contained items; an item has exactly one clock here.
    pub(crate) fn set_registry_active(
        &mut self,
        actor: EntityId,
        active: bool,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        self.heartbeat(now);
        let clock = self
            .registry_clocks
            .get_mut(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if clock.reserved || clock.error.is_some() || clock.active && clock.next_due <= now {
            return Err(CastRejection::Busy);
        }
        if clock.active != active {
            clock.active = active;
            clock.next_due = now + 5.0;
        }
        Ok(())
    }

    /// Freeze registry mutations while an owning valuable operation is pending.
    /// Active time remains owed and is applied after release, including rollback.
    pub(crate) fn reserve_registry(
        &mut self,
        actor: EntityId,
        reserved: bool,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        if reserved && self.periodic_involves(actor) {
            return Err(CastRejection::Busy);
        }
        if reserved
            && (self.attempts.values().any(|a| {
                let involved = a.origin.actor() == actor || attempt_target(a) == Some(actor);
                let waiting_release = (a.origin.actor() == actor
                    && a.component_request_sent
                    && !a.components_confirmed)
                    || a.portal_request_sent;
                involved && !waiting_release
            }))
        {
            return Err(CastRejection::Busy);
        }
        self.heartbeat(now);
        if reserved && self.periodic_involves(actor) {
            return Err(CastRejection::Busy);
        }
        let clock = self
            .registry_clocks
            .get_mut(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if clock.error.is_some() || reserved && clock.active && clock.next_due <= now {
            return Err(CastRejection::Busy);
        }
        clock.reserved = reserved;
        Ok(())
    }

    /// Transfer ownership after output and valuable-operation drainage. This is
    /// not a durability acknowledgment. The caller must retain the returned state.
    pub(crate) fn can_take_registry(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        if self.periodic_involves(actor)
            || self.proc_involves(actor)
            || self
                .instant_continuations
                .values()
                .any(|a| a.origin.actor() == actor || a.target == Some(actor))
        {
            return Err(CastRejection::Busy);
        }
        if self
            .attempts
            .values()
            .any(|a| a.origin.actor() == actor || attempt_target(a) == Some(actor))
            || self.flying.values().any(|f| {
                f.source == actor
                    || f.target == Some(actor)
                    || f.pending_impact.flatten() == Some(actor.0)
                    || f.pending_damage.as_ref().is_some_and(|d| d.target == actor)
            })
            || self.events.iter().any(|e| event_actor(e) == actor)
        {
            return Err(CastRejection::Busy);
        }
        self.heartbeat(now);
        if self.periodic_involves(actor)
            || self.proc_involves(actor)
            || self
                .instant_continuations
                .values()
                .any(|a| a.origin.actor() == actor || a.target == Some(actor))
        {
            return Err(CastRejection::Busy);
        }
        let clock = self
            .registry_clocks
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if clock.reserved
            || clock.error.is_some()
            || clock.active && clock.next_due <= now
            || self.events.iter().any(|e| event_actor(e) == actor)
        {
            return Err(CastRejection::Busy);
        }
        Ok(())
    }

    pub(crate) fn take_registry(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<EnchantmentRegistry, CastRejection> {
        self.can_take_registry(actor, now)?;
        self.casters.remove(&actor);
        self.vitae_sequences.remove(&actor);
        self.vitae_removals.remove(&actor);
        self.defense_profiles.remove(&actor);
        self.damage_profiles.remove(&actor);
        self.item_types.remove(&actor);
        self.damage_wands.remove(&actor.0);
        self.item_spell_qualities.remove(&actor);
        self.item_targets.remove(&actor);
        self.registry_clocks.remove(&actor);
        self.registries
            .remove(&actor)
            .ok_or(CastRejection::MissingActor)
    }

    /// Before committing an item tombstone, ensure the registry can retire.
    /// Outbox events are immutable projections and remain owned until drained.
    pub(crate) fn can_retire_item_registry(
        &self,
        actor: EntityId,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.check_time(now)?;
        let clock = self
            .registry_clocks
            .get(&actor)
            .ok_or(CastRejection::MissingActor)?;
        if !clock.reserved
            || self.periodic_involves(actor)
            || clock.error.is_some()
            || self.casters.contains_key(&actor)
            || self
                .attempts
                .values()
                .any(|a| a.origin.actor() == actor || attempt_target(a) == Some(actor))
            || self.flying.values().any(|f| {
                f.source == actor
                    || f.target == Some(actor)
                    || f.pending_impact.flatten() == Some(actor.0)
                    || f.pending_damage.as_ref().is_some_and(|d| d.target == actor)
            })
        {
            return Err(CastRejection::Busy);
        }
        Ok(())
    }
    /// Only after the exact durable item-removal receipt. Does not discard events
    /// or represent a save acknowledgment; the caller owns the committed tombstone.
    pub(crate) fn retire_item_registry(
        &mut self,
        actor: EntityId,
        now: f64,
    ) -> Result<(), CastRejection> {
        self.can_retire_item_registry(actor, now)?;
        self.registries
            .remove(&actor)
            .ok_or(CastRejection::MissingActor)?;
        self.registry_clocks.remove(&actor);
        self.defense_profiles.remove(&actor);
        self.damage_profiles.remove(&actor);
        self.item_types.remove(&actor);
        self.damage_wands.remove(&actor.0);
        self.item_spell_qualities.remove(&actor);
        self.item_targets.remove(&actor);
        Ok(())
    }

    pub(crate) fn registry_failure(&self, actor: EntityId) -> Option<RegistryError> {
        self.registry_clocks.get(&actor).and_then(|c| c.error)
    }

    pub(crate) fn register_enchantment_metadata(
        &mut self,
        spell: u32,
        metadata: EnchantmentMetadata,
    ) -> Result<(), CastRejection> {
        if !self.spells.contains_key(&spell)
            || self.enchantment_metadata.contains_key(&spell)
            || !metadata.degrade_modifier.is_finite()
            || !metadata.degrade_limit.is_finite()
            || !metadata.last_time_degraded.is_finite()
        {
            return Err(CastRejection::InvalidState);
        }
        self.enchantment_metadata.insert(spell, metadata);
        Ok(())
    }

    pub(super) fn metadata_for(
        &self,
        spell: u32,
        _spec: &bace_magic::EnchantmentSpec,
    ) -> EnchantmentMetadata {
        *self
            .enchantment_metadata
            .get(&spell)
            .expect("enchanting cast requires prepared metadata")
    }

    pub(crate) fn prepare_registry_time(&mut self, now: f64) -> Result<(), CastRejection> {
        self.check_time(now)?;
        self.heartbeat(now);
        Ok(())
    }

    pub(super) fn check_time(&self, now: f64) -> Result<(), CastRejection> {
        if !now.is_finite()
            || now < self.current_time
            || !(now + 5.0).is_finite()
            || now + 5.0 <= now
        {
            return Err(CastRejection::InvalidState);
        }
        Ok(())
    }

    pub(super) fn heartbeat(&mut self, now: f64) {
        if self.check_time(now).is_err() {
            for clock in self.registry_clocks.values_mut() {
                clock.error = Some(RegistryError::InvalidTime);
            }
            return;
        }
        self.current_time = now;
        for (&actor, registry) in &mut self.registries {
            let clock = self
                .registry_clocks
                .get_mut(&actor)
                .expect("registry clock admitted atomically");
            if !clock.active || clock.reserved || clock.error.is_some() || now < clock.next_due {
                continue;
            }
            let pulse = if self.damage_profiles.contains_key(&actor) {
                match super::periodic_native::native_pulse(
                    actor,
                    registry,
                    clock.next_due.to_bits(),
                    |spell| self.damage_spell_flags.get(&spell).copied(),
                ) {
                    Ok(pulse) => pulse,
                    Err(error) => {
                        clock.error = Some(error);
                        continue;
                    }
                }
            } else {
                periodic::periodic_pulse(actor, registry)
            };
            if pulse.is_some() && self.periodic.len() >= self.capacity {
                continue;
            }
            // Timer-only debt coalesces; periodic effects advance one 5s pulse
            // at a time, preserving every boundary and the final expiry tick.
            let intervals = if pulse.is_some() {
                1.0
            } else {
                ((now - clock.next_due) / 5.0).floor() + 1.0
            };
            let elapsed = intervals * 5.0;
            let next_due = clock.next_due + elapsed;
            if !next_due.is_finite() || next_due <= clock.next_due {
                clock.error = Some(RegistryError::InvalidTime);
                continue;
            }
            let count = match registry.expired_count(elapsed) {
                Ok(count) => count,
                Err(error) => {
                    clock.error = Some(error);
                    continue;
                }
            };
            if count > 0 && self.events.len() >= self.capacity {
                continue;
            }
            self.heartbeat_scratch.clear();
            match registry.heartbeat(elapsed, &mut self.heartbeat_scratch) {
                Ok(()) => {
                    let periodic = pulse.is_some();
                    if let Some(mut pulse) = pulse {
                        if count > 0 {
                            pulse.expired = self.heartbeat_scratch.clone();
                        }
                        self.periodic.push_back(pulse);
                    }
                    clock.next_due = next_due;
                    if count > 0 && !periodic {
                        self.events.push_back(MagicEvent::EnchantmentsRemoved {
                            actor,
                            entries: self.heartbeat_scratch.clone(),
                        });
                    }
                }
                Err(error) => clock.error = Some(error),
            }
        }
    }
}

pub(super) fn event_actor(event: &MagicEvent) -> EntityId {
    match event {
        MagicEvent::EnchantmentExpired { actor, .. }
        | MagicEvent::Motion { actor, .. }
        | MagicEvent::Turning { actor, .. }
        | MagicEvent::Vital { actor, .. }
        | MagicEvent::Enchantment { actor, .. }
        | MagicEvent::EnchantmentsRemoved { actor, .. }
        | MagicEvent::ProjectileCreated { actor, .. }
        | MagicEvent::ProjectileRemoved { actor, .. }
        | MagicEvent::ProjectileExploded { actor, .. }
        | MagicEvent::ComponentsRequired { actor, .. }
        | MagicEvent::TargetRejected { actor, .. }
        | MagicEvent::Fizzle { actor, .. }
        | MagicEvent::MotionStopped { actor, .. }
        | MagicEvent::MotionHook { actor, .. }
        | MagicEvent::PortalRequired { actor, .. } => *actor,
    }
}

impl Magic {
    pub(crate) fn adopt_skill_device_cooldown(
        &mut self,
        actor: EntityId,
        proposal: bace_magic::PreparedEnchantment,
    ) {
        let update = self
            .registries
            .get_mut(&actor)
            .expect("reserved skill device cooldown owner")
            .adopt(proposal)
            .expect("preflighted exact skill device cooldown revision");
        self.events.push_back(MagicEvent::Enchantment {
            actor,
            entry: update.entry,
        });
    }
    pub(crate) fn adopt_pet_cooldown(
        &mut self,
        actor: EntityId,
        proposal: bace_magic::PreparedEnchantment,
    ) {
        let update = self
            .registries
            .get_mut(&actor)
            .expect("reserved cooldown owner")
            .adopt(proposal)
            .expect("reserved exact cooldown revision");
        self.events.push_back(MagicEvent::Enchantment {
            actor,
            entry: update.entry,
        });
    }
}
