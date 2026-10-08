mod inventory_placement;
mod item_experience;
mod player_admission_commands;
mod player_snapshot;
mod pve_commands;
mod region_admission_commands;
pub use inventory_placement::{PreparedStackDrop, StackWorldPlacement};
mod actor_magic;
mod allegiance_cache;
mod allegiance_commands;
mod allegiances;
mod constructed_creatures;
pub use constructed_creatures::PreparedRestoredConstructedCreature;
mod experience_events;
mod fellowships;
mod generated_retirement;
mod generator_enchantments;
mod npc_rewards;
mod player_admission;
mod player_death;
mod pve_rewards;
mod recalls;
mod reward_supplements;
mod social;
mod social_controls;
mod social_rewards;
mod staff;
mod staff_assets;
mod staff_inspection;
mod world_policies;
pub use generator_enchantments::PreparedGeneratorEnchantment;
mod generators;
pub use generated_retirement::GeneratedRetirementTicket;
mod transactions;
pub use transactions::{HousingPaymentBinding, HousingPaymentError};
mod attribute_transfer;
mod character;
mod crafting;
mod crafting_motion;
mod crafting_proficiency;
mod crafting_wand;
mod device_commands;
mod equipment_effects;
mod equipment_mana;
mod housing;
mod inventory;
mod inventory_commands;
mod inventory_magic;
mod live_vitals;
mod locomotion;
mod locomotion_queue;
mod locomotion_refresh;
mod magic;
mod magic_npc_caster;
mod magic_procs;
mod magic_resource_commands;
mod npc;
mod npc_service_queue;
mod object_views;
mod physical_procs;
mod physical_refresh;
mod physical_resources;
mod pk_activity;
mod shutdown;
mod simulation;
mod skill_devices;
mod skill_modifiers;
mod skill_refresh;
mod visibility;
use crate::combat::{Combat, CombatEvent};
use crate::doors::{DoorEvent, Doors, PreparedDoor};
use crate::pve::{DeathProposal, NpcBlueprint, Population, PveError, PveEvent};
use crate::{CharacterRegistrationError, characters::Characters};
use bace_character::CharacterProgression;
use bace_gameplay_api::{
    ActionContext, ActionResult, CharacterBinding, CombatOutcome, CombatRejection, CombatRequest,
    ProgressionActionRejection, ProgressionOutcome, RaiseProgression,
};
use bace_gameplay_api::{DoorOutcome, DoorRejection, UseDoor};
use bace_geometry::Vec3;
use bace_motion::MotionIntent;
use bace_types::{CellId, EntityId};
use bace_world::{World, WorldError};
pub use live_vitals::PreparedVitalInputs;
use std::collections::VecDeque;

