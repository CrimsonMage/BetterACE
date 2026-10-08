//! Single adapter-owner lifecycle pump. Fixed retained I/O futures keep SQL and
//! cold assets off the event pump. Dropping this owner is never a shutdown path.
mod allegiance;
mod combat;
mod crafting;
mod deaths;
mod pets;
mod pve_deaths;
pub use deaths::{CorpseSpillPresentation, DeathDelivery, DeathDeliveryWork};
mod attribute_transfer;
mod creation;
mod entry;
mod ingress;
mod inventory;
mod magic;
mod movement;
mod npc;
mod skill_devices;
mod vendors;
mod visibility;
pub use magic::MagicObserverWork;
mod lifecycle;
mod logout;
mod shutdown;
mod startup;
pub use shutdown::GameRuntimeShutdown;
mod world;
#[cfg(test)]
pub(crate) use logout::finish as finish_detached_logout;
pub use startup::{GameRuntimeStartup, GameRuntimeStartupAssets};
mod policy;
mod portals;
pub use portals::{PortalDelivery, PortalDeliveryWork};
mod observer_output;
mod progression;
mod publication;
mod pump;
mod recalls;
mod rewards;
pub use pump::{GameRuntimeControl, GameRuntimePause, GameRuntimeStatus};
pub use rewards::RewardObserverWork;
mod shard;
mod social;
mod staff;
use crate::{
    authentication_pool::AuthenticationPool,
    game_bootstrap::{GameBootstrap, GameClock},
    game_login::LoadedPlayer,
    generator_service::GeneratorService,
    network::{NetworkCommand, NetworkEvent, NetworkThread},
    online_player_saves::{OnlinePlayerSaveService, RoutineResolutionOutcome},
    player_preparation_worker::PlayerPreparationWorker,
    player_service::{PlayerIoAction, PlayerIoCompletion, PlayerService},
    region_service::RegionService,
    saves::SaveWorker,
    simulation::SimulationWorker,
};
use bace_session::SessionKey;
pub use policy::PlayerLoginPolicy;
pub use shard::ShardHostRecord;
pub use staff::StaffBroadcastRecord;
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};

