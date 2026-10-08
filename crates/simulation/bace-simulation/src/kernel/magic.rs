//! Single-owner kernel magic operations.
use super::*;
impl Kernel {
    /// Install raw qualities only after their item owner adopted the exact durable
    /// mutation; cached in-flight wand identity is retained even when unequipped.
    pub fn refresh_magic_wand_source(
        &mut self,
        item: EntityId,
        revision: u64,
        source: &bace_content::WeenieV1,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        let owned = self
            .inventory
            .item(item)
            .ok_or(bace_gameplay_api::CastRejection::InvalidTarget)?;
        if self.inventory.reserved(item)
            || owned.revision != revision
            || owned.template != source.weenie_id
        {
            return Err(bace_gameplay_api::CastRejection::OwnershipMismatch);
        }
        let wand = bace_magic::prepare_magic_wand(item.0, revision, source)
            .map_err(|_| bace_gameplay_api::CastRejection::InvalidState)?;
        self.magic.refresh_damage_wand(wand)
    }
    pub fn register_magic_asset_batch(
        &mut self,
        batch: crate::PreparedMagicAssetBatch,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_asset_batch(batch)
    }
    pub fn register_magic_item_type(
        &mut self,
        item: EntityId,
        item_type: u32,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if self.inventory.item(item).is_none() || self.inventory.reserved(item) {
            return Err(bace_gameplay_api::CastRejection::InvalidTarget);
        }
        self.magic.register_item_type(item, item_type)
    }
    pub fn register_magic_item_quality(
        &mut self,
        item: EntityId,
        resist_magic: u32,
        non_projectile_immune: bool,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if self.inventory.item(item).is_none() || self.inventory.reserved(item) {
            return Err(bace_gameplay_api::CastRejection::InvalidTarget);
        }
        self.magic
            .register_item_spell_quality(item, resist_magic, non_projectile_immune)
    }
    pub fn register_magic_target_mask(
        &mut self,
        spell: u32,
        mask: u32,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_spell_target_mask(spell, mask)
    }
    pub fn register_magic_damage_profile(
        &mut self,
        actor: EntityId,
        profile: bace_magic::MagicDamageProfile,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
        {
            return Err(bace_gameplay_api::CastRejection::Busy);
        }
        self.magic.register_damage_profile(actor, profile)
    }
    pub fn register_magic_damage_spell(
        &mut self,
        spell: u32,
        formula_level: u32,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_damage_spell(spell, formula_level)
    }
}
impl Kernel {
    pub(super) fn sync_registry_revisions(&mut self) -> Result<(), SimulationError> {
        for (actor, seen) in &mut self.registry_revisions {
            if self.characters.reserved(*actor)
                || self.npcs.reserved(*actor)
                || self.inventory.reserved(*actor)
                || self.housing.reserved(*actor)
                || self.pets.reserved(*actor)
                || self.portals.reserved(*actor)
            {
                continue;
            }
            let Some(registry) = self.magic.registry(*actor) else {
                continue;
            };
            if registry.revision() != *seen {
                if self.npc_combat_assets.contains_key(actor) {
                    *seen = registry.revision();
                    self.physical_refresh_dirty.insert(*actor);
                    continue;
                }
                let item_before = self.inventory.item(*actor).map(|item| item.revision);
                let changed = if self.characters.get(*actor).is_some() {
                    self.characters.touch_auxiliary(*actor)
                } else if self.housing.state(*actor).is_some() {
                    if self
                        .housing
                        .touch_registry(*actor)
                        .map_err(|_| SimulationError::AuxiliaryRevision)?
                    {
                        *seen = registry.revision();
                    }
                    continue;
                } else {
                    self.inventory.touch_registry(*actor)
                }
                .map_err(|_| SimulationError::AuxiliaryRevision)?;
                if changed {
                    let owner = if self.characters.get(*actor).is_some() {
                        Some(*actor)
                    } else {
                        self.inventory
                            .item(*actor)
                            .and_then(|item| match item.place {
                                bace_inventory::ItemPlace::Contained {
                                    container,
                                    equipped,
                                    ..
                                } if equipped != 0 => Some(container),
                                _ => None,
                            })
                    };
                    if let Some(owner) = owner {
                        if let Some(before) = item_before
                            && let Some(after) =
                                self.inventory.item(*actor).map(|item| item.revision)
                        {
                            match self
                                .combat
                                .rebind_physical_registry_revision(*actor, before, after)
                            {
                                Ok(()) | Err(crate::SkillRefreshError::Busy) => {}
                                Err(_) => return Err(SimulationError::SkillRefresh),
                            }
                        }
                        if self.combat.physical_refresh_source(owner).is_some() {
                            self.physical_refresh_dirty.insert(owner);
                        }
                    }
                    *seen = registry.revision();
                }
            }
        }
        Ok(())
    }

