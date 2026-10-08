//! Authoritative equipment preparation admission and physical result projections.
use super::*;
impl Kernel {
    pub fn capture_physical_recovery(&self, actor: EntityId) -> f64 {
        self.combat
            .capture_physical_recovery(actor, self.tick as f64 / 30.0)
    }
    pub fn restore_physical_recovery(
        &mut self,
        actor: EntityId,
        remaining: f64,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        self.combat
            .restore_physical_recovery(actor, self.tick as f64 / 30.0, remaining)?;
        if self.characters.get(actor).is_some() {
            self.physical_recovery_deadlines
                .insert(actor, self.tick as f64 / 30.0 + remaining);
        }
        Ok(())
    }
    /// Prepared immutable chains are admitted off-thread; only World advances
    /// their hooks and completion on the simulation owner.
    pub fn register_physical_motion(
        &mut self,
        actor: EntityId,
        motion: u32,
        speed: f32,
        chain: std::sync::Arc<bace_motion::PreparedMotionChain>,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        self.combat
            .register_physical_motion(actor, motion, speed, chain)
    }

    pub fn set_physical_options(
        &mut self,
        actor: EntityId,
        options1: u32,
        options2: u32,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        self.combat.set_physical_options(actor, options1, options2)
    }

    pub fn physical_recovery_pending(&self, actor: EntityId) -> bool {
        self.combat.recovery_pending(actor, self.tick as f64 / 30.0)
    }
    pub fn register_physical_combat(
        &mut self,
        actor: EntityId,
        profile: std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        if self.characters.reserved(actor)
            || self.inventory.reserved(actor)
            || self.npcs.reserved(actor)
        {
            return Err(bace_gameplay_api::CombatRejection::Busy);
        }
        self.combat.register_physical(actor, profile, &self.world)?;
        if let Some(ui) = self.characters.ui(actor) {
            self.combat
                .set_physical_options(actor, ui.options1, ui.options2)?;
        }
        Ok(())
    }
    pub fn physical_combat_profile(
        &self,
        actor: EntityId,
    ) -> Option<&std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>> {
        self.combat.physical_profile(actor)
    }
    pub fn take_physical_combat_event(&mut self) -> Option<crate::combat::PhysicalCombatEvent> {
        self.combat.take_physical_event()
    }
}

impl Kernel {
    pub fn supply_physical_projectile_id(
        &mut self,
        id: EntityId,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        if (self.population.reserves_identity(id) || self.generator_reserves_identity(id))
            || self.magic.reserves_identity(id)
            || self.inventory.item(id).is_some()
        {
            return Err(bace_gameplay_api::CombatRejection::InvalidRequest);
        }
        self.combat.supply_missile_id(id, &self.world)
    }
    pub fn physical_reserves_identity(&self, id: EntityId) -> bool {
        self.combat.reserves_projectile(id)
    }
    pub fn take_physical_launch(
        &mut self,
    ) -> Option<bace_gameplay_api::weapon_combat::PhysicalLaunchProposal> {
        self.combat.take_launch()
    }
    pub fn retry_physical_launch(
        &mut self,
        operation: u64,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        self.combat.retry_launch(operation)
    }
    pub fn confirm_physical_launch(
        &mut self,
        receipt: bace_gameplay_api::weapon_combat::PhysicalLaunchReceipt,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        self.combat.set_simulation_tick(self.tick);
        self.combat
            .confirm_launch_with_observers(receipt, &mut self.world, &self.characters)
    }
}

impl Kernel {
    pub fn has_physical_combat_state(&self) -> bool {
        self.combat.has_physical_state()
    }
}

impl Kernel {
    /// Invoke only after the matching equipment receipt. The caller prepares this
    /// immutable view before the valuable operation; in-flight missiles keep their
    /// original firing snapshot while subsequent defense/attacks use this view.
    pub fn adopt_physical_equipment_profile(
        &mut self,
        actor: EntityId,
        profile: std::sync::Arc<bace_gameplay_api::weapon_combat::PhysicalCombatProfile>,
    ) -> Result<(), bace_gameplay_api::CombatRejection> {
        bace_combat::physical::validate_physical_profile(&profile)
            .map_err(|_| bace_gameplay_api::CombatRejection::InvalidRequest)?;
        if !self
            .combat
            .proposed_equipment_current(&profile, actor, &self.inventory)
        {
            return Err(bace_gameplay_api::CombatRejection::MissingCombatProfile);
        }
        self.combat.cancel(actor);
        self.combat.register_physical(actor, profile, &self.world)
    }
}