/// All live owners are returned intact if constructor validation rejects them.
pub struct GameRuntimeOwners {
    pub bootstrap: GameBootstrap,
    pub chat_api: Option<crate::chat_service::ChatApiService>,
    pub simulation: SimulationWorker,
    pub network: NetworkThread,
    pub saves: SaveWorker,
    pub authentication: AuthenticationPool,
    pub players: PlayerService,
    pub online_saves: OnlinePlayerSaveService,
    pub regions: RegionService,
    pub generators: GeneratorService,
    pub preparation: PlayerPreparationWorker,
}
#[derive(Clone)]
pub struct GameRuntimeAssets {
    /// Accepted startup world policy; closing later is managed by ShardControl.
    pub world_open: bool,
    /// Explicit offset for pinned shutdown labels, supplied by the cold host.
    pub local_offset_seconds: i32,
    pub staff_definitions: Arc<[bace_gameplay_api::staff::StaffSpellDefinition]>,
    pub spell_rows: Arc<BTreeMap<u32, bace_content::SpellRowV1>>,
    pub component_templates: Arc<[u32]>,
    pub projectile_shapes: Arc<BTreeMap<u32, Arc<bace_physics::CollisionShape>>>,
    pub projectile_visibility:
        Arc<BTreeMap<u32, Arc<crate::visibility_assets::PreparedProjectileVisibilityTemplate>>>,
    /// Accepted creature policy reused for authored pet admission.
    pub creatures: crate::generator_preparation::CreatureAdmissionPolicy,
    pub policy: PlayerLoginPolicy,
}
#[derive(Clone, Copy, Debug)]
pub struct GameRuntimeLimits {
    pub sessions: usize,
    pub loading: usize,
    pub messages: usize,
    pub message_bytes: usize,
    pub work_per_poll: usize,
}
impl GameRuntimeLimits {
    fn validate(self) -> Result<(), String> {
        if !(1..=4096).contains(&self.sessions)
            || !(1..=4).contains(&self.loading)
            || !(1..=4096).contains(&self.messages)
            || !(4096..=64 * 1024 * 1024).contains(&self.message_bytes)
            || !(1..=256).contains(&self.work_per_poll)
        {
            Err("game lifecycle limits".into())
        } else {
            Ok(())
        }
    }
}
type Job<T> = Pin<Box<dyn Future<Output = T>>>;
struct Session {
    account: bace_auth::AccountRecord,
    connected: bool,
    closing: bool,
    terminated: bool,
    disconnected: bool,
    loading: Option<lifecycle::Loading>,
    failure: Option<String>,
}
/// Remains on one adapter executor. Rc only shares channel handles; all physical
/// state remains exclusively in SimulationWorker's dedicated owner thread.
pub struct GameRuntime {
    bootstrap: GameBootstrap,
    publication: publication::PublicationRuntime,
    social: social::SocialRuntime,
    allegiance: allegiance::AllegianceRuntime,
    progression: progression::ProgressionRuntime,
    inventory: inventory::InventoryRuntime,
    vendors: vendors::VendorRuntime,
    crafting: crafting::CraftingRuntime,
    combat: combat::CombatRuntime,
    deaths: deaths::DeathRuntime,
    pve_deaths: pve_deaths::PveDeathRuntime,
    pets: pets::PetRuntime,
    recalls: recalls::RecallRuntime,
    skill_devices: skill_devices::SkillDeviceRuntime,
    attribute_transfers: attribute_transfer::AttributeTransferRuntime,
    creation: creation::CreationRuntime,
    npc: npc::NpcRuntime,
    magic: magic::MagicRuntime,
    visibility: visibility::VisibilityRuntime,
    movement: movement::MovementRuntime,
    portals: portals::PortalRuntime,
    rewards: rewards::RewardRuntime,
    observer_output: observer_output::ObserverOutputRuntime,
    staff: staff::StaffRuntime,
    shard: shard::ShardRuntime,
    simulation: Rc<SimulationWorker>,
    network: NetworkThread,
    saves: SaveWorker,
    authentication: AuthenticationPool,
    players: PlayerService,
    online_saves: OnlinePlayerSaveService,
    world: Option<world::Owners>,
    world_job: Option<Job<(world::Owners, Result<(), String>)>>,
    preparation: PlayerPreparationWorker,
    unexpected_cold: Option<crate::player_preparation_worker::PlayerPreparationCompletion>,
    assets: GameRuntimeAssets,
    limits: GameRuntimeLimits,
    clock: GameClock,
    last_elapsed: Duration,
    lifecycle_cursor: Option<SessionKey>,
    sessions: BTreeMap<SessionKey, Session>,
    login_job: Option<Job<PlayerIoCompletion>>,
    login_key: Option<SessionKey>,
    metadata_job: Option<Job<lifecycle::MetadataCompletion>>,
    receipt_job: Option<Job<lifecycle::ReceiptCompletion>>,
    resolution: Option<Job<RoutineResolutionOutcome>>,
    logout_job: Option<Job<logout::Completion>>,
    logout: BTreeMap<SessionKey, logout::Work>,
    unexpected_detach: Option<Box<bace_simulation::PlayerDetachOutcome>>,
    input: VecDeque<NetworkEvent>,
    input_bytes: usize,
    network_output: VecDeque<NetworkCommand>,
    reliable_admissions: VecDeque<(SessionKey, u64, bool)>,
    login_queue: VecDeque<(SessionKey, PlayerIoAction)>,
    pending_auth: Option<crate::authentication_pool::AuthenticationCompletion>,
    auth_inflight: std::collections::BTreeSet<SessionKey>,
    auth_cancelled: std::collections::BTreeSet<SessionKey>,
    request_regions: BTreeMap<u64, (SessionKey, u16)>,
    regions_quiesce: Option<u64>,
    regions_quiesced: bool,
    social_controls: BTreeMap<u64, SessionKey>,
    unexpected_social_control: Option<bace_simulation::SocialControlOutcome>,
    snapshot: Option<(u64, SessionKey)>,
    unexpected_snapshot: Option<bace_simulation::PlayerSnapshotOutcome>,
    unexpected_admission: Option<Box<bace_simulation::PlayerAdmissionOutcome>>,
    unexpected_entered: Option<bace_simulation::PlayerEnteredOutcome>,
    next: u64,
    draining: bool,
    failure: Option<String>,
}
impl GameRuntime {
    pub fn new(
        owners: GameRuntimeOwners,
        assets: GameRuntimeAssets,
        limits: GameRuntimeLimits,
        clock: GameClock,
    ) -> Result<Box<Self>, (String, Box<GameRuntimeOwners>)> {
        let checked = limits.validate().and_then(|()| {
            if !(-86_400..=86_400).contains(&assets.local_offset_seconds)
                || assets.spell_rows.len() > 65536
                || assets.staff_definitions.len() > 8192
                || assets.component_templates.len() > 4096
                || assets.projectile_shapes.len() > 1024
                || assets.projectile_visibility.len() > 1024
            {
                Err("game lifecycle asset closure limits".into())
            } else {
                Ok(())
            }
        });
        if let Err(error) = checked {
            return Err((error, Box::new(owners)));
        }
        if owners.bootstrap.config.chat_api.enabled != owners.chat_api.is_some() {
            return Err((
                "configured chat API service missing or unexpected".into(),
                Box::new(owners),
            ));
        }
        let allegiance = match allegiance::AllegianceRuntime::new(&owners.bootstrap) {
            Ok(value) => value,
            Err(error) => return Err((error, Box::new(owners))),
        };
        let visibility = match visibility::VisibilityRuntime::new(limits) {
            Ok(value) => value,
            Err(error) => return Err((error, Box::new(owners))),
        };
        let creation = match creation::CreationRuntime::new(
            owners.bootstrap.assets.clone(),
            limits.loading,
            owners.players.maximum_slots(),
        ) {
            Ok(value) => value,
            Err(error) => return Err((error, Box::new(owners))),
        };
        let lookup = match crate::social_lookup::SocialLookupService::start_with_pressure(
            owners.bootstrap.store.clone(),
            limits.sessions,
            owners.bootstrap.save_pressure.clone(),
        ) {
            Ok(lookup) => lookup,
            Err(error) => return Err((error, Box::new(owners))),
        };
        let GameRuntimeOwners {
            bootstrap,
            chat_api,
            simulation,
            network,
            saves,
            authentication,
            mut players,
            online_saves,
            regions,
            generators,
            preparation,
        } = owners;
        players.enable_turbine_chat();
        Ok(Box::new(Self {
            bootstrap,
            publication: publication::PublicationRuntime::new(),
            shard: shard::ShardRuntime::new(
                assets.world_open,
                chat_api.as_ref().map(|api| api.publisher()),
            ),
            social: social::SocialRuntime::new(chat_api, lookup),
            allegiance,
            progression: progression::ProgressionRuntime::new(),
            inventory: inventory::InventoryRuntime::new(),
            vendors: vendors::VendorRuntime::new(),
            crafting: crafting::CraftingRuntime::new(),
            combat: combat::CombatRuntime::new(),
            deaths: deaths::DeathRuntime::new(),
            pve_deaths: pve_deaths::PveDeathRuntime::new(),
            pets: pets::PetRuntime::new(),
            recalls: recalls::RecallRuntime::new(),
            skill_devices: skill_devices::SkillDeviceRuntime::new(),
            attribute_transfers: attribute_transfer::AttributeTransferRuntime::new(),
            creation,
            npc: npc::NpcRuntime::new(),
            magic: magic::MagicRuntime::new(),
            visibility,
            movement: movement::MovementRuntime::new(),
            portals: portals::PortalRuntime::new(),
            rewards: rewards::RewardRuntime::new(),
            observer_output: observer_output::ObserverOutputRuntime::new(),
            staff: staff::StaffRuntime::new(assets.staff_definitions.clone()),
            simulation: Rc::new(simulation),
            network,
            saves,
            authentication,
            players,
            online_saves,
            world: Some(world::Owners {
                regions,
                generators,
                pending: None,
                events: VecDeque::new(),
            }),
            world_job: None,
            preparation,
            unexpected_cold: None,
            assets,
            limits,
            clock,
            last_elapsed: Duration::ZERO,
            lifecycle_cursor: None,
            sessions: BTreeMap::new(),
            login_job: None,
            login_key: None,
            metadata_job: None,
            receipt_job: None,
            resolution: None,
            logout_job: None,
            logout: BTreeMap::new(),
            unexpected_detach: None,
            input: VecDeque::new(),
            input_bytes: 0,
            network_output: VecDeque::new(),
            reliable_admissions: VecDeque::new(),
            login_queue: VecDeque::new(),
            pending_auth: None,
            auth_inflight: Default::default(),
            auth_cancelled: Default::default(),
            request_regions: BTreeMap::new(),
            regions_quiesce: None,
            regions_quiesced: false,
            social_controls: BTreeMap::new(),
            unexpected_social_control: None,
            snapshot: None,
            unexpected_snapshot: None,
            unexpected_admission: None,
            unexpected_entered: None,
            next: 0x4752_0000_0000_0000,
            draining: false,
            failure: None,
        }))
    }
    fn token(&mut self) -> Result<u64, String> {
        self.next = self
            .next
            .checked_add(1)
            .ok_or("game correlation exhausted")?;
        Ok(self.next)
    }
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    pub fn player_failure(&self, key: SessionKey) -> Option<&str> {
        self.sessions.get(&key)?.failure.as_deref()
    }
    pub fn players(&self) -> &PlayerService {
        &self.players
    }
    pub fn saves(&self) -> &OnlinePlayerSaveService {
        &self.online_saves
    }
    /// Progress admitted work even while quiescing. No SQL await or thread join
    /// occurs here. The caller polls at a bounded cadence on its adapter executor.
    pub fn poll(&mut self, elapsed: Duration) -> Result<(), String> {
        if elapsed < self.last_elapsed {
            return Err("adapter clock moved backwards".into());
        }
        self.last_elapsed = elapsed;
        let unix = self
            .clock
            .unix_millis
            .checked_add(u64::try_from(elapsed.as_millis()).map_err(|_| "game clock overflow")?)
            .ok_or("game clock overflow")?;
        let result = self.poll_inner(elapsed, unix);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn poll_inner(&mut self, elapsed: Duration, unix: u64) -> Result<(), String> {
        // Persistence always gets service before reads/new logins (SAVE-04).
        // A degraded lane retains its exact owner but must not prevent other
        // players' saves, disconnects, or ready output from making progress.
        let mut first_error = None;
        for result in [
            self.poll_durability(elapsed, unix),
            self.poll_magic(unix),
            self.poll_combat(unix),
            self.poll_allegiance(),
            self.poll_lifecycle(elapsed, unix),
            self.poll_creation(unix),
            self.poll_logout(elapsed, unix),
            self.poll_world(elapsed, unix),
            self.poll_generator_publications(),
            self.poll_npcs(elapsed),
            self.poll_portal_publications(),
            self.poll_observer_outputs(),
            self.poll_visibility(),
            self.poll_movement(),
            self.poll_publication(elapsed),
            self.poll_social_ingress(),
            self.poll_progression(),
            self.poll_inventory(),
            self.poll_vendors(),
            self.poll_crafting(),
            self.poll_recalls(),
            self.poll_skill_devices(),
            self.poll_attribute_transfers(),
            self.poll_portals(),
            self.poll_deaths(elapsed, unix),
            self.poll_pve_deaths(elapsed, unix),
            self.poll_pets(),
            self.poll_corpse_expiry(unix),
            self.poll_shard(),
            self.poll_ingress(),
        ] {
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        self.players
            .flush_admissions(&self.simulation.input(), self.limits.work_per_poll);
        self.players
            .flush_network(&self.network, self.limits.work_per_poll);
        self.flush_output();
        self.players
            .flush_entered(&self.simulation.input(), self.limits.work_per_poll);
        if let Err(error) = self.poll_social_outputs() {
            first_error.get_or_insert(error);
        }
        if let Err(error) = self.poll_reward_outputs() {
            first_error.get_or_insert(error);
        }
        if let Err(error) = self.poll_staff() {
            first_error.get_or_insert(error);
        }
        if self.simulation.is_finished() {
            first_error.get_or_insert_with(|| {
                "simulation worker exited; kernel and pending outputs require recovery".into()
            });
        }
        first_error.map_or(Ok(()), Err)
    }
    pub fn quiesce(&mut self, now: Duration) -> Result<(), String> {
        self.draining = true;
        if let Some(world) = &mut self.world {
            world.generators.quiesce();
        }
        for session in self.sessions.values() {
            if let Some(loading) = &session.loading
                && loading.online.is_some()
            {
                self.online_saves
                    .drain_player(loading.loaded.binding.actor.0, now)?;
            }
        }
        Ok(())
    }
    /// This vertical slice intentionally cannot claim a drained game while any
    /// character/world owner or unhandled gameplay input remains retained.
    pub fn take_reliable_batch_admission(&mut self) -> Option<(SessionKey, u64, bool)> {
        self.reliable_admissions.pop_front()
    }
    pub fn requires_drain(&self) -> bool {
        if !self.reliable_admissions.is_empty() {
            return true;
        }
        (self.draining && !self.regions_quiesced)
            || !self.auth_inflight.is_empty()
            || self.pending_auth.is_some()
            || self.social.has_pending()
            || self.allegiance.has_pending()
            || self.progression.has_pending()
            || self.publication.has_pending()
            || self.inventory.has_pending()
            || self.vendors.has_pending()
            || self.crafting.has_pending()
            || self.deaths.has_pending()
            || self.pve_deaths.has_pending()
            || self.pets.has_pending()
            || self.recalls.has_pending()
            || self.skill_devices.has_pending()
            || self.attribute_transfers.has_pending()
            || self.creation.pending()
            || self.npc.has_pending()
            || self.magic.has_pending()
            || self.combat.has_pending()
            || self.visibility.has_pending()
            || self.movement.has_pending()
            || self.portals.has_pending()
            || self.rewards.has_pending()
            || self.observer_output.has_pending()
            || self.staff.has_pending()
            || (self.shard.has_pending() && !self.shard.terminal_stop_pending())
            || self.players.requires_drain()
            || self.online_saves.requires_drain()
            || !self.sessions.is_empty()
            || !self.input.is_empty()
            || !self.network_output.is_empty()
            || self.login_job.is_some()
            || self.world_job.is_some()
            || self.metadata_job.is_some()
            || self.receipt_job.is_some()
            || self.resolution.is_some()
            || self.logout_job.is_some()
            || !self.logout.is_empty()
            || self.unexpected_detach.is_some()
            || self.preparation.pending() != 0
            || self.unexpected_cold.is_some()
            || self.snapshot.is_some()
            || self.unexpected_snapshot.is_some()
            || self.unexpected_admission.is_some()
            || self.unexpected_entered.is_some()
            || self.world.as_ref().is_some_and(|w| {
                w.regions.has_pending() || w.generators.has_pending() || !w.events.is_empty()
            })
    }
}
fn ready<T>(job: &mut Option<Job<T>>) -> Option<T> {
    let future = job.as_mut()?;
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Pending => None,
        Poll::Ready(value) => {
            *job = None;
            Some(value)
        }
    }
}

#[cfg(test)]
mod tests;
