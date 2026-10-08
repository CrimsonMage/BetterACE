//! Tick pacing, bounded output delivery and recoverable worker exit.
use super::*;

pub(super) struct SimulationOutputs {
    pub(super) social: super::social::SocialSenders,
    pub(super) player_deaths: super::death_outputs::DeathSenders,
    pub(super) interactions: super::interaction_outputs::InteractionSenders,
    pub(super) regions: super::regions::RegionSenders,
    pub(super) region_admissions: super::region_admissions::RegionAdmissionSenders,
    pub(super) player_admissions: super::player_admissions::PlayerAdmissionSenders,
    pub(super) player_snapshots: super::player_snapshots::PlayerSnapshotSenders,
    pub(super) pve_services: super::pve_service_outputs::PveServiceSenders,
    pub(super) npc_services: super::npc_service_outputs::NpcServiceSenders,
    pub(super) inventory_output: super::inventory_outputs::InventorySenders,
    pub(super) visibility: super::visibility_outputs::VisibilitySenders,
    pub(super) object_view: super::object_view_outputs::ObjectViewSenders,
    pub(super) locomotion: super::locomotion_outputs::LocomotionSenders,
    pub(super) physical_resource: super::physical_resource_outputs::PhysicalResourceSenders,
    pub(super) magic_resource: super::magic_resource_outputs::MagicResourceSenders,
    pub(super) progression: mpsc::SyncSender<ProgressionOutcome>,
    pub(super) combat: mpsc::SyncSender<CombatOutcome>,
    pub(super) events: mpsc::SyncSender<CombatEvent>,
    pub(super) deaths: mpsc::SyncSender<DeathProposal>,
    pub(super) pve: mpsc::SyncSender<PveEvent>,
    pub(super) doors: mpsc::SyncSender<DoorOutcome>,
    pub(super) door_events: mpsc::SyncSender<DoorEvent>,
    pub(super) npc_proposals: mpsc::SyncSender<NpcProposal>,
    pub(super) npc_notifications: mpsc::SyncSender<NpcNotification>,
    pub(super) cast_outcomes: mpsc::SyncSender<CastOutcome>,
    pub(super) magic_events: mpsc::SyncSender<MagicEvent>,
    pub(super) inventory_proposals: mpsc::SyncSender<InventoryTicket>,
    pub(super) physical_launches: mpsc::SyncSender<PhysicalLaunchProposal>,
    pub(super) physical_events: mpsc::SyncSender<PhysicalCombatEvent>,
    pub(super) server_cast_outcomes: mpsc::SyncSender<ServerCastOutcome>,
    pub(super) skill_outcomes: mpsc::SyncSender<SkillOutcome>,
    pub(super) skill_device_outcomes: mpsc::SyncSender<SkillDeviceOutcome>,
    pub(super) skill_device_proposals: mpsc::SyncSender<SkillDeviceTicket>,
    pub(super) attribute_transfer_outcomes: mpsc::SyncSender<AttributeTransferOutcome>,
    pub(super) attribute_transfer_proposals: mpsc::SyncSender<AttributeTransferDeviceTicket>,
    pub(super) crafting_proposals: mpsc::SyncSender<CraftingTicket>,
    pub(super) crafting_outcomes: mpsc::SyncSender<CraftingOutcome>,
    pub(super) ui_outcomes: mpsc::SyncSender<UiOutcome>,
    pub(super) pet_events: mpsc::SyncSender<PetEvent>,
    pub(super) pet_outcomes: mpsc::SyncSender<bace_simulation::PetOutcome>,
    pub(super) vendor_outcomes: mpsc::SyncSender<bace_simulation::VendorOutcome>,
    pub(super) portal_proposals: mpsc::SyncSender<PortalServiceTicket>,
    pub(super) portal_events: mpsc::SyncSender<PortalServiceEvent>,
    pub(super) generator_requests: mpsc::SyncSender<GeneratorHostRequest>,
    pub(super) generator_events: mpsc::SyncSender<GeneratorWorldEvent>,
    pub(super) generator_outcomes: mpsc::SyncSender<GeneratorCommandOutcome>,
    pub(super) generator_retirements: mpsc::SyncSender<GeneratedRetirementTicket>,
    pub(super) housing_proposals: mpsc::SyncSender<HousingTicket>,
}
pub(super) fn drive_owned(
    mut kernel: Kernel,
    inbox: SimulationInbox,
    stop: Arc<AtomicBool>,
    config: SimulationConfig,
    outputs: SimulationOutputs,
) -> SimulationExit {
    let mut report = SimulationReport {
        thread_id: thread::current().id(),
        ticks: 0,
        rejected_commands: 0,
        discarded_commands: 0,
        missed_deadlines: 0,
        max_tick: Duration::ZERO,
        p99_upper_bound: None,
    };
    let mut pending_command = None;
    let mut pending_outcome = None;
    let mut pending_combat = None;
    let mut pending_event = None;
    let mut pending_death = None;
    let mut pending_pve = None;
    let mut pending_door = None;
    let mut pending_door_event = None;
    let mut pending_npc_proposals = None;
    let mut pending_npc_notifications = None;
    let mut pending_cast_outcomes = None;
    let mut pending_magic_events = None;
    let mut pending_inventory_proposals = None;
    let mut pending_physical_launches = None;
    let mut pending_physical_events = None;
    let mut pending_server_cast_outcomes = None;
    let mut pending_skill_outcomes = None;
    let mut pending_skill_device_outcomes = None;
    let mut pending_skill_device_proposals = None;
    let mut pending_attribute_transfer_outcomes = None;
    let mut pending_attribute_transfer_proposals = None;
    let mut pending_crafting_proposals = None;
    let mut pending_crafting_outcomes = None;
    let mut pending_ui_outcomes = None;
    let mut pending_pet_events = None;
    let mut pending_pet_outcomes = None;
    let mut pending_vendor_outcomes = None;
    let mut pending_portal_proposals = None;
    let mut pending_portal_events = None;
    let mut pending_generator_requests = None;
    let mut pending_generator_events = None;
    let mut pending_generator_outcomes = None;
    let mut pending_generator_retirements = None;
    let mut pending_housing_proposals = None;
    let mut failure = None;
    let mut histogram = [0_u64; 256];
    let mut deadline = Instant::now();
    while !stop.load(Ordering::Acquire)
        && config.tick_limit.is_none_or(|limit| report.ticks < limit)
    {
        if config.real_time {
            // park may wake spuriously; the deadline remains authoritative.
            while Instant::now() < deadline && !stop.load(Ordering::Acquire) {
                thread::park_timeout(deadline.saturating_duration_since(Instant::now()));
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
        }
        let start = Instant::now();
        outputs.social.flush(&mut kernel, config.command_capacity);
        outputs
            .player_deaths
            .flush(&mut kernel, config.command_capacity);
        outputs
            .interactions
            .flush(&mut kernel, config.command_capacity);
        outputs.regions.flush(&mut kernel, config.command_capacity);
        outputs.player_admissions.flush(&mut kernel, 1);
        outputs.region_admissions.flush(&mut kernel, 1);
        outputs.player_snapshots.flush(&mut kernel, 1);
        outputs
            .pve_services
            .flush(&mut kernel, config.command_capacity);
        outputs
            .npc_services
            .flush(&mut kernel, config.command_capacity);
        outputs
            .inventory_output
            .flush(&mut kernel, config.command_capacity);
        outputs.visibility.flush(&mut kernel, 1);
        outputs.object_view.flush(&mut kernel, 1);
        outputs.locomotion.flush(&mut kernel, 1);
        outputs.physical_resource.flush(&mut kernel, 1);
        outputs.magic_resource.flush(&mut kernel, 1);
        // Preserve output before draining new inputs. A stalled consumer applies
        // backpressure to commands while physics continues at its fixed step.
        flush_outputs(
            || kernel.take_progression_outcome(),
            &outputs.progression,
            &mut pending_outcome,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_combat_outcome(),
            &outputs.combat,
            &mut pending_combat,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_combat_event(),
            &outputs.events,
            &mut pending_event,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_death_proposal(),
            &outputs.deaths,
            &mut pending_death,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pve_event(),
            &outputs.pve,
            &mut pending_pve,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_door_outcome(),
            &outputs.doors,
            &mut pending_door,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_door_event(),
            &outputs.door_events,
            &mut pending_door_event,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_npc_proposal(),
            &outputs.npc_proposals,
            &mut pending_npc_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_npc_notification(),
            &outputs.npc_notifications,
            &mut pending_npc_notifications,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_cast_outcome(),
            &outputs.cast_outcomes,
            &mut pending_cast_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_magic_event(),
            &outputs.magic_events,
            &mut pending_magic_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_inventory_proposal(),
            &outputs.inventory_proposals,
            &mut pending_inventory_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_physical_launch(),
            &outputs.physical_launches,
            &mut pending_physical_launches,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_physical_combat_event(),
            &outputs.physical_events,
            &mut pending_physical_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_server_cast_outcome(),
            &outputs.server_cast_outcomes,
            &mut pending_server_cast_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_outcome(),
            &outputs.skill_outcomes,
            &mut pending_skill_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_device_outcome(),
            &outputs.skill_device_outcomes,
            &mut pending_skill_device_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_device_proposal(),
            &outputs.skill_device_proposals,
            &mut pending_skill_device_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_attribute_transfer_outcome(),
            &outputs.attribute_transfer_outcomes,
            &mut pending_attribute_transfer_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_attribute_transfer_proposal(),
            &outputs.attribute_transfer_proposals,
            &mut pending_attribute_transfer_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_crafting_proposal(),
            &outputs.crafting_proposals,
            &mut pending_crafting_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_crafting_outcome(),
            &outputs.crafting_outcomes,
            &mut pending_crafting_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_ui_outcome(),
            &outputs.ui_outcomes,
            &mut pending_ui_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pet_event(),
            &outputs.pet_events,
            &mut pending_pet_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pet_outcome(),
            &outputs.pet_outcomes,
            &mut pending_pet_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.pop_vendor_outcome(),
            &outputs.vendor_outcomes,
            &mut pending_vendor_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_portal_proposal(),
            &outputs.portal_proposals,
            &mut pending_portal_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_portal_event(),
            &outputs.portal_events,
            &mut pending_portal_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_request(),
            &outputs.generator_requests,
            &mut pending_generator_requests,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_event(),
            &outputs.generator_events,
            &mut pending_generator_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_outcome(),
            &outputs.generator_outcomes,
            &mut pending_generator_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generated_retirement(),
            &outputs.generator_retirements,
            &mut pending_generator_retirements,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_housing_proposal(),
            &outputs.housing_proposals,
            &mut pending_housing_proposals,
            config.command_capacity,
        );

        for _ in 0..config.command_capacity {
            let command = match pending_command
                .take()
                .or_else(|| inbox.receiver.try_recv().ok())
            {
                Some(command) => command,
                None => break,
            };
            if let Err(command) = kernel.try_enqueue(command) {
                pending_command = Some(command);
                break;
            }
        }
        match kernel.step() {
            Ok(rejected) => report.rejected_commands += rejected.len() as u64,
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
        outputs.social.flush(&mut kernel, config.command_capacity);
        outputs
            .player_deaths
            .flush(&mut kernel, config.command_capacity);
        outputs
            .interactions
            .flush(&mut kernel, config.command_capacity);
        outputs.regions.flush(&mut kernel, config.command_capacity);
        outputs.player_admissions.flush(&mut kernel, 1);
        outputs.region_admissions.flush(&mut kernel, 1);
        outputs.player_snapshots.flush(&mut kernel, 1);
        outputs
            .pve_services
            .flush(&mut kernel, config.command_capacity);
        outputs
            .npc_services
            .flush(&mut kernel, config.command_capacity);
        outputs
            .inventory_output
            .flush(&mut kernel, config.command_capacity);
        outputs.visibility.flush(&mut kernel, 1);
        outputs.object_view.flush(&mut kernel, 1);
        outputs.locomotion.flush(&mut kernel, 1);
        outputs.physical_resource.flush(&mut kernel, 1);
        outputs.magic_resource.flush(&mut kernel, 1);
        flush_outputs(
            || kernel.take_progression_outcome(),
            &outputs.progression,
            &mut pending_outcome,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_combat_outcome(),
            &outputs.combat,
            &mut pending_combat,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_combat_event(),
            &outputs.events,
            &mut pending_event,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_death_proposal(),
            &outputs.deaths,
            &mut pending_death,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pve_event(),
            &outputs.pve,
            &mut pending_pve,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_door_outcome(),
            &outputs.doors,
            &mut pending_door,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_door_event(),
            &outputs.door_events,
            &mut pending_door_event,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_npc_proposal(),
            &outputs.npc_proposals,
            &mut pending_npc_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_npc_notification(),
            &outputs.npc_notifications,
            &mut pending_npc_notifications,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_cast_outcome(),
            &outputs.cast_outcomes,
            &mut pending_cast_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_magic_event(),
            &outputs.magic_events,
            &mut pending_magic_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_inventory_proposal(),
            &outputs.inventory_proposals,
            &mut pending_inventory_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_physical_launch(),
            &outputs.physical_launches,
            &mut pending_physical_launches,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_physical_combat_event(),
            &outputs.physical_events,
            &mut pending_physical_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_server_cast_outcome(),
            &outputs.server_cast_outcomes,
            &mut pending_server_cast_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_outcome(),
            &outputs.skill_outcomes,
            &mut pending_skill_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_device_outcome(),
            &outputs.skill_device_outcomes,
            &mut pending_skill_device_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_skill_device_proposal(),
            &outputs.skill_device_proposals,
            &mut pending_skill_device_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_attribute_transfer_outcome(),
            &outputs.attribute_transfer_outcomes,
            &mut pending_attribute_transfer_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_attribute_transfer_proposal(),
            &outputs.attribute_transfer_proposals,
            &mut pending_attribute_transfer_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_crafting_proposal(),
            &outputs.crafting_proposals,
            &mut pending_crafting_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_crafting_outcome(),
            &outputs.crafting_outcomes,
            &mut pending_crafting_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_ui_outcome(),
            &outputs.ui_outcomes,
            &mut pending_ui_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pet_event(),
            &outputs.pet_events,
            &mut pending_pet_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_pet_outcome(),
            &outputs.pet_outcomes,
            &mut pending_pet_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.pop_vendor_outcome(),
            &outputs.vendor_outcomes,
            &mut pending_vendor_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_portal_proposal(),
            &outputs.portal_proposals,
            &mut pending_portal_proposals,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_portal_event(),
            &outputs.portal_events,
            &mut pending_portal_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_request(),
            &outputs.generator_requests,
            &mut pending_generator_requests,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_event(),
            &outputs.generator_events,
            &mut pending_generator_events,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generator_outcome(),
            &outputs.generator_outcomes,
            &mut pending_generator_outcomes,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_generated_retirement(),
            &outputs.generator_retirements,
            &mut pending_generator_retirements,
            config.command_capacity,
        );
        flush_outputs(
            || kernel.take_housing_proposal(),
            &outputs.housing_proposals,
            &mut pending_housing_proposals,
            config.command_capacity,
        );

        report.ticks += 1;
        let elapsed = start.elapsed();
        report.max_tick = report.max_tick.max(elapsed);
        let bucket = (elapsed.as_nanos().div_ceil(100_000).saturating_sub(1)).min(255) as usize;
        histogram[bucket] += 1;
        deadline += STEP;
        if config.real_time {
            let now = Instant::now();
            if now > deadline {
                let missed = (now.duration_since(deadline).as_nanos() / STEP.as_nanos() + 1) as u64;
                report.missed_deadlines += missed;
                // No busy catch-up spiral and no enlarged physics delta.
                deadline = now + STEP;
            }
        }
    }
    let mut unprocessed_commands = Vec::new();
    if let Some(command) = pending_command {
        unprocessed_commands.push(command);
    }
    unprocessed_commands.extend(inbox.close_and_recover());
    report.discarded_commands = unprocessed_commands.len() as u64;
    let undelivered_outcomes = pending_outcome.into_iter().collect();
    if report.ticks != 0 {
        let target = report.ticks.saturating_mul(99).div_ceil(100);
        let mut cumulative = 0;
        for (bucket, count) in histogram.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                report.p99_upper_bound =
                    (bucket < 255).then(|| Duration::from_micros((bucket as u64 + 1) * 100));
                break;
            }
        }
    }
    outputs.social.flush(&mut kernel, config.command_capacity);
    outputs
        .player_deaths
        .flush(&mut kernel, config.command_capacity);
    outputs
        .interactions
        .flush(&mut kernel, config.command_capacity);
    outputs.regions.flush(&mut kernel, config.command_capacity);
    outputs.player_admissions.flush(&mut kernel, 1);
    outputs.region_admissions.flush(&mut kernel, 1);
    outputs.player_snapshots.flush(&mut kernel, 1);
    outputs
        .pve_services
        .flush(&mut kernel, config.command_capacity);
    outputs
        .npc_services
        .flush(&mut kernel, config.command_capacity);
    outputs
        .inventory_output
        .flush(&mut kernel, config.command_capacity);
    outputs.visibility.flush(&mut kernel, 1);
    outputs.object_view.flush(&mut kernel, 1);
    outputs.locomotion.flush(&mut kernel, 1);
    outputs.physical_resource.flush(&mut kernel, 1);
    outputs.magic_resource.flush(&mut kernel, 1);
    SimulationExit {
        social: RecoveredSocial::default(),
        player_deaths: RecoveredPlayerDeaths::default(),
        interactions: RecoveredInteractions::default(),
        regions: outputs.regions.recover_pending(),
        pve_services: RecoveredPveServices::default(),
        npc_services: RecoveredNpcServices::default(),
        inventory_output: RecoveredInventory::default(),
        visibility: RecoveredVisibility::default(),
        object_view: RecoveredObjectView::default(),
        locomotion: RecoveredLocomotion::default(),
        physical_resource: RecoveredPhysicalResource::default(),
        magic_resource: RecoveredMagicResource::default(),
        player_snapshots: RecoveredPlayerSnapshots::default(),
        player_admissions: RecoveredPlayerAdmissions::default(),
        region_admissions: RecoveredRegionAdmissions::default(),
        report,
        kernel,
        unprocessed_commands,
        undelivered_outcomes,
        undelivered_combat_outcomes: pending_combat.into_iter().collect(),
        undelivered_combat_events: pending_event.into_iter().collect(),
        undelivered_death_proposals: pending_death.into_iter().collect(),
        undelivered_pve_events: pending_pve.into_iter().collect(),
        undelivered_door_outcomes: pending_door.into_iter().collect(),
        undelivered_door_events: pending_door_event.into_iter().collect(),
        undelivered_npc_proposals: pending_npc_proposals.into_iter().collect(),
        undelivered_npc_notifications: pending_npc_notifications.into_iter().collect(),
        undelivered_cast_outcomes: pending_cast_outcomes.into_iter().collect(),
        undelivered_magic_events: pending_magic_events.into_iter().collect(),
        undelivered_inventory_proposals: pending_inventory_proposals.into_iter().collect(),
        undelivered_physical_launches: pending_physical_launches.into_iter().collect(),
        undelivered_physical_events: pending_physical_events.into_iter().collect(),
        undelivered_server_cast_outcomes: pending_server_cast_outcomes.into_iter().collect(),
        undelivered_skill_outcomes: pending_skill_outcomes.into_iter().collect(),
        undelivered_skill_device_outcomes: pending_skill_device_outcomes.into_iter().collect(),
        undelivered_skill_device_proposals: pending_skill_device_proposals.into_iter().collect(),
        undelivered_attribute_transfer_outcomes: pending_attribute_transfer_outcomes
            .into_iter()
            .collect(),
        undelivered_attribute_transfer_proposals: pending_attribute_transfer_proposals
            .into_iter()
            .collect(),
        undelivered_crafting_proposals: pending_crafting_proposals.into_iter().collect(),
        undelivered_crafting_outcomes: pending_crafting_outcomes.into_iter().collect(),
        undelivered_ui_outcomes: pending_ui_outcomes.into_iter().collect(),
        undelivered_pet_events: pending_pet_events.into_iter().collect(),
        undelivered_pet_outcomes: pending_pet_outcomes.into_iter().collect(),
        undelivered_vendor_outcomes: pending_vendor_outcomes.into_iter().collect(),
        undelivered_portal_proposals: pending_portal_proposals.into_iter().collect(),
        undelivered_portal_events: pending_portal_events.into_iter().collect(),
        undelivered_generator_requests: pending_generator_requests.into_iter().collect(),
        undelivered_generator_events: pending_generator_events.into_iter().collect(),
        undelivered_generator_outcomes: pending_generator_outcomes.into_iter().collect(),
        undelivered_generator_retirements: pending_generator_retirements.into_iter().collect(),
        undelivered_housing_proposals: pending_housing_proposals.into_iter().collect(),
        failure,
    }
}

fn flush_outputs<T>(
    mut next: impl FnMut() -> Option<T>,
    output: &mpsc::SyncSender<T>,
    pending: &mut Option<T>,
    limit: usize,
) {
    for _ in 0..limit {
        let Some(outcome) = pending.take().or_else(&mut next) else {
            break;
        };
        if let Err(error) = output.try_send(outcome) {
            *pending = Some(match error {
                mpsc::TrySendError::Full(value) | mpsc::TrySendError::Disconnected(value) => value,
            });
            break;
        }
    }
}