    pub fn register_magic_registry(
        &mut self,
        actor: EntityId,
        registry: bace_magic::EnchantmentRegistry,
        active: bool,
    ) -> Result<
        (),
        (
            bace_gameplay_api::CastRejection,
            bace_magic::EnchantmentRegistry,
        ),
    > {
        if self.housing.reserved(actor)
            || self.inventory.reserved(actor)
            || self.characters.reserved(actor)
        {
            return Err((bace_gameplay_api::CastRejection::Busy, registry));
        }
        if !self.world.contains_identity(actor)
            && self.inventory.item(actor).is_none()
            && self.housing.state(actor).is_none()
        {
            return Err((bace_gameplay_api::CastRejection::InvalidTarget, registry));
        }
        let revision = registry.revision();
        self.magic
            .register_registry(actor, registry, active, self.tick as f64 / 30.0)?;
        self.registry_revisions.insert(actor, revision);
        Ok(())
    }
    pub fn set_magic_registry_active(
        &mut self,
        actor: EntityId,
        active: bool,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic
            .set_registry_active(actor, active, self.tick as f64 / 30.0)
    }
    pub fn reserve_magic_registry(
        &mut self,
        actor: EntityId,
        reserved: bool,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if !reserved
            && (self.characters.reserved(actor)
                || self.inventory.reserved(actor)
                || self.housing.reserved(actor)
                || self.npcs.reserved(actor))
        {
            return Err(bace_gameplay_api::CastRejection::Busy);
        }
        self.magic
            .reserve_registry(actor, reserved, self.tick as f64 / 30.0)
    }
    pub fn take_magic_registry(
        &mut self,
        actor: EntityId,
    ) -> Result<bace_magic::EnchantmentRegistry, bace_gameplay_api::CastRejection> {
        if self.combat.dirty_involves(actor)
            || self.housing.reserved(actor)
            || self.inventory.reserved(actor)
            || self.characters.reserved(actor)
        {
            return Err(bace_gameplay_api::CastRejection::Busy);
        }
        self.magic
            .can_take_registry(actor, self.tick as f64 / 30.0)?;
        self.sync_registry_revisions()
            .map_err(|_| bace_gameplay_api::CastRejection::InvalidState)?;
        let registry = self.magic.take_registry(actor, self.tick as f64 / 30.0)?;
        self.registry_revisions.remove(&actor);
        Ok(registry)
    }
    pub fn magic_registry_failure(&self, actor: EntityId) -> Option<bace_magic::RegistryError> {
        self.magic.registry_failure(actor)
    }
    pub fn register_enchantment_metadata(
        &mut self,
        spell: u32,
        metadata: bace_magic::EnchantmentMetadata,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_enchantment_metadata(spell, metadata)
    }
    pub fn required_spell_components(&self, actor: EntityId, spell: u32) -> Option<&[(u32, u32)]> {
        self.magic.required_components(actor, spell)
    }
    pub fn configure_magic_random_shared(
        &mut self,
        root: std::sync::Arc<bace_random::RandomRoot>,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.configure_random_shared(root)
    }
    pub fn configure_magic_random(
        &mut self,
        root: bace_random::RandomRoot,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.configure_random(root)
    }
    pub fn register_magic_spell(
        &mut self,
        spell: crate::PreparedMagicSpell,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_spell(spell)
    }
    pub fn register_magic_caster(
        &mut self,
        actor: EntityId,
        caster: crate::MagicCaster,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_caster(actor, caster, &self.world)?;
        if let Some(revision) = self.magic.recovery_revision(actor) {
            self.recovery_revisions.insert(actor, revision);
        }
        if let Some(registry) = self.magic.registry(actor) {
            self.registry_revisions
                .entry(actor)
                .or_insert(registry.revision());
        }
        Ok(())
    }
    pub fn take_cast_outcome(&mut self) -> Option<bace_gameplay_api::CastOutcome> {
        self.cast_outcomes.pop_front()
    }
    pub fn take_magic_event(&mut self) -> Option<crate::MagicEvent> {
        self.magic.take_event()
    }
    pub fn pending_cast_outcomes(&self) -> usize {
        self.cast_outcomes.len() + self.magic.pending_outcomes()
    }
    pub fn pending_magic_events(&self) -> usize {
        self.magic.pending_events()
    }
    pub fn has_magic_state(&self) -> bool {
        self.magic.has_state()
            || !self.cast_outcomes.is_empty()
            || self.pending_magic_damage.is_some()
            || self
                .commands
                .iter()
                .any(|c| matches!(c, Command::Cast { .. }))
    }
    pub fn magic_registry(&self, actor: EntityId) -> Option<&bace_magic::EnchantmentRegistry> {
        self.magic.registry(actor)
    }
    pub fn confirm_magic_components(
        &mut self,
        cast: u64,
        actor: EntityId,
        success: bool,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        if success {
            // Durable component success is admitted exclusively by the correlated
            // InventoryReceipt, never an uncorrelated boolean acknowledgement.
            return Err(bace_gameplay_api::CastRejection::InvalidState);
        }
        self.magic.confirm_components(cast, actor, false)
    }
    /// Portal completion is admitted only by confirm_portal_committed and the
    /// owning service's geometry/mutation adoption, never a boolean acknowledgement.
    pub fn confirm_magic_portal(
        &mut self,
        _cast: u64,
        _actor: EntityId,
        _success: bool,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        Err(bace_gameplay_api::CastRejection::InvalidState)
    }
    pub(super) fn drain_cast_outcomes(&mut self) {
        while self.cast_outcomes.len() < self.outcome_capacity {
            let Some(outcome) = self.magic.take_outcome() else {
                break;
            };
            self.cast_outcomes.push_back(outcome);
        }
    }
    pub fn has_spell_components(
        &self,
        actor: EntityId,
        required: &[(u32, u32)],
    ) -> Result<bool, bace_gameplay_api::InventoryRejection> {
        self.inventory.has_required(actor, required)
    }
}