pub enum Command {
    AdmitPlayer(crate::PlayerAdmissionRequest),
    PlayerEntered(crate::PlayerEnteredRequest),
    DetachPlayer(crate::PlayerDetachRequest),
    AdmitResidentRegion(crate::RegionAdmissionRequest),
    PlayerSnapshot(crate::PlayerSnapshotRequest),
    Visibility(Box<bace_gameplay_api::visibility::VisibilityRequest>),
    ObjectView(Box<bace_gameplay_api::visibility::ObjectViewRequest>),
    Locomotion(bace_gameplay_api::locomotion::LocomotionCommand),
    PhysicalResource(Box<crate::PhysicalResourceCommand>),
    MagicResource(crate::MagicResourceCommand),
    PveService(crate::PveServiceCommand),
    Pet(crate::PetCommand),
    NpcService(Box<crate::NpcServiceCommand>),
    Staff(bace_gameplay_api::staff::StaffCommand),
    ActorMagicProgram {
        correlation: u64,
        program: Box<crate::PreparedActorMagicProgram>,
    },
    PlayerDeath(crate::PlayerDeathCommand),
    PlayerDeathService(crate::PlayerDeathServiceCommand),
    CorpseAccess(crate::CorpseAccessCommand),
    CorpseConsent(crate::CorpseConsentCommand),
    SocialControl(crate::SocialControl),
    Allegiance {
        context: ActionContext,
        request: bace_gameplay_api::social::AllegianceRequest,
    },
    Fellowship {
        context: ActionContext,
        request: bace_gameplay_api::social::FellowshipRequest,
    },
    Social {
        context: ActionContext,
        request: bace_gameplay_api::social::SocialRequest,
    },
    SocialResolved {
        context: ActionContext,
        request: bace_gameplay_api::social::SocialRequest,
        identity: Option<bace_gameplay_api::social::SocialIdentity>,
    },
    Recall(crate::RecallCommand),
    PortalResolution(crate::PortalResolutionCommand),
    Generator(crate::GeneratorCommand),
    Vendor(Box<crate::VendorCommand>),
    Crafting(crate::CraftingCommand),
    Inventory(Box<crate::InventoryCommand>),
    SkillDevice(crate::SkillDeviceCommand),
    AttributeTransfer(crate::AttributeTransferCommand),
    Ui {
        context: ActionContext,
        request: bace_gameplay_api::UiRequest,
    },
    TrainSkill {
        context: ActionContext,
        request: bace_gameplay_api::TrainSkill,
    },
    CommitSkill {
        ticket: crate::SkillTicket,
    },
    RollbackSkill {
        ticket: crate::SkillTicket,
    },
    Cast {
        context: ActionContext,
        request: bace_gameplay_api::CastRequest,
    },
    UseDoor {
        context: ActionContext,
        request: UseDoor,
    },
    Combat {
        context: ActionContext,
        request: CombatRequest,
    },
    RaiseProgression {
        context: ActionContext,
        request: RaiseProgression,
    },
    Movement {
        actor: EntityId,
        epoch: u16,
        sequence: u32,
        intent: MotionIntent,
    },
    ServerTeleport {
        actor: EntityId,
        cell: CellId,
        position: Vec3,
    },
}

