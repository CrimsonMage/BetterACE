//! Real PostgreSQL commit with an injected lost reply; live owner state remains
//! in the real simulation worker while the immutable operation is retried.
use super::*;
use crate::{
    player_service::tests::cluster::Cluster,
    saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker},
    simulation::{SimulationConfig, SimulationWorker},
};
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_persistence::{
    DurableItemPlace, OperationOutcome, PlacementChange, PlacementOperation, SaveAck,
};
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5, PlayerSaveV1, PlayerSaveV6,
};
use bace_types::EntityId;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::time::{Duration, Instant};
mod fixture;
use fixture::*;
#[derive(Clone)]
struct LostReply {
    store: bace_db_postgres::PgStore,
    gate: Arc<tokio::sync::Semaphore>,
    attempts: Arc<AtomicUsize>,
    operations: Arc<Mutex<Vec<PlacementOperation>>>,
}
impl SaveBackend for LostReply {
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("no routine writer in this operation fixture")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("ammunition uses placement")
    }
    async fn placement(
        &self,
        operation: &PlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.operations.lock().unwrap().push(operation.clone());
        self.gate.acquire().await.unwrap().forget();
        let result = self
            .store
            .placement_operation(operation)
            .await
            .map_err(|e| SaveFailure::Storage {
                message: e.to_string(),
                uncertain: false,
            })?;
        if self.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(SaveFailure::Storage {
                message: "injected reply loss after PostgreSQL commit".into(),
                uncertain: true,
            })
        } else {
            Ok(result)
        }
    }
}
fn source(id: u32, stack: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 1,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: format!("ammo_fixture_{id}"),
            weenie_type: 1,
            last_modified: None,
            properties: bace_content::SparseProperties {
                ints: vec![
                    bace_content::Property {
                        id: 5,
                        value: stack as i32,
                    },
                    bace_content::Property {
                        id: 12,
                        value: stack as i32,
                    },
                    bace_content::Property { id: 13, value: 1 },
                    bace_content::Property { id: 15, value: 1 },
                    bace_content::Property {
                        id: 19,
                        value: stack as i32,
                    },
                    bace_content::Property { id: 20, value: 100 },
                ],
                ..Default::default()
            },
        },
    }
}
async fn setup() -> (
    Cluster,
    bace_db_postgres::PgStore,
    OnlinePlayerSaveService,
    CharacterBinding,
) {
    let cluster = Cluster::start();
    let store = bace_db_postgres::PgStore::connect(&cluster.url(), 4)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let CreateAccountOutcome::Created(account) = store
        .create(NewAccount {
            name: AccountName::parse("ammunition-owner").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"fixture-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("account")
    };
    let binding = CharacterBinding {
        actor: EntityId(ACTOR),
        account: account.id,
        session: bace_gameplay_api::SessionId(9),
    };
    let player = PlayerSaveV1 {
        entity: source(ACTOR, 1),
        account_id: account.id.0,
        name: "Ammunition Owner".into(),
        metadata: Default::default(),
        quests: vec![],
    };
    let initial = [(source(LAUNCHER, 1), 0), (source(AMMO, 2), 1)];
    let offline = store.create_player(&player, 0, 2, &initial).await.unwrap();
    let loading = store.begin_login(offline).await.unwrap();
    let online = store.finish_login(loading.lease).await.unwrap();
    let mut snapshots = Vec::new();
    let mut changes = Vec::new();
    let mut items = Vec::new();
    for ((mut entity, slot), equipped) in initial.into_iter().zip([0x400000, 0x800000]) {
        entity.mutation_revision = 2;
        let item = ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity,
            placement: ItemPlacementV2::Contained {
                container: ACTOR,
                slot,
                pack_slot: false,
                equipped,
            },
        })
        .unwrap();
        snapshots.push(SaveSnapshot {
            object_id: item.entity.object_id,
            mutation_revision: 2,
            expected_version: 1,
            bytes: item.encode().unwrap(),
        });
        changes.push(PlacementChange {
            item: item.entity.object_id,
            expected: Some(DurableItemPlace::Contained {
                container: ACTOR,
                slot,
                pack_slot: false,
                equipped: 0,
            }),
            destination: DurableItemPlace::Contained {
                container: ACTOR,
                slot,
                pack_slot: false,
                equipped,
            },
        });
        items.push((item, 2));
    }
    store
        .placement_operation(&PlacementOperation {
            operation_id: "fixture-equip".into(),
            snapshots,
            changes,
            participants: vec![ACTOR, LAUNCHER, AMMO],
            leases: vec![online],
            storage_views: vec![],
        })
        .await
        .unwrap();
    let mut saves = OnlinePlayerSaveService::new(2, 1 << 20, Instant::now()).unwrap();
    saves
        .register(
            binding,
            online,
            PlayerSaveV6::migrate_v1(player).unwrap(),
            1,
            Duration::ZERO,
        )
        .unwrap();
    saves.register_inventory(ACTOR, items).unwrap();
    (cluster, store, saves, binding)
}
async fn tick(
    service: &mut PhysicalResourceService,
    worker: &SimulationWorker,
    online: &mut OnlinePlayerSaveService,
    saves: &SaveHandle,
) -> Result<(), String> {
    while let Ok(outcome) = worker.physical_resource_outcomes().try_recv() {
        service
            .accept(outcome, online, 1_800_000_000_000)
            .unwrap_or_else(|_| panic!("exact correlated resource outcome"));
    }
    // This durability-only fixture has no rendering adapter. Its synthetic
    // source appearance is explicitly accepted once the exact ticket arrives.
    if let Some(ticket) = service.appearance_ticket() {
        let operation = ticket.launch.operation;
        service.acknowledge_appearance(operation).unwrap();
    }
    let result = service.poll(&worker.input(), online, saves);
    tokio::time::sleep(Duration::from_millis(2)).await;
    result
}
#[tokio::test]
async fn postgres_ammo_commit_lost_reply_and_owner_disconnect_retry_exactly_once() {
    let (_cluster, store, mut online, binding) = setup().await;
    let (kernel, launch) = kernel(binding);
    let mut worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 1,
            ..Default::default()
        },
        1,
    )
    .unwrap();
    let backend = LostReply {
        store: store.clone(),
        gate: Arc::new(tokio::sync::Semaphore::new(0)),
        attempts: Arc::new(AtomicUsize::new(0)),
        operations: Arc::new(Mutex::new(vec![])),
    };
    let writer = spawn_save_worker(
        backend.clone(),
        SaveWorkerConfig {
            valuable_capacity: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut service = PhysicalResourceService::new();
    service.stage(binding, launch.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while backend.operations.lock().unwrap().is_empty() {
        assert!(
            service.completion.is_none(),
            "unexpected terminal resource rejection: {:?}",
            service
                .completion
                .as_ref()
                .and_then(|c| c.failure.as_deref())
        );
        assert!(
            Instant::now() < deadline,
            "reservation/save failed: {:?}, phase {:?}, owner finished {}",
            service.failure(),
            service.pending.as_ref().map(|p| (
                std::mem::discriminant(&p.phase),
                p.appearance_prepared,
                p.ticket.is_some()
            )),
            worker.is_finished()
        );
        tick(&mut service, &worker, &mut online, &writer.handle)
            .await
            .unwrap();
    }
    assert!(service.has_pending());
    assert!(online.critical_ready(&[ACTOR]).is_err() || !online.critical_ready(&[ACTOR]).unwrap());
    // Before SQL admission no projectile is emitted and the durable count is2.
    assert!(worker.physical_events().try_iter().all(|e| !matches!(
        e,
        bace_simulation::PhysicalCombatEvent::ProjectileCreated { .. }
    )));
    backend.gate.add_permits(1);
    let mut uncertain = false;
    while !uncertain {
        assert!(
            service.completion.is_none(),
            "database rejected: {:?}",
            service
                .completion
                .as_ref()
                .and_then(|c| c.failure.as_deref())
        );
        assert!(
            Instant::now() < deadline,
            "waiting receipt: {:?} attempts {} phase {:?}",
            service.failure(),
            backend.attempts.load(Ordering::SeqCst),
            service
                .pending
                .as_ref()
                .map(|p| std::mem::discriminant(&p.phase))
        );
        uncertain = tick(&mut service, &worker, &mut online, &writer.handle)
            .await
            .is_err();
    }
    assert_eq!(backend.attempts.load(Ordering::SeqCst), 1);
    let row = store.load(AMMO).await.unwrap().unwrap();
    let durable = ItemSaveV5::decode(&row.bytes).unwrap();
    assert_eq!(
        durable
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        1
    );
    let input = worker.input();
    let recovered = worker.shutdown_recover().unwrap();
    assert_eq!(
        recovered
            .kernel
            .inventory_item(EntityId(AMMO))
            .unwrap()
            .stack,
        2
    );
    assert!(
        recovered
            .kernel
            .world()
            .projectile(EntityId(PROJECTILE))
            .is_none()
    );
    writer.handle.close();
    writer.task.await.unwrap();
    assert!(service.poll(&input, &mut online, &writer.handle).is_err());
    assert!(service.has_pending());
    worker = SimulationWorker::spawn_with_output_capacity(
        recovered.kernel,
        SimulationConfig {
            command_capacity: 1,
            ..Default::default()
        },
        1,
    )
    .unwrap();
    let writer = spawn_save_worker(
        backend.clone(),
        SaveWorkerConfig {
            valuable_capacity: 1,
            ..Default::default()
        },
    )
    .unwrap();
    backend.gate.add_permits(1);
    while service.completion.is_none() {
        assert!(
            Instant::now() < deadline,
            "resource retained: {:?}",
            service.failure()
        );
        tick(&mut service, &worker, &mut online, &writer.handle)
            .await
            .unwrap();
    }
    let completion = service.take_completion().unwrap();
    assert!(completion.committed);
    assert_eq!(completion.launch, launch);
    assert_eq!(backend.attempts.load(Ordering::SeqCst), 2);
    {
        let operations = backend.operations.lock().unwrap();
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0], operations[1]);
    }
    let mut recovered = worker.shutdown_recover().unwrap();
    assert_eq!(
        recovered
            .kernel
            .inventory_item(EntityId(AMMO))
            .unwrap()
            .stack,
        1
    );
    let mut created = recovered
        .undelivered_physical_events
        .iter()
        .filter(|e| {
            matches!(
                e,
                bace_simulation::PhysicalCombatEvent::ProjectileCreated { .. }
            )
        })
        .count();
    while let Some(e) = recovered.kernel.take_physical_combat_event() {
        created += usize::from(matches!(
            e,
            bace_simulation::PhysicalCombatEvent::ProjectileCreated { .. }
        ));
    }
    assert_eq!(created, 1);
    assert_eq!(
        store.load(AMMO).await.unwrap().unwrap().persisted_version,
        3
    );
    assert_eq!(
        online
            .inventory_baseline(ACTOR, AMMO)
            .unwrap()
            .entity
            .state
            .properties
            .ints
            .iter()
            .find(|p| p.id == 12)
            .unwrap()
            .value,
        1
    );
    writer.handle.close();
    writer.task.await.unwrap();
}