impl Kernel {
    pub fn magic_recovery(
        &self,
        actor: EntityId,
    ) -> Result<bace_magic::CastRecovery, bace_gameplay_api::CastRejection> {
        self.magic.recovery_snapshot(actor, self.tick as f64 / 30.0)
    }
    pub fn restore_magic_recovery(
        &mut self,
        actor: EntityId,
        recovery: bace_magic::CastRecovery,
        elapsed_offline_seconds: f64,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.restore_recovery(
            actor,
            recovery,
            self.tick as f64 / 30.0,
            elapsed_offline_seconds,
        )?;
        self.recovery_revisions.insert(actor, recovery.revision);
        Ok(())
    }
}

impl Kernel {
    /// Server-only ingress. Network commands use authenticated Command::Cast.
    pub fn cast_from_server(
        &mut self,
        origin: bace_gameplay_api::CastOrigin,
        request: bace_gameplay_api::CastRequest,
    ) -> Result<bace_gameplay_api::CastChange, bace_gameplay_api::CastRejection> {
        use bace_gameplay_api::{CastOrigin, CastRejection as E};
        let actor = origin.actor();
        if matches!(origin, CastOrigin::Player(_)) {
            return Err(E::NotBound);
        }
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.housing.reserved(actor)
        {
            return Err(E::Busy);
        }
        if let CastOrigin::ItemProc { item, .. } = origin {
            let item = self.inventory.item(item).ok_or(E::InvalidTarget)?;
            if !matches!(item.place, bace_inventory::ItemPlace::Contained { container, equipped, .. } if container == actor && equipped != 0)
            {
                return Err(E::OwnershipMismatch);
            }
        }
        self.magic.refresh_item_targets(
            &self.inventory,
            match request {
                bace_gameplay_api::CastRequest::Targeted { target, .. } => Some(target),
                _ => None,
            },
        );
        self.magic.apply_origin(
            origin,
            request,
            &mut self.world,
            self.tick as f64 / 30.0,
            &self.combat,
            &self.fellowships,
            Some((&self.characters, self.tick)),
        )
    }
    pub fn take_server_cast_outcome(&mut self) -> Option<bace_gameplay_api::ServerCastOutcome> {
        self.magic.take_server_outcome()
    }
}

