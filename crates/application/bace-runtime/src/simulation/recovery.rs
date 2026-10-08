//! Complete simulation ownership recovery and unresolved-output gate.
use super::*;

/// Recovery transfers the single kernel owner; it never acknowledges persistence.
#[must_use = "retain the kernel and unresolved commands until lifecycle/save drain completes"]
pub struct SimulationExit {
    pub social: RecoveredSocial,
    pub player_deaths: RecoveredPlayerDeaths,
    pub interactions: RecoveredInteractions,
    pub regions: RecoveredRegions,
    pub pve_services: RecoveredPveServices,
    pub npc_services: RecoveredNpcServices,
    pub inventory_output: RecoveredInventory,
    pub visibility: RecoveredVisibility,
    pub object_view: RecoveredObjectView,
    pub locomotion: RecoveredLocomotion,
    pub physical_resource: RecoveredPhysicalResource,
    pub magic_resource: RecoveredMagicResource,
    pub player_snapshots: RecoveredPlayerSnapshots,
    pub player_admissions: RecoveredPlayerAdmissions,
    pub region_admissions: RecoveredRegionAdmissions,
    pub report: SimulationReport,
    pub kernel: Kernel,
    pub unprocessed_commands: Vec<Command>,
    pub undelivered_outcomes: Vec<ProgressionOutcome>,
    pub undelivered_combat_outcomes: Vec<CombatOutcome>,
    pub undelivered_combat_events: Vec<CombatEvent>,
    pub undelivered_death_proposals: Vec<DeathProposal>,
    pub undelivered_pve_events: Vec<PveEvent>,
    pub undelivered_door_outcomes: Vec<DoorOutcome>,
    pub undelivered_door_events: Vec<DoorEvent>,
    pub undelivered_npc_proposals: Vec<NpcProposal>,
    pub undelivered_npc_notifications: Vec<NpcNotification>,
    pub undelivered_cast_outcomes: Vec<CastOutcome>,
    pub undelivered_magic_events: Vec<MagicEvent>,
    pub undelivered_inventory_proposals: Vec<InventoryTicket>,
    pub undelivered_physical_launches: Vec<PhysicalLaunchProposal>,
    pub undelivered_physical_events: Vec<PhysicalCombatEvent>,
    pub undelivered_server_cast_outcomes: Vec<ServerCastOutcome>,
    pub undelivered_skill_outcomes: Vec<SkillOutcome>,
    pub undelivered_skill_device_outcomes: Vec<SkillDeviceOutcome>,
    pub undelivered_skill_device_proposals: Vec<SkillDeviceTicket>,
    pub undelivered_attribute_transfer_outcomes: Vec<AttributeTransferOutcome>,
    pub undelivered_attribute_transfer_proposals: Vec<AttributeTransferDeviceTicket>,
    pub undelivered_crafting_proposals: Vec<CraftingTicket>,
    pub undelivered_crafting_outcomes: Vec<CraftingOutcome>,
    pub undelivered_ui_outcomes: Vec<UiOutcome>,
    pub undelivered_pet_events: Vec<PetEvent>,
    pub undelivered_pet_outcomes: Vec<bace_simulation::PetOutcome>,
    pub undelivered_vendor_outcomes: Vec<bace_simulation::VendorOutcome>,
    pub undelivered_portal_proposals: Vec<PortalServiceTicket>,
    pub undelivered_portal_events: Vec<PortalServiceEvent>,
    pub undelivered_generator_requests: Vec<GeneratorHostRequest>,
    pub undelivered_generator_events: Vec<GeneratorWorldEvent>,
    pub undelivered_generator_outcomes: Vec<GeneratorCommandOutcome>,
    pub undelivered_generator_retirements: Vec<GeneratedRetirementTicket>,
    pub undelivered_housing_proposals: Vec<HousingTicket>,
    pub failure: Option<SimulationError>,
}
impl SimulationExit {
    /// Terminal-only proof after every adapter/save owner has already drained.
    /// Failure leaves this entire recovered kernel and every output available.
    pub fn prepare_durable_shutdown(&mut self) -> Result<(), String> {
        if self.failure.is_some() || !self.unprocessed_commands.is_empty() {
            return Err("simulation failure or unprocessed commands retained".into());
        }
        self.kernel
            .discard_idle_preparation()
            .map_err(str::to_owned)?;
        if self.requires_recovery() || self.kernel.world().has_live_state() {
            return Err(format!(
                "simulation shutdown has unresolved owners: {self:?}"
            ));
        }
        Ok(())
    }
    pub(super) fn requires_recovery(&self) -> bool {
        self.region_admissions.has_state()
            || self.kernel.has_region_admission_work()
            || self.player_admissions.has_state()
            || self.kernel.has_player_admission_work()
            || self.player_snapshots.has_state()
            || self.kernel.has_player_snapshot_work()
            || self.pve_services.has_state()
            || self.npc_services.has_state()
            || self.inventory_output.has_state()
            || self.kernel.has_inventory_command_state()
            || self.visibility.has_state()
            || self.object_view.has_state()
            || self.locomotion.has_state()
            || self.physical_resource.has_state()
            || self.magic_resource.has_state()
            || self.kernel.has_visibility_work()
            || self.kernel.has_object_view_work()
            || self.kernel.has_locomotion_work()
            || self.kernel.has_physical_resource_work()
            || self.kernel.has_magic_resource_work()
            || self.kernel.has_npc_service_work()
            || self.regions.has_state()
            || self.kernel.has_region_unload_state()
            || self.kernel.has_staff_state()
            || self.interactions.has_state()
            || self.kernel.has_recall_state()
            || self.player_deaths.has_state()
            || self.kernel.has_player_death_state()
            || self.kernel.has_corpse_expiry_state()
            || self.social.has_state()
            || self.kernel.has_social_state()
            || self.kernel.has_characters()
            || self.kernel.has_magic_state()
            || self.kernel.world().has_motion_state()
            || self.kernel.world().has_vital_reservations()
            || self.kernel.has_pet_state()
            || self.kernel.has_pet_command_state()
            || self.kernel.has_portal_state()
            || self.kernel.has_physical_combat_state()
            || self.kernel.has_inventory_state()
            || self.kernel.has_character_feature_work()
            || self.kernel.has_crafting_state()
            || self.kernel.has_generator_state()
            || self.kernel.has_housing_state()
            || !self.undelivered_cast_outcomes.is_empty()
            || !self.undelivered_magic_events.is_empty()
            || !self.undelivered_inventory_proposals.is_empty()
            || !self.undelivered_physical_launches.is_empty()
            || !self.undelivered_physical_events.is_empty()
            || !self.undelivered_server_cast_outcomes.is_empty()
            || !self.undelivered_skill_outcomes.is_empty()
            || !self.undelivered_skill_device_outcomes.is_empty()
            || !self.undelivered_skill_device_proposals.is_empty()
            || !self.undelivered_attribute_transfer_outcomes.is_empty()
            || !self.undelivered_attribute_transfer_proposals.is_empty()
            || !self.undelivered_crafting_proposals.is_empty()
            || !self.undelivered_crafting_outcomes.is_empty()
            || !self.undelivered_ui_outcomes.is_empty()
            || !self.undelivered_pet_events.is_empty()
            || !self.undelivered_pet_outcomes.is_empty()
            || !self.undelivered_vendor_outcomes.is_empty()
            || !self.undelivered_portal_proposals.is_empty()
            || !self.undelivered_portal_events.is_empty()
            || !self.undelivered_generator_requests.is_empty()
            || !self.undelivered_generator_events.is_empty()
            || !self.undelivered_generator_outcomes.is_empty()
            || !self.undelivered_generator_retirements.is_empty()
            || !self.undelivered_housing_proposals.is_empty()
            || self.kernel.has_npc_state()
            || !self.undelivered_npc_proposals.is_empty()
            || !self.undelivered_npc_notifications.is_empty()
            || self.kernel.has_queued_progression()
            || self.kernel.has_queued_combat()
            || self.kernel.has_pve_state()
            || self.kernel.has_door_state()
            || self.kernel.has_queued_door()
            || self.kernel.pending_door_outcomes() != 0
            || self.kernel.pending_door_events() != 0
            || !self.undelivered_door_outcomes.is_empty()
            || !self.undelivered_door_events.is_empty()
            || !self.undelivered_death_proposals.is_empty()
            || !self.undelivered_pve_events.is_empty()
            || !self.undelivered_combat_outcomes.is_empty()
            || !self.undelivered_combat_events.is_empty()
            || self.kernel.pending_combat_outcomes() != 0
            || self.kernel.pending_combat_events() != 0
            || !self.undelivered_outcomes.is_empty()
            || self.kernel.pending_progression_outcomes() != 0
            || self.unprocessed_commands.iter().any(|command| {
                matches!(
                    command,
                    Command::AdmitPlayer(_)
                        | Command::AdmitResidentRegion(_)
                        | Command::PlayerSnapshot(_)
                        | Command::PveService(_)
                        | Command::PlayerDeath(_)
                        | Command::PlayerDeathService(_)
                        | Command::Recall(_)
                        | Command::PortalResolution(_)
                        | Command::Social { .. }
                        | Command::Fellowship { .. }
                        | Command::Allegiance { .. }
                        | Command::SocialControl(_)
                        | Command::Crafting(_)
                        | Command::Inventory(_)
                        | Command::SkillDevice(_)
                        | Command::Ui { .. }
                        | Command::TrainSkill { .. }
                        | Command::CommitSkill { .. }
                        | Command::RollbackSkill { .. }
                        | Command::RaiseProgression { .. }
                        | Command::Combat { .. }
                        | Command::UseDoor { .. }
                )
            })
    }
}
impl std::fmt::Debug for SimulationExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulationExit")
            .field("report", &self.report)
            .field("has_characters", &self.kernel.has_characters())
            .field("unprocessed_commands", &self.unprocessed_commands.len())
            .field("undelivered_outcomes", &self.undelivered_outcomes.len())
            .field(
                "undelivered_combat_outcomes",
                &self.undelivered_combat_outcomes.len(),
            )
            .field(
                "undelivered_combat_events",
                &self.undelivered_combat_events.len(),
            )
            .field(
                "undelivered_death_proposals",
                &self.undelivered_death_proposals.len(),
            )
            .field("undelivered_pve_events", &self.undelivered_pve_events.len())
            .field(
                "undelivered_door_outcomes",
                &self.undelivered_door_outcomes.len(),
            )
            .field(
                "undelivered_door_events",
                &self.undelivered_door_events.len(),
            )
            .field("failure", &self.failure)
            .finish()
    }
}
