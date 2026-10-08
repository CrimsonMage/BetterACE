//! One operating-system thread owns the kernel and all authoritative mutation.
//! Network/database adapters may only submit bounded commands; they never borrow
//! the world. This runtime boundary currently drives the synthetic kernel.
use bace_gameplay_api::CastOutcome;
use bace_gameplay_api::ServerCastOutcome;
use bace_gameplay_api::weapon_combat::PhysicalLaunchProposal;
use bace_gameplay_api::{CombatOutcome, DoorOutcome, ProgressionOutcome};
use bace_simulation::PhysicalCombatEvent;
use bace_simulation::VendorOutcome;
use bace_simulation::{AttributeTransferDeviceTicket, AttributeTransferOutcome};
use bace_simulation::{
    CombatEvent, Command, DeathProposal, DoorEvent, Kernel, NpcNotification, NpcProposal, PveEvent,
    SimulationError,
};
use bace_simulation::{
    CraftingOutcome, CraftingTicket, SkillDeviceOutcome, SkillDeviceTicket, SkillOutcome, UiOutcome,
};
use bace_simulation::{
    GeneratedRetirementTicket, GeneratorCommandOutcome, GeneratorHostRequest, GeneratorWorldEvent,
};
use bace_simulation::{HousingTicket, InventoryTicket, MagicEvent};
use bace_simulation::{PetEvent, PetOutcome, PortalServiceEvent, PortalServiceTicket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

mod death_outputs;
mod interaction_outputs;
mod player_admissions;
mod region_admissions;
pub use region_admissions::RecoveredRegionAdmissions;
mod inventory_outputs;
mod npc_service_outputs;
mod player_snapshots;
mod pve_service_outputs;
pub use inventory_outputs::RecoveredInventory;
mod locomotion_outputs;
mod magic_resource_outputs;
mod object_view_outputs;
mod physical_resource_outputs;
mod visibility_outputs;
pub use locomotion_outputs::RecoveredLocomotion;
pub use magic_resource_outputs::RecoveredMagicResource;
pub use npc_service_outputs::RecoveredNpcServices;
pub use object_view_outputs::RecoveredObjectView;
pub use physical_resource_outputs::RecoveredPhysicalResource;
pub use player_admissions::RecoveredPlayerAdmissions;
pub use player_snapshots::RecoveredPlayerSnapshots;
pub use visibility_outputs::RecoveredVisibility;
mod regions;
pub use pve_service_outputs::RecoveredPveServices;
mod social;
pub use death_outputs::RecoveredPlayerDeaths;
pub use interaction_outputs::RecoveredInteractions;
pub use regions::RecoveredRegions;
pub use social::RecoveredSocial;

const STEP: Duration = Duration::from_nanos(1_000_000_000 / 30);
const MAX_CAPACITY: usize = 65_536;

pub struct SimulationConfig {
    pub command_capacity: usize,
    /// None runs until explicit shutdown. A limit is useful for replay/exercises.
    pub tick_limit: Option<u64>,
    /// Unpaced execution is for synthetic measurement/replay only.
    pub real_time: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            command_capacity: 8192,
            tick_limit: None,
            real_time: true,
        }
    }
}

/// A bounded sender returns ownership of commands on overload/disconnection.
#[derive(Clone)]
pub struct SimulationInput {
    sender: mpsc::SyncSender<Command>,
    admission: Arc<Mutex<bool>>,
}

impl SimulationInput {
    /// Success acknowledges enqueueing only, not validation or application.
    /// Shutdown may discard queued commands; the final report counts them.
    /// Never waits for channel capacity; a short admission gate serializes
    /// senders with closure, and is never acquired during a kernel tick.
    #[expect(
        clippy::result_large_err,
        reason = "bounded admission returns command ownership without allocating per retry"
    )]
    pub fn try_submit(&self, command: Command) -> Result<(), mpsc::TrySendError<Command>> {
        if matches!(&command,Command::AdmitResidentRegion(c) if !c.valid_bounds())
            || matches!(&command,Command::AdmitPlayer(c) if !c.valid_bounds())
            || matches!(&command,Command::DetachPlayer(c) if !c.valid_bounds())
            || matches!(&command,Command::PveService(c) if !c.valid_bounds())
            || matches!(&command,Command::NpcService(c) if !c.valid_bounds())
            || matches!(&command,Command::Visibility(c) if !c.valid_bounds())
            || matches!(&command,Command::ObjectView(c) if !c.valid_bounds())
            || matches!(&command,Command::PhysicalResource(c) if !c.valid_bounds())
            || matches!(&command,Command::MagicResource(c) if !c.valid_bounds())
            || !social::bounded(&command)
            || matches!(&command, Command::Crafting(c) if c.validate_bounds().is_err())
            || matches!(&command, Command::Inventory(c) if !c.valid_bounds())
            || matches!(&command, Command::Generator(c) if !c.valid_bounds())
            || matches!(&command, Command::Vendor(c) if !c.valid_bounds())
            || matches!(&command, Command::CorpseConsent(c) if !c.valid_bounds())
        {
            return Err(mpsc::TrySendError::Full(command));
        }
        let open = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if !*open {
            return Err(mpsc::TrySendError::Disconnected(command));
        }
        self.sender.try_send(command)
    }
}