impl Kernel {
    pub fn register_magic_components(
        &mut self,
        actor: EntityId,
        spell: u32,
        required: Vec<(u32, u32)>,
        modifiers: Vec<(u32, f32)>,
        loss: f32,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic
            .register_components(actor, spell, required, modifiers, loss)
    }
}

impl Kernel {
    pub fn register_monster_spellbook(
        &mut self,
        actor: EntityId,
        entries: Vec<bace_ai::MonsterSpell>,
        delay: f64,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_monster_spellbook(actor, entries, delay)
    }
    pub(super) fn service_monster_magic(&mut self) {
        let candidates: Vec<_> = self.population.magic_targets().collect();
        let now = self.tick as f64 / 30.0;
        for (actor, target) in candidates {
            let Some(target) = target else {
                continue;
            };
            if self.combat.active(actor)
                || self.npcs.retiring(actor)
                || self.world.combatant(actor).is_none_or(|s| s.health() == 0)
            {
                continue;
            }
            if let Ok(Some((event, spell))) = self.magic.monster_cast_request(actor, now)
                && self
                    .cast_from_server(
                        bace_gameplay_api::CastOrigin::Monster { actor, event },
                        bace_gameplay_api::CastRequest::Targeted { target, spell },
                    )
                    .is_ok()
            {
                self.combat.cancel(actor);
            }
        }
    }
}

impl Kernel {
    pub fn register_periodic_defense(
        &mut self,
        actor: EntityId,
        profile: crate::magic::PeriodicDefenseProfile,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_periodic_defense(actor, profile)
    }
}

impl Kernel {
    pub fn register_vitae_template(
        &mut self,
        entry: bace_magic::EnchantmentEntry,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_vitae_template(entry)
    }
    pub fn prepare_vitae(
        &self,
        actor: EntityId,
        value: Option<f32>,
        remove_after_seconds: Option<f64>,
    ) -> Result<bace_magic::VitaeMutation, bace_gameplay_api::CastRejection> {
        self.magic.prepare_vitae(actor, value, remove_after_seconds)
    }
    pub fn adopt_vitae(
        &mut self,
        actor: EntityId,
        mutation: bace_magic::VitaeMutation,
    ) -> Result<
        (),
        (
            bace_gameplay_api::CastRejection,
            Box<bace_magic::VitaeMutation>,
        ),
    > {
        self.magic
            .adopt_vitae(actor, mutation, self.tick as f64 / 30.0)
    }
}

impl Kernel {
    /// Production supplies the durable world-owner generation before admitting casters.
    /// Epoch zero is reserved for explicitly constructed synthetic test kernels.
    pub fn configure_magic_epoch(
        &mut self,
        epoch: u64,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.configure_execution_epoch(epoch)
    }
}

impl Kernel {
    /// Fixture-only opt-in. Authentic bodies still require prepared DAT chains.
    pub fn enable_synthetic_cast_timing(&mut self) {
        self.magic.enable_synthetic_cast_timing();
    }
}

impl Kernel {
    pub fn register_magic_gestures(
        &mut self,
        actor: EntityId,
        spell: u32,
        gestures: Vec<crate::PreparedCastGesture>,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_gestures(actor, spell, gestures)
    }
}

impl Kernel {
    /// Verified immutable Setup shape. Current magic flight admits only centered
    /// scalar spheres; more complex authored models are refused explicitly.
    pub fn register_magic_projectile_shape(
        &mut self,
        template: u32,
        shape: std::sync::Arc<bace_physics::CollisionShape>,
    ) -> Result<(), bace_gameplay_api::CastRejection> {
        self.magic.register_projectile_shape(template, shape)
    }
}