/// Pure, bounded, caller-driven simulation. Scheduling and clocks belong to
/// runtime; this function never waits for network, storage or a channel.
pub struct Kernel {
    social_gags: crate::social_gags::SocialGags,
    recalls: crate::recalls::Recalls,
    social: crate::social::SocialState,
    fellowships: crate::fellowships::Fellowships,
    allegiances: crate::allegiances::Allegiances,
    staff: crate::staff::StaffState,
    corpse_expiry: crate::corpse_expiry::CorpseExpiries,
    player_deaths: crate::player_death::PlayerDeaths,
    region_unloads: crate::region_unload::RegionUnloads,
    region_content_heads: std::collections::BTreeMap<u16, (u64, u64, [u8; 32])>,
    region_plain_roots: std::collections::BTreeMap<u16, std::collections::BTreeSet<EntityId>>,
    region_residency: crate::RegionResidency,
    region_activity_scratch: Vec<u16>,
    world_policies: bace_interactions::WorldPolicies,
    recall_policy: bace_interactions::RecallPolicy,
    death_policy: bace_interactions::PlayerDeathPolicy,
    generated_enchantments: generator_enchantments::GeneratedEnchantments,
    constructed_creatures: constructed_creatures::ConstructedCreatures,
    generated_retirements: std::collections::BTreeMap<u64, generated_retirement::PendingRetirement>,
    generated_vendors: std::collections::BTreeMap<EntityId, vendor_generators::GeneratedVendor>,
    vendor_outcomes: VecDeque<crate::VendorOutcome>,
    generator_outcomes: VecDeque<crate::GeneratorCommandOutcome>,
    avatar_locomotion:
        std::collections::BTreeMap<EntityId, std::sync::Arc<bace_motion::AnimatedLocomotion>>,
    player_world_seen: std::collections::BTreeMap<EntityId, crate::PlayerWorldSnapshot>,
    player_world_dirty: std::collections::BTreeMap<EntityId, crate::player_world::WorldDirty>,
    player_world_scratch: Vec<EntityId>,
    portals: portals::PortalServices,
    pets: crate::pets::Pets,
    pet_outcomes: VecDeque<crate::PetOutcome>,
    crafting: crate::crafting::Crafting,
    inventory_commands: inventory_commands::InventoryCommands,
    skill_devices: crate::skill_devices::SkillDevices,
    attribute_transfers: crate::attribute_transfer::AttributeTransfers,
    world: World,
    commands: VecDeque<Command>,
    capacity: usize,
    tick: u64,
    characters: Characters,
    progression_outcomes: VecDeque<ProgressionOutcome>,
    skill_outcomes: VecDeque<crate::SkillOutcome>,
    crafting_outcomes: VecDeque<crate::CraftingOutcome>,
    ui_outcomes: VecDeque<crate::UiOutcome>,
    skill_device_outcomes: VecDeque<crate::SkillDeviceOutcome>,
    attribute_transfer_outcomes: VecDeque<crate::AttributeTransferOutcome>,
    outcome_capacity: usize,
    combat: Combat,
    combat_outcomes: VecDeque<CombatOutcome>,
    combat_events: VecDeque<CombatEvent>,
    generators: crate::generators::Generators,
    population: Population,
    doors: Doors,
    npcs: crate::npc::Npcs,
    magic: crate::magic::Magic,
    player_admission_outcomes: VecDeque<crate::PlayerAdmissionOutcome>,
    player_entered_outcomes: VecDeque<crate::PlayerEnteredOutcome>,
    player_detach_outcomes: VecDeque<crate::PlayerDetachOutcome>,
    region_admission_outcomes: VecDeque<crate::RegionAdmissionOutcome>,
    player_snapshot_outcomes: VecDeque<crate::PlayerSnapshotOutcome>,
    pve_service_outcomes: VecDeque<crate::PveServiceOutcome>,
    npc_service_outcomes: VecDeque<std::sync::Arc<crate::NpcServiceOutcome>>,
    visibility_outcomes: VecDeque<std::sync::Arc<bace_gameplay_api::visibility::VisibilityOutcome>>,
    object_view_outcomes:
        VecDeque<std::sync::Arc<bace_gameplay_api::visibility::ObjectViewOutcome>>,
    locomotion_outcomes: VecDeque<std::sync::Arc<bace_gameplay_api::locomotion::LocomotionOutcome>>,
    locomotion_dirty: std::collections::BTreeSet<EntityId>,
    locomotion_refresh_scratch: Vec<EntityId>,
    physical_resource_outcomes: VecDeque<std::sync::Arc<crate::PhysicalResourceOutcome>>,
    magic_resources: crate::magic_resource_commands::MagicResourceCommands,
    npc_combat_assets:
        std::collections::BTreeMap<EntityId, std::sync::Arc<crate::PreparedNpcCombatAssets>>,
    npc_combat_staging: std::collections::BTreeMap<EntityId, Vec<EntityId>>,
    physical_resources:
        std::collections::BTreeMap<u64, crate::physical_resources::PhysicalResourcePending>,
    inventory: crate::inventory::Inventory,
    equipment_mana: crate::equipment_mana::EquipmentMana,
    item_experience: crate::item_experience::ItemExperienceState,
    inventory_placements: inventory_placement::InventoryPlacements,
    inventory_registry_reservations: std::collections::BTreeMap<u64, Vec<EntityId>>,
    housing_inventory_bindings: std::collections::BTreeMap<u64, u64>,
    housing: crate::housing::Housing,
    cast_outcomes: VecDeque<bace_gameplay_api::CastOutcome>,
    registry_revisions: std::collections::BTreeMap<EntityId, u64>,
    recovery_revisions: std::collections::BTreeMap<EntityId, u64>,
    physical_recovery_deadlines: std::collections::BTreeMap<EntityId, f64>,
    vital_inputs: std::collections::BTreeMap<EntityId, PreparedVitalInputs>,
    item_proc_inflight: Vec<bace_gameplay_api::CastOrigin>,
    item_proc_blocked: Vec<bace_gameplay_api::CastOrigin>,
    item_proc_exclusions: Vec<bace_gameplay_api::CastOrigin>,
    physical_refresh_dirty: std::collections::BTreeSet<EntityId>,
    physical_refresh_scratch: Vec<EntityId>,
    pending_magic_damage: Option<CombatEvent>,
    door_outcomes: VecDeque<DoorOutcome>,
}