/// Closes admission on every exit, including errors and unwinding.
struct SimulationInbox {
    receiver: mpsc::Receiver<Command>,
    admission: Arc<Mutex<bool>>,
}

impl SimulationInbox {
    fn close_and_recover(&self) -> Vec<Command> {
        *self.admission.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.receiver.try_iter().collect()
    }
    fn close_and_discard(&self) -> u64 {
        self.close_and_recover().len() as u64
    }
}

impl Drop for SimulationInbox {
    fn drop(&mut self) {
        self.close_and_discard();
    }
}

#[must_use = "use shutdown_recover to retain character state and unapplied work"]
pub struct SimulationWorker {
    social: social::SocialReceivers,
    player_deaths: death_outputs::DeathReceivers,
    interactions: interaction_outputs::InteractionReceivers,
    regions: regions::RegionReceivers,
    pve_services: pve_service_outputs::PveServiceReceivers,
    npc_services: npc_service_outputs::NpcServiceReceivers,
    inventory_output: inventory_outputs::InventoryReceivers,
    visibility: visibility_outputs::VisibilityReceivers,
    object_view: object_view_outputs::ObjectViewReceivers,
    locomotion: locomotion_outputs::LocomotionReceivers,
    physical_resource: physical_resource_outputs::PhysicalResourceReceivers,
    magic_resource: magic_resource_outputs::MagicResourceReceivers,
    player_snapshots: player_snapshots::PlayerSnapshotReceivers,
    player_admissions: player_admissions::PlayerAdmissionReceivers,
    region_admissions: region_admissions::RegionAdmissionReceivers,
    input: SimulationInput,
    stop: Arc<AtomicBool>,
    finite: bool,
    thread: Option<JoinHandle<SimulationExit>>,
    outcomes: mpsc::Receiver<ProgressionOutcome>,
    combat_outcomes: mpsc::Receiver<CombatOutcome>,
    combat_events: mpsc::Receiver<CombatEvent>,
    death_proposals: mpsc::Receiver<DeathProposal>,
    pve_events: mpsc::Receiver<PveEvent>,
    door_outcomes: mpsc::Receiver<DoorOutcome>,
    door_events: mpsc::Receiver<DoorEvent>,
    npc_proposals: mpsc::Receiver<NpcProposal>,
    npc_notifications: mpsc::Receiver<NpcNotification>,
    cast_outcomes: mpsc::Receiver<CastOutcome>,
    magic_events: mpsc::Receiver<MagicEvent>,
    inventory_proposals: mpsc::Receiver<InventoryTicket>,
    physical_launches: mpsc::Receiver<PhysicalLaunchProposal>,
    physical_events: mpsc::Receiver<PhysicalCombatEvent>,
    server_cast_outcomes: mpsc::Receiver<ServerCastOutcome>,
    skill_outcomes: mpsc::Receiver<SkillOutcome>,
    skill_device_outcomes: mpsc::Receiver<SkillDeviceOutcome>,
    skill_device_proposals: mpsc::Receiver<SkillDeviceTicket>,
    attribute_transfer_outcomes: mpsc::Receiver<AttributeTransferOutcome>,
    attribute_transfer_proposals: mpsc::Receiver<AttributeTransferDeviceTicket>,
    crafting_proposals: mpsc::Receiver<CraftingTicket>,
    crafting_outcomes: mpsc::Receiver<CraftingOutcome>,
    ui_outcomes: mpsc::Receiver<UiOutcome>,
    pet_events: mpsc::Receiver<PetEvent>,
    pet_outcomes: mpsc::Receiver<PetOutcome>,
    vendor_outcomes: mpsc::Receiver<VendorOutcome>,
    portal_proposals: mpsc::Receiver<PortalServiceTicket>,
    portal_events: mpsc::Receiver<PortalServiceEvent>,
    generator_requests: mpsc::Receiver<GeneratorHostRequest>,
    generator_events: mpsc::Receiver<GeneratorWorldEvent>,
    generator_outcomes: mpsc::Receiver<GeneratorCommandOutcome>,
    generator_retirements: mpsc::Receiver<GeneratedRetirementTicket>,
    housing_proposals: mpsc::Receiver<HousingTicket>,
}

mod recovery;
pub use recovery::SimulationExit;

