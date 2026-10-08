//! One ammunition save and one bounded ID refill; uncertain operations and cold
//! failures retain their original evidence across disconnect and shutdown.
use super::*;
use crate::{
    physical_resource_service::{PhysicalResourceCompletion, PhysicalResourceService},
    visibility_assets::PreparedVisibilityObject,
};
use bace_gameplay_api::weapon_combat::PhysicalLaunchProposal;
use bace_simulation::{
    PhysicalResourceAction as A, PhysicalResourceDecision as D, PhysicalResourceOutcome,
};
pub(super) struct Resources {
    service: PhysicalResourceService,
    launch: Option<PhysicalLaunchProposal>,
    cold: Option<Job<Result<PreparedVisibilityObject, String>>>,
    prepared: Option<PreparedVisibilityObject>,
    pub(super) completion: Option<PhysicalResourceCompletion>,
    ids: Option<Job<Result<Vec<EntityId>, String>>>,
    allocated: Option<Vec<EntityId>>,
    supply: Option<u64>,
    remaining: usize,
    next_probe: Duration,
    unexpected: Option<Arc<PhysicalResourceOutcome>>,
}
impl Resources {
    pub(super) fn new() -> Self {
        Self {
            service: PhysicalResourceService::new(),
            launch: None,
            cold: None,
            prepared: None,
            completion: None,
            ids: None,
            allocated: None,
            supply: None,
            remaining: 0,
            next_probe: Duration::ZERO,
            unexpected: None,
        }
    }
    pub(super) fn has_pending(&self) -> bool {
        self.service.has_pending()
            || self.launch.is_some()
            || self.cold.is_some()
            || self.prepared.is_some()
            || self.completion.is_some()
            || self.ids.is_some()
            || self.allocated.is_some()
            || self.supply.is_some()
            || self.unexpected.is_some()
    }
    pub(super) fn owns(&self, actor: EntityId) -> bool {
        self.service.actor() == Some(actor)
            || self.launch.as_ref().is_some_and(|p| p.actor == actor.0)
            || self
                .completion
                .as_ref()
                .is_some_and(|p| p.binding.actor == actor)
    }
}
impl GameRuntime {
    pub(super) fn poll_physical_resources(&mut self, unix: u64) -> Result<(), String> {
        if self.combat.resources.unexpected.is_some() {
            return Err("physical resource correlation retained".into());
        }
        for _ in 0..self.limits.work_per_poll {
            let Ok(outcome) = self.simulation.physical_resource_outcomes().try_recv() else {
                break;
            };
            if self.combat.resources.supply == Some(outcome.correlation) {
                match &outcome.result {
                    Ok(D::Projectiles { remaining }) => {
                        self.combat.resources.remaining = *remaining;
                        self.combat.resources.allocated = None;
                        self.combat.resources.supply = None;
                        self.combat.resources.next_probe =
                            self.last_elapsed.saturating_add(Duration::from_secs(1));
                    }
                    Err(error) => {
                        self.combat.resources.supply = None;
                        return Err(format!("projectile identity supply retained: {error:?}"));
                    }
                    _ => {
                        self.combat.resources.unexpected = Some(outcome);
                        return Err("physical identity receipt mismatch".into());
                    }
                }
            } else if let Err(outcome) =
                self.combat
                    .resources
                    .service
                    .accept(outcome, &self.online_saves, unix)
            {
                self.combat.resources.unexpected = Some(outcome);
                return Err("physical resource owner mismatch".into());
            }
        }
        self.refill_physical_ids()?;
        self.prepare_physical_ammo_appearance()?;
        self.combat.resources.service.poll(
            &self.simulation.input(),
            &mut self.online_saves,
            &self.saves.handle,
        )?;
        if self.combat.resources.completion.is_none() {
            self.combat.resources.completion = self.combat.resources.service.take_completion();
        }
        self.project_ammunition_completion()?;
        if self.combat.resources.service.has_pending() || self.combat.resources.completion.is_some()
        {
            return Ok(());
        }
        if self.combat.resources.launch.is_none() {
            self.combat.resources.launch = self.simulation.physical_launches().try_recv().ok();
        }
        let Some(launch) = self.combat.resources.launch.as_ref() else {
            return Ok(());
        };
        let binding = self
            .sessions
            .values()
            .find_map(|s| {
                s.loading
                    .as_ref()
                    .map(|l| l.loaded.binding)
                    .filter(|b| b.actor.0 == launch.actor)
            })
            .ok_or("ammunition launch lost admitted player binding")?;
        let launch = self
            .combat
            .resources
            .launch
            .take()
            .expect("retained launch");
        self.combat
            .resources
            .service
            .stage(binding, launch)
            .map_err(|launch| {
                self.combat.resources.launch = Some(*launch);
                "ammunition service capacity".to_string()
            })?;
        Ok(())
    }
    fn prepare_physical_ammo_appearance(&mut self) -> Result<(), String> {
        let Some(ticket) = self.combat.resources.service.appearance_ticket().cloned() else {
            return Ok(());
        };
        if self.combat.resources.prepared.is_none() && self.combat.resources.cold.is_none() {
            let source = ticket
                .appearance
                .as_ref()
                .ok_or("accepted ammunition source missing")?
                .as_ref()
                .clone();
            let manifest = self.bootstrap.assets.clone();
            let launch = ticket.launch.clone();
            self.combat.resources.cold = Some(Box::pin(async move {
                tokio::task::spawn_blocking(move || cold_blueprint(manifest, source, launch))
                    .await
                    .map_err(|e| e.to_string())?
            }));
        }
        if let Some(job) = self.combat.resources.cold.as_mut()
            && let Poll::Ready(result) = job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        {
            self.combat.resources.cold = None;
            self.combat.resources.prepared = Some(result?);
        }
        if let Some(source) = &self.combat.resources.prepared {
            if self.combat.projectiles.blueprints.len() >= 4096 {
                return Ok(());
            }
            source
                .register(&mut self.visibility.service, 0)
                .map_err(|e| format!("ammunition visibility retained: {e:?}"))?;
            self.combat
                .projectiles
                .blueprints
                .insert(EntityId(source.description.object_id), source.clone());
            self.combat
                .resources
                .service
                .acknowledge_appearance(ticket.launch.operation)?;
            self.combat.resources.prepared = None;
        }
        Ok(())
    }
    fn refill_physical_ids(&mut self) -> Result<(), String> {
        let r = &mut self.combat.resources;
        if let Some(job) = r.ids.as_mut()
            && let Poll::Ready(result) = job.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        {
            r.ids = None;
            r.allocated = Some(result?);
        }
        if r.supply.is_some() {
            return Ok(());
        }
        if !self.draining && r.remaining < 16 && r.ids.is_none() && r.allocated.is_none() {
            let store = self.bootstrap.store.clone();
            r.ids = Some(Box::pin(async move {
                let ids = store
                    .allocate_dynamic_ids(32)
                    .await
                    .map_err(|e| e.to_string())?;
                if ids.len() != 32 {
                    return Err("projectile allocator count".into());
                }
                Ok(ids.into_iter().map(EntityId).collect())
            }));
        }
        if r.allocated.is_some()
            || (!self.draining && r.ids.is_none() && self.last_elapsed >= r.next_probe)
        {
            let ids = r.allocated.clone().unwrap_or_default();
            let correlation = self.token()?;
            match self
                .simulation
                .input()
                .try_submit(bace_simulation::Command::PhysicalResource(Box::new(
                    bace_simulation::PhysicalResourceCommand {
                        correlation,
                        action: A::SupplyProjectiles(ids),
                    },
                ))) {
                Ok(()) => self.combat.resources.supply = Some(correlation),
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return Err("physical ID owner closed; allocation retained".into());
                }
            }
        }
        Ok(())
    }
}
pub(super) fn cold_blueprint(
    manifest: crate::region_activation::RegionAssetManifest,
    mut source: bace_content::WeenieV1,
    launch: PhysicalLaunchProposal,
) -> Result<PreparedVisibilityObject, String> {
    // GDLE combat/MissileAttackEventData.cpp::FireMissile and Ammunition.cpp.
    source
        .properties
        .instance_ids
        .retain(|p| !matches!(p.id, 2 | 3));
    source.properties.ints.retain(|p| !matches!(p.id, 10 | 52));
    if source.properties.ints.iter().any(|p| p.id == 12) {
        crate::game_inventory::set(&mut source.properties.ints, 12, 1);
    }
    let epoch = u64::from_le_bytes(
        launch.event_id[..8]
            .try_into()
            .expect("fixed event identity"),
    );
    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
    let mut rows =
        assets.prepare_visibility_sources(vec![crate::visibility_assets::VisibilitySource {
            entity: EntityId(launch.projectile),
            incarnation: epoch,
            revision: 1,
            source: &source,
            equipment: vec![],
            missile_combat: false,
        }])?;
    let mut row = rows.pop().ok_or("ammunition appearance missing")?;
    let description = Arc::make_mut(&mut row.description);
    description.physics.state = 0x20748;
    description.physics.options.movement = Some(bace_wire::PhysicsMovement::AnimationFrame(52));
    Ok(row)
}