impl Kernel {
    pub fn new(world: World, capacity: usize) -> Result<Self, SimulationError> {
        Self::with_gameplay_limits(world, capacity, capacity.min(4096), capacity)
    }
    pub fn with_gameplay_limits(
        world: World,
        capacity: usize,
        character_capacity: usize,
        outcome_capacity: usize,
    ) -> Result<Self, SimulationError> {
        if !(1..=65536).contains(&capacity)
            || !(1..=4096).contains(&character_capacity)
            || !(1..=65536).contains(&outcome_capacity)
        {
            return Err(SimulationError::Capacity);
        }
        Ok(Self {
            recalls: crate::recalls::Recalls::new(outcome_capacity),
            corpse_expiry: crate::corpse_expiry::CorpseExpiries::new(outcome_capacity),
            player_deaths: crate::player_death::PlayerDeaths::new(outcome_capacity),
            social: crate::social::SocialState::new(outcome_capacity),
            fellowships: crate::fellowships::Fellowships::new(outcome_capacity),
            allegiances: crate::allegiances::Allegiances::new(outcome_capacity),
            staff: crate::staff::StaffState::new(outcome_capacity.min(4096))
                .expect("bounded staff capacity"),
            region_unloads: Default::default(),
            region_content_heads: Default::default(),
            region_plain_roots: Default::default(),
            region_residency: crate::RegionResidency::new(1024).expect("fixed valid capacity"),
            region_activity_scratch: Vec::with_capacity(outcome_capacity),
            world_policies: Default::default(),
            recall_policy: Default::default(),
            death_policy: Default::default(),
            generated_enchantments: Default::default(),
            constructed_creatures: Default::default(),
            generated_retirements: Default::default(),
            generated_vendors: Default::default(),
            vendor_outcomes: VecDeque::with_capacity(outcome_capacity),
            generator_outcomes: VecDeque::with_capacity(outcome_capacity),
            avatar_locomotion: Default::default(),
            player_world_seen: Default::default(),
            player_world_dirty: Default::default(),
            player_world_scratch: Vec::with_capacity(outcome_capacity),
            portals: portals::PortalServices::new(outcome_capacity),
            pets: crate::pets::Pets::new(outcome_capacity),
            pet_outcomes: VecDeque::with_capacity(outcome_capacity),
            crafting: crate::crafting::Crafting::new(outcome_capacity),
            inventory_commands: inventory_commands::InventoryCommands::new(outcome_capacity),
            skill_devices: crate::skill_devices::SkillDevices::new(outcome_capacity),
            attribute_transfers: crate::attribute_transfer::AttributeTransfers::new(
                outcome_capacity,
            ),
            world,
            commands: VecDeque::new(),
            capacity,
            tick: 0,
            characters: Characters::new(character_capacity),
            progression_outcomes: VecDeque::with_capacity(outcome_capacity),
            skill_outcomes: VecDeque::with_capacity(outcome_capacity),
            crafting_outcomes: VecDeque::with_capacity(outcome_capacity),
            ui_outcomes: VecDeque::with_capacity(outcome_capacity),
            skill_device_outcomes: VecDeque::with_capacity(outcome_capacity),
            attribute_transfer_outcomes: VecDeque::with_capacity(outcome_capacity),
            outcome_capacity,
            combat: Combat::new(outcome_capacity),
            combat_outcomes: VecDeque::with_capacity(outcome_capacity),
            combat_events: VecDeque::with_capacity(outcome_capacity),
            generators: crate::generators::Generators::new(outcome_capacity),
            population: Population::new(outcome_capacity),
            doors: Doors::new(outcome_capacity),
            npcs: crate::npc::Npcs::new(outcome_capacity),
            magic: crate::magic::Magic::new(outcome_capacity),
            player_admission_outcomes: VecDeque::with_capacity(1),
            player_entered_outcomes: VecDeque::with_capacity(1),
            player_detach_outcomes: VecDeque::with_capacity(1),
            region_admission_outcomes: VecDeque::with_capacity(1),
            player_snapshot_outcomes: VecDeque::with_capacity(1),
            pve_service_outcomes: VecDeque::with_capacity(outcome_capacity),
            npc_service_outcomes: VecDeque::with_capacity(outcome_capacity),
            visibility_outcomes: VecDeque::with_capacity(1),
            object_view_outcomes: VecDeque::with_capacity(1),
            locomotion_outcomes: VecDeque::with_capacity(1),
            locomotion_dirty: Default::default(),
            locomotion_refresh_scratch: Vec::new(),
            physical_resource_outcomes: VecDeque::with_capacity(1),
            magic_resources: crate::magic_resource_commands::MagicResourceCommands::new(8),
            npc_combat_assets: Default::default(),
            npc_combat_staging: Default::default(),
            physical_resources: Default::default(),
            inventory: crate::inventory::Inventory::new(outcome_capacity),
            equipment_mana: crate::equipment_mana::EquipmentMana::new(4096),
            social_gags: Default::default(),
            item_experience: crate::item_experience::ItemExperienceState::new(4096),
            inventory_placements: inventory_placement::InventoryPlacements::new(outcome_capacity),
            inventory_registry_reservations: std::collections::BTreeMap::new(),
            housing_inventory_bindings: std::collections::BTreeMap::new(),
            housing: crate::housing::Housing::new(outcome_capacity),
            cast_outcomes: VecDeque::with_capacity(outcome_capacity),
            registry_revisions: Default::default(),
            recovery_revisions: Default::default(),
            physical_recovery_deadlines: Default::default(),
            vital_inputs: Default::default(),
            item_proc_inflight: Vec::with_capacity(32),
            item_proc_blocked: Vec::with_capacity(32),
            item_proc_exclusions: Vec::with_capacity(64),
            physical_refresh_dirty: Default::default(),
            physical_refresh_scratch: Vec::with_capacity(character_capacity),
            pending_magic_damage: None,
            door_outcomes: VecDeque::with_capacity(outcome_capacity),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("command/outcome capacity must be 1..=65536 and character capacity 1..=4096")]
    Capacity,
    #[error("bounded command queue is full")]
    QueueFull,
    #[error("owned command exceeds admission bounds")]
    InvalidCommand,
    #[error("simulation tick counter exhausted")]
    TickOverflow,
    #[error("persistent registry revision exhausted")]
    AuxiliaryRevision,
    #[error("accepted character skills require profile recovery")]
    SkillRefresh,
    #[error(transparent)]
    World(#[from] WorldError),
}

mod magic_components;
#[cfg(test)]
mod magic_components_fixture;

mod physical_combat;

mod recovery;

mod pet_commands;
mod pets;

mod npc_services;

mod portals;
pub use portals::{
    PortalAcceptedView, PortalServiceEffect, PortalServiceEvent, PortalServiceOrigin,
    PortalServiceReceipt, PortalServiceTicket,
};

mod player_world;

mod generated_inventory;
mod generated_item_trees;
mod generator_mixed;
mod generator_npc_staging;

mod generator_births;
mod generator_commands;
mod generator_request_ids;
mod generator_scripts;

mod vendor_buy;
mod vendor_generators;
mod vendor_lazy;
pub use vendor_buy::{VendorBuyError, VendorBuyReceipt, VendorBuyReservation, VendorBuySource};
mod vendor_trees;

mod generator_item_lifecycle;

mod staff_audit;
mod staff_commands;

mod region_items;
mod region_unload;
pub use region_items::{
    PreparedResidentRegion, PreparedWorldCorpse, PreparedWorldRegionItems, PreparedWorldRegionRoot,
    ResidentRegionRefresh,
};

mod staff_magic;

mod staff_spells;

mod staff_buffs;

mod corpse_expiry;

mod staff_rewards;

mod player_detach;

mod staff_broadcast;

mod social_gags;

mod staff_selection;