#[derive(Debug)]
pub struct SimulationReport {
    pub thread_id: thread::ThreadId,
    pub ticks: u64,
    pub rejected_commands: u64,
    /// Enqueued adapter commands left unprocessed at normal worker exit.
    /// Distinct from commands processed but rejected by the kernel.
    pub discarded_commands: u64,
    pub missed_deadlines: u64,
    pub max_tick: Duration,
    /// Upper bound of the 100-microsecond p99 histogram bucket; None means
    /// no ticks ran or p99 exceeded the final 25.5 ms bucket.
    pub p99_upper_bound: Option<Duration>,
}

/// Startup failure returns the only kernel owner, including unsaved revisions.
pub struct SimulationStartupFailure {
    pub kernel: Kernel,
    pub reason: String,
}
impl std::fmt::Debug for SimulationStartupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulationStartupFailure")
            .field("reason", &self.reason)
            .field("has_characters", &self.kernel.has_characters())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("command capacity must be 1..=65536 and tick limit must be nonzero")]
    Configuration,
    #[error("simulation startup failed; kernel ownership is retained: {0:?}")]
    StartupRecovery(Box<SimulationStartupFailure>),
    #[error("simulation state requires explicit owner recovery; no save drain was acknowledged")]
    RecoveryRequired(Box<SimulationExit>),
    #[error("wait requires a finite tick limit; the unbounded worker was stopped and joined")]
    UnboundedWait,
    #[error("simulation thread could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("simulation thread panicked")]
    Panic,
    #[error(transparent)]
    Simulation(#[from] SimulationError),
}

impl SimulationWorker {
    pub fn spawn(kernel: Kernel, config: SimulationConfig) -> Result<Self, WorkerError> {
        let output_capacity = config.command_capacity;
        Self::spawn_with_output_capacity(kernel, config, output_capacity)
    }
    pub fn spawn_with_output_capacity(
        kernel: Kernel,
        config: SimulationConfig,
        output_capacity: usize,
    ) -> Result<Self, WorkerError> {
        if !(1..=MAX_CAPACITY).contains(&config.command_capacity)
            || config.tick_limit == Some(0)
            || !(1..=MAX_CAPACITY).contains(&output_capacity)
        {
            return Err(WorkerError::StartupRecovery(Box::new(
                SimulationStartupFailure {
                    kernel,
                    reason: "invalid command/output capacity or tick limit".into(),
                },
            )));
        }
        let (social_output, social) = social::channels(output_capacity);
        let (player_death_output, player_deaths) = death_outputs::channels(output_capacity);
        let (interaction_output, interactions) = interaction_outputs::channels(output_capacity);
        let (region_output, regions) = regions::channels(output_capacity);
        let (player_admission_output, player_admissions) = player_admissions::channels(1);
        let (region_admission_output, region_admissions) = region_admissions::channels(1);
        let (player_snapshot_output, player_snapshots) = player_snapshots::channels(1);
        let (pve_service_output, pve_services) = pve_service_outputs::channels(output_capacity);
        let (npc_service_output, npc_services) = npc_service_outputs::channels(output_capacity);
        let (inventory_sender, inventory_output) = inventory_outputs::channels(output_capacity);
        let (visibility_output, visibility) = visibility_outputs::channels(1);
        let (object_view_output, object_view) = object_view_outputs::channels(1);
        let (locomotion_output, locomotion) = locomotion_outputs::channels(1);
        let (physical_resource_output, physical_resource) = physical_resource_outputs::channels(1);
        let (magic_resource_output, magic_resource) = magic_resource_outputs::channels(1);
        let (sender, receiver) = mpsc::sync_channel(config.command_capacity);
        let admission = Arc::new(Mutex::new(true));
        let inbox = SimulationInbox {
            receiver,
            admission: Arc::clone(&admission),
        };
        let (outcome_sender, outcomes) = mpsc::sync_channel(output_capacity);
        let (combat_output, combat_outcomes) = mpsc::sync_channel(output_capacity);
        let (combat_event_output, combat_events) = mpsc::sync_channel(output_capacity);
        let (death_output, death_proposals) = mpsc::sync_channel(output_capacity);
        let (pve_output, pve_events) = mpsc::sync_channel(output_capacity);
        let (door_output, door_outcomes) = mpsc::sync_channel(output_capacity);
        let (door_event_output, door_events) = mpsc::sync_channel(output_capacity);
        let (npc_proposals_output, npc_proposals) = mpsc::sync_channel(output_capacity);
        let (npc_notifications_output, npc_notifications) = mpsc::sync_channel(output_capacity);
        let (cast_outcomes_output, cast_outcomes) = mpsc::sync_channel(output_capacity);
        let (magic_events_output, magic_events) = mpsc::sync_channel(output_capacity);
        let (inventory_proposals_output, inventory_proposals) = mpsc::sync_channel(output_capacity);
        let (physical_launches_output, physical_launches) = mpsc::sync_channel(output_capacity);
        let (physical_events_output, physical_events) = mpsc::sync_channel(output_capacity);
        let (server_cast_outcomes_output, server_cast_outcomes) =
            mpsc::sync_channel(output_capacity);
        let (skill_outcomes_output, skill_outcomes) = mpsc::sync_channel(output_capacity);
        let (skill_device_outcomes_output, skill_device_outcomes) =
            mpsc::sync_channel(output_capacity);
        let (skill_device_proposals_output, skill_device_proposals) =
            mpsc::sync_channel(output_capacity);
        let (attribute_transfer_outcomes_output, attribute_transfer_outcomes) =
            mpsc::sync_channel(output_capacity);
        let (attribute_transfer_proposals_output, attribute_transfer_proposals) =
            mpsc::sync_channel(output_capacity);
        let (crafting_proposals_output, crafting_proposals) = mpsc::sync_channel(output_capacity);
        let (crafting_outcomes_output, crafting_outcomes) = mpsc::sync_channel(output_capacity);
        let (ui_outcomes_output, ui_outcomes) = mpsc::sync_channel(output_capacity);
        let (pet_events_output, pet_events) = mpsc::sync_channel(output_capacity);
        let (pet_outcomes_output, pet_outcomes) = mpsc::sync_channel(output_capacity);
        let (vendor_outcomes_output, vendor_outcomes) = mpsc::sync_channel(output_capacity);
        let (portal_proposals_output, portal_proposals) = mpsc::sync_channel(output_capacity);
        let (portal_events_output, portal_events) = mpsc::sync_channel(output_capacity);
        let (generator_requests_output, generator_requests) = mpsc::sync_channel(output_capacity);
        let (generator_events_output, generator_events) = mpsc::sync_channel(output_capacity);
        let (generator_outcomes_output, generator_outcomes) = mpsc::sync_channel(output_capacity);
        let (generator_retirements_output, generator_retirements) =
            mpsc::sync_channel(output_capacity);
        let (housing_proposals_output, housing_proposals) = mpsc::sync_channel(output_capacity);
        let finite = config.tick_limit.is_some();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        // One startup-only handoff keeps state recoverable if OS thread creation
        // fails. Neither mutex nor shared kernel storage survives into ticks.
        let handoff = Arc::new(Mutex::new(Some(kernel)));
        let receive_kernel = handoff.clone();
        let thread = match thread::Builder::new()
            .name("bace-simulation".into())
            .spawn(move || {
                let kernel = receive_kernel
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("single kernel startup handoff");
                drop(receive_kernel);
                drive_owned(
                    kernel,
                    inbox,
                    worker_stop,
                    config,
                    SimulationOutputs {
                        social: social_output,
                        player_deaths: player_death_output,
                        interactions: interaction_output,
                        regions: region_output,
                        pve_services: pve_service_output,
                        npc_services: npc_service_output,
                        inventory_output: inventory_sender,
                        visibility: visibility_output,
                        object_view: object_view_output,
                        locomotion: locomotion_output,
                        physical_resource: physical_resource_output,
                        magic_resource: magic_resource_output,
                        player_snapshots: player_snapshot_output,
                        player_admissions: player_admission_output,
                        region_admissions: region_admission_output,
                        progression: outcome_sender,
                        combat: combat_output,
                        events: combat_event_output,
                        deaths: death_output,
                        pve: pve_output,
                        doors: door_output,
                        door_events: door_event_output,
                        npc_proposals: npc_proposals_output,
                        npc_notifications: npc_notifications_output,
                        cast_outcomes: cast_outcomes_output,
                        magic_events: magic_events_output,
                        inventory_proposals: inventory_proposals_output,
                        physical_launches: physical_launches_output,
                        physical_events: physical_events_output,
                        server_cast_outcomes: server_cast_outcomes_output,
                        skill_outcomes: skill_outcomes_output,
                        skill_device_outcomes: skill_device_outcomes_output,
                        skill_device_proposals: skill_device_proposals_output,
                        attribute_transfer_outcomes: attribute_transfer_outcomes_output,
                        attribute_transfer_proposals: attribute_transfer_proposals_output,
                        crafting_proposals: crafting_proposals_output,
                        crafting_outcomes: crafting_outcomes_output,
                        ui_outcomes: ui_outcomes_output,
                        pet_events: pet_events_output,
                        pet_outcomes: pet_outcomes_output,
                        vendor_outcomes: vendor_outcomes_output,
                        portal_proposals: portal_proposals_output,
                        portal_events: portal_events_output,
                        generator_requests: generator_requests_output,
                        generator_events: generator_events_output,
                        generator_outcomes: generator_outcomes_output,
                        generator_retirements: generator_retirements_output,
                        housing_proposals: housing_proposals_output,
                    },
                )
            }) {
            Ok(thread) => thread,
            Err(error) => {
                let kernel = handoff
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("failed spawn retains kernel");
                return Err(WorkerError::StartupRecovery(Box::new(
                    SimulationStartupFailure {
                        kernel,
                        reason: error.to_string(),
                    },
                )));
            }
        };
        Ok(Self {
            social,
            player_deaths,
            interactions,
            regions,
            pve_services,
            npc_services,
            inventory_output,
            visibility,
            object_view,
            locomotion,
            physical_resource,
            magic_resource,
            player_snapshots,
            player_admissions,
            region_admissions,
            input: SimulationInput { sender, admission },
            stop,
            finite,
            thread: Some(thread),
            outcomes,
            combat_outcomes,
            combat_events,
            death_proposals,
            pve_events,
            door_outcomes,
            door_events,
            npc_proposals,
            npc_notifications,
            cast_outcomes,
            magic_events,
            inventory_proposals,
            physical_launches,
            physical_events,
            server_cast_outcomes,
            skill_outcomes,
            skill_device_outcomes,
            skill_device_proposals,
            attribute_transfer_outcomes,
            attribute_transfer_proposals,
            crafting_proposals,
            crafting_outcomes,
            ui_outcomes,
            pet_events,
            pet_outcomes,
            vendor_outcomes,
            portal_proposals,
            portal_events,
            generator_requests,
            generator_events,
            generator_outcomes,
            generator_retirements,
            housing_proposals,
        })
    }

    pub fn npc_proposals(&self) -> &mpsc::Receiver<NpcProposal> {
        &self.npc_proposals
    }
    pub fn npc_notifications(&self) -> &mpsc::Receiver<NpcNotification> {
        &self.npc_notifications
    }
    pub fn cast_outcomes(&self) -> &mpsc::Receiver<CastOutcome> {
        &self.cast_outcomes
    }
    pub fn magic_events(&self) -> &mpsc::Receiver<MagicEvent> {
        &self.magic_events
    }
    pub fn server_cast_outcomes(&self) -> &mpsc::Receiver<ServerCastOutcome> {
        &self.server_cast_outcomes
    }
    pub fn physical_events(&self) -> &mpsc::Receiver<PhysicalCombatEvent> {
        &self.physical_events
    }
    pub fn physical_launches(&self) -> &mpsc::Receiver<PhysicalLaunchProposal> {
        &self.physical_launches
    }
    pub fn inventory_proposals(&self) -> &mpsc::Receiver<InventoryTicket> {
        &self.inventory_proposals
    }
    pub fn skill_outcomes(&self) -> &mpsc::Receiver<SkillOutcome> {
        &self.skill_outcomes
    }
    pub fn skill_device_outcomes(&self) -> &mpsc::Receiver<SkillDeviceOutcome> {
        &self.skill_device_outcomes
    }
    pub fn skill_device_proposals(&self) -> &mpsc::Receiver<SkillDeviceTicket> {
        &self.skill_device_proposals
    }
    pub fn attribute_transfer_outcomes(&self) -> &mpsc::Receiver<AttributeTransferOutcome> {
        &self.attribute_transfer_outcomes
    }
    pub fn attribute_transfer_proposals(&self) -> &mpsc::Receiver<AttributeTransferDeviceTicket> {
        &self.attribute_transfer_proposals
    }
    pub fn crafting_proposals(&self) -> &mpsc::Receiver<CraftingTicket> {
        &self.crafting_proposals
    }
    pub fn crafting_outcomes(&self) -> &mpsc::Receiver<CraftingOutcome> {
        &self.crafting_outcomes
    }
    pub fn ui_outcomes(&self) -> &mpsc::Receiver<UiOutcome> {
        &self.ui_outcomes
    }
    pub fn pet_events(&self) -> &mpsc::Receiver<PetEvent> {
        &self.pet_events
    }
    pub fn pet_outcomes(&self) -> &mpsc::Receiver<PetOutcome> {
        &self.pet_outcomes
    }
    pub fn vendor_outcomes(&self) -> &mpsc::Receiver<VendorOutcome> {
        &self.vendor_outcomes
    }
    pub fn portal_proposals(&self) -> &mpsc::Receiver<PortalServiceTicket> {
        &self.portal_proposals
    }
    pub fn portal_events(&self) -> &mpsc::Receiver<PortalServiceEvent> {
        &self.portal_events
    }
    pub fn generator_requests(&self) -> &mpsc::Receiver<GeneratorHostRequest> {
        &self.generator_requests
    }
    pub fn generator_events(&self) -> &mpsc::Receiver<GeneratorWorldEvent> {
        &self.generator_events
    }
    pub fn generator_outcomes(&self) -> &mpsc::Receiver<GeneratorCommandOutcome> {
        &self.generator_outcomes
    }
    pub fn generator_retirements(&self) -> &mpsc::Receiver<GeneratedRetirementTicket> {
        &self.generator_retirements
    }
    pub fn housing_proposals(&self) -> &mpsc::Receiver<HousingTicket> {
        &self.housing_proposals
    }
    pub fn input(&self) -> SimulationInput {
        self.input.clone()
    }

    pub fn outcomes(&self) -> &mpsc::Receiver<ProgressionOutcome> {
        &self.outcomes
    }

    /// Ordered correlated combat replies. No network or save success is implied.
    pub fn combat_outcomes(&self) -> &mpsc::Receiver<CombatOutcome> {
        &self.combat_outcomes
    }
    /// Authoritative impacts/completions in simulation order. This separate stream
    /// is not a total ordering against correlated request replies.
    pub fn combat_events(&self) -> &mpsc::Receiver<CombatEvent> {
        &self.combat_events
    }

    /// Valuable death/loot/XP proposals must acquire durable operation IDs and
    /// commit before success is returned to the simulation owner.
    pub fn death_proposals(&self) -> &mpsc::Receiver<DeathProposal> {
        &self.death_proposals
    }
    pub fn pve_events(&self) -> &mpsc::Receiver<PveEvent> {
        &self.pve_events
    }

    pub fn door_outcomes(&self) -> &mpsc::Receiver<DoorOutcome> {
        &self.door_outcomes
    }
    pub fn door_events(&self) -> &mpsc::Receiver<DoorEvent> {
        &self.door_events
    }

    /// Stops the single owner and returns all authoritative state, unapplied
    /// adapter requests and undelivered results. Caller still owns save drain.
    /// Observable worker termination only; this never joins or discards the
    /// retained kernel/output recovery value.
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }
    pub fn shutdown_recover(mut self) -> Result<SimulationExit, WorkerError> {
        self.request_stop();
        self.join_owned()
    }
    pub fn wait_recover(mut self) -> Result<SimulationExit, WorkerError> {
        if !self.finite {
            self.request_stop();
        }
        self.join_owned()
    }

    /// Close admission, finish any current tick, count queued discards and join.
    /// Persistence has its own drain protocol; this does not acknowledge saves.
    pub fn shutdown(mut self) -> Result<SimulationReport, WorkerError> {
        self.request_stop();
        self.join()
    }

    /// Wait for a configured finite exercise/replay to finish.
    /// An unbounded worker is stopped and joined before returning an error.
    pub fn wait(mut self) -> Result<SimulationReport, WorkerError> {
        if !self.finite {
            self.request_stop();
            let exit = self.join_owned()?;
            if exit.requires_recovery() {
                return Err(WorkerError::RecoveryRequired(Box::new(exit)));
            }
            return Err(WorkerError::UnboundedWait);
        }
        self.join()
    }

    fn request_stop(&self) {
        *self
            .input
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = false;
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    fn join_owned(&mut self) -> Result<SimulationExit, WorkerError> {
        let mut exit = self
            .thread
            .take()
            .ok_or(WorkerError::Panic)?
            .join()
            .map_err(|_| WorkerError::Panic)?;
        // Channel results precede the retained overflow result and kernel outbox.
        let mut delivered: Vec<_> = self.outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_outcomes);
        exit.undelivered_outcomes = delivered;
        let mut combat: Vec<_> = self.combat_outcomes.try_iter().collect();
        combat.append(&mut exit.undelivered_combat_outcomes);
        exit.undelivered_combat_outcomes = combat;
        let mut events: Vec<_> = self.combat_events.try_iter().collect();
        events.append(&mut exit.undelivered_combat_events);
        exit.undelivered_combat_events = events;
        let mut deaths: Vec<_> = self.death_proposals.try_iter().collect();
        deaths.append(&mut exit.undelivered_death_proposals);
        exit.undelivered_death_proposals = deaths;
        let mut pve: Vec<_> = self.pve_events.try_iter().collect();
        pve.append(&mut exit.undelivered_pve_events);
        exit.undelivered_pve_events = pve;
        let mut doors: Vec<_> = self.door_outcomes.try_iter().collect();
        doors.append(&mut exit.undelivered_door_outcomes);
        exit.undelivered_door_outcomes = doors;
        let mut events: Vec<_> = self.door_events.try_iter().collect();
        events.append(&mut exit.undelivered_door_events);
        exit.undelivered_door_events = events;
        let mut delivered: Vec<_> = self.npc_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_npc_proposals);
        exit.undelivered_npc_proposals = delivered;
        let mut delivered: Vec<_> = self.npc_notifications.try_iter().collect();
        delivered.append(&mut exit.undelivered_npc_notifications);
        exit.undelivered_npc_notifications = delivered;
        let mut delivered: Vec<_> = self.cast_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_cast_outcomes);
        exit.undelivered_cast_outcomes = delivered;
        let mut delivered: Vec<_> = self.magic_events.try_iter().collect();
        delivered.append(&mut exit.undelivered_magic_events);
        exit.undelivered_magic_events = delivered;
        let mut delivered: Vec<_> = self.inventory_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_inventory_proposals);
        exit.undelivered_inventory_proposals = delivered;
        let mut delivered: Vec<_> = self.physical_launches.try_iter().collect();
        delivered.append(&mut exit.undelivered_physical_launches);
        exit.undelivered_physical_launches = delivered;
        let mut delivered: Vec<_> = self.physical_events.try_iter().collect();
        delivered.append(&mut exit.undelivered_physical_events);
        exit.undelivered_physical_events = delivered;
        let mut delivered: Vec<_> = self.server_cast_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_server_cast_outcomes);
        exit.undelivered_server_cast_outcomes = delivered;
        let mut delivered: Vec<_> = self.skill_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_skill_outcomes);
        exit.undelivered_skill_outcomes = delivered;
        let mut delivered: Vec<_> = self.skill_device_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_skill_device_outcomes);
        exit.undelivered_skill_device_outcomes = delivered;
        let mut delivered: Vec<_> = self.skill_device_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_skill_device_proposals);
        exit.undelivered_skill_device_proposals = delivered;
        let mut delivered: Vec<_> = self.attribute_transfer_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_attribute_transfer_outcomes);
        exit.undelivered_attribute_transfer_outcomes = delivered;
        let mut delivered: Vec<_> = self.attribute_transfer_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_attribute_transfer_proposals);
        exit.undelivered_attribute_transfer_proposals = delivered;
        let mut delivered: Vec<_> = self.crafting_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_crafting_proposals);
        exit.undelivered_crafting_proposals = delivered;
        let mut delivered: Vec<_> = self.crafting_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_crafting_outcomes);
        exit.undelivered_crafting_outcomes = delivered;
        let mut delivered: Vec<_> = self.ui_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_ui_outcomes);
        exit.undelivered_ui_outcomes = delivered;
        let mut delivered: Vec<_> = self.pet_events.try_iter().collect();
        delivered.append(&mut exit.undelivered_pet_events);
        exit.undelivered_pet_events = delivered;
        let mut delivered: Vec<_> = self.pet_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_pet_outcomes);
        exit.undelivered_pet_outcomes = delivered;
        let mut delivered: Vec<_> = self.vendor_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_vendor_outcomes);
        exit.undelivered_vendor_outcomes = delivered;
        let mut delivered: Vec<_> = self.portal_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_portal_proposals);
        exit.undelivered_portal_proposals = delivered;
        let mut delivered: Vec<_> = self.portal_events.try_iter().collect();
        delivered.append(&mut exit.undelivered_portal_events);
        exit.undelivered_portal_events = delivered;
        let mut delivered: Vec<_> = self.generator_requests.try_iter().collect();
        delivered.append(&mut exit.undelivered_generator_requests);
        exit.undelivered_generator_requests = delivered;
        let mut delivered: Vec<_> = self.generator_events.try_iter().collect();
        delivered.append(&mut exit.undelivered_generator_events);
        exit.undelivered_generator_events = delivered;
        let mut delivered: Vec<_> = self.generator_outcomes.try_iter().collect();
        delivered.append(&mut exit.undelivered_generator_outcomes);
        exit.undelivered_generator_outcomes = delivered;
        let mut delivered: Vec<_> = self.generator_retirements.try_iter().collect();
        delivered.append(&mut exit.undelivered_generator_retirements);
        exit.undelivered_generator_retirements = delivered;
        let mut delivered: Vec<_> = self.housing_proposals.try_iter().collect();
        delivered.append(&mut exit.undelivered_housing_proposals);
        exit.undelivered_housing_proposals = delivered;
        self.social.recover(&mut exit.social);
        self.player_deaths.recover(&mut exit.player_deaths);
        self.interactions.recover(&mut exit.interactions);
        self.regions.recover(&mut exit.regions);
        self.pve_services.recover(&mut exit.pve_services);
        self.npc_services.recover(&mut exit.npc_services);
        self.inventory_output.recover(&mut exit.inventory_output);
        self.visibility.recover(&mut exit.visibility);
        self.object_view.recover(&mut exit.object_view);
        self.locomotion.recover(&mut exit.locomotion);
        self.physical_resource.recover(&mut exit.physical_resource);
        self.magic_resource.recover(&mut exit.magic_resource);
        self.player_snapshots.recover(&mut exit.player_snapshots);
        self.player_admissions.recover(&mut exit.player_admissions);
        self.region_admissions.recover(&mut exit.region_admissions);
        Ok(exit)
    }
    fn join(&mut self) -> Result<SimulationReport, WorkerError> {
        let exit = self.join_owned()?;
        if exit.requires_recovery() {
            return Err(WorkerError::RecoveryRequired(Box::new(exit)));
        }
        if let Some(error) = exit.failure {
            return Err(WorkerError::Simulation(error));
        }
        Ok(exit.report)
    }
}

impl Drop for SimulationWorker {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

mod driver;
use driver::{SimulationOutputs, drive_owned};

#[cfg(test)]
fn drive(
    kernel: Kernel,
    inbox: SimulationInbox,
    stop: Arc<AtomicBool>,
    config: SimulationConfig,
) -> Result<SimulationReport, SimulationError> {
    let (output, _receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (combat, _combat_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (events, _event_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (deaths, _death_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (pve, _pve_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (doors, _door_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (door_events, _door_event_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (npc_proposals, _npc_proposals_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (npc_notifications, _npc_notifications_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (cast_outcomes, _cast_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (magic_events, _magic_events_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (inventory_proposals, _inventory_proposals_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (physical_launches, _physical_launches_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (physical_events, _physical_events_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (server_cast_outcomes, _server_cast_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (skill_outcomes, _skill_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (skill_device_outcomes, _skill_device_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (skill_device_proposals, _skill_device_proposals_receiver) =
        mpsc::sync_channel(MAX_CAPACITY);
    let (attribute_transfer_outcomes, _attribute_transfer_outcomes_receiver) =
        mpsc::sync_channel(MAX_CAPACITY);
    let (attribute_transfer_proposals, _attribute_transfer_proposals_receiver) =
        mpsc::sync_channel(MAX_CAPACITY);
    let (crafting_proposals, _crafting_proposals_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (crafting_outcomes, _crafting_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (ui_outcomes, _ui_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (pet_events, _pet_events_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (pet_outcomes, _pet_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (vendor_outcomes, _vendor_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (portal_proposals, _portal_proposals_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (portal_events, _portal_events_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (generator_requests, _generator_requests_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (generator_events, _generator_events_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (generator_outcomes, _generator_outcomes_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (generator_retirements, _generator_retirements_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (housing_proposals, _housing_proposals_receiver) = mpsc::sync_channel(MAX_CAPACITY);
    let (social, _social_receivers) = social::channels(MAX_CAPACITY);
    let (player_deaths, _death_receivers) = death_outputs::channels(MAX_CAPACITY);
    let (interactions, _interaction_receivers) = interaction_outputs::channels(MAX_CAPACITY);
    let (regions, _region_receivers) = regions::channels(MAX_CAPACITY);
    let (player_admissions, _player_admission_receivers) = player_admissions::channels(1);
    let (region_admissions, _region_admission_receivers) = region_admissions::channels(1);
    let (player_snapshots, _player_snapshot_receivers) = player_snapshots::channels(1);
    let (pve_services, _pve_service_receivers) = pve_service_outputs::channels(MAX_CAPACITY);
    let (npc_services, _npc_service_receivers) = npc_service_outputs::channels(MAX_CAPACITY);
    let (inventory_output, _inventory_receiver) = inventory_outputs::channels(MAX_CAPACITY);
    let (visibility, _visibility_receivers) = visibility_outputs::channels(1);
    let (object_view, _object_view_receivers) = object_view_outputs::channels(1);
    let (locomotion, _locomotion_receivers) = locomotion_outputs::channels(1);
    let (physical_resource, _physical_resource_receivers) = physical_resource_outputs::channels(1);
    let (magic_resource, _magic_resource_receivers) = magic_resource_outputs::channels(1);
    let exit = drive_owned(
        kernel,
        inbox,
        stop,
        config,
        SimulationOutputs {
            social,
            player_deaths,
            interactions,
            regions,
            pve_services,
            npc_services,
            inventory_output,
            visibility,
            object_view,
            locomotion,
            physical_resource,
            magic_resource,
            player_snapshots,
            player_admissions,
            region_admissions,
            progression: output,
            combat,
            events,
            deaths,
            pve,
            doors,
            door_events,
            npc_proposals,
            npc_notifications,
            cast_outcomes,
            magic_events,
            inventory_proposals,
            physical_launches,
            physical_events,
            server_cast_outcomes,
            skill_outcomes,
            skill_device_outcomes,
            skill_device_proposals,
            attribute_transfer_outcomes,
            attribute_transfer_proposals,
            crafting_proposals,
            crafting_outcomes,
            ui_outcomes,
            pet_events,
            pet_outcomes,
            vendor_outcomes,
            portal_proposals,
            portal_events,
            generator_requests,
            generator_events,
            generator_outcomes,
            generator_retirements,
            housing_proposals,
        },
    );
    match exit.failure {
        Some(error) => Err(error),
        None => Ok(exit.report),
    }
}

#[cfg(test)]
mod tests;
