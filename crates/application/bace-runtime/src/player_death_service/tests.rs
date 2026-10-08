use super::*;
use crate::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, SaveAck, WorldPlacementOperation,
};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::{AccountId, EntityId};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
#[derive(Clone, Default)]
struct Backend {
    seen: Arc<Mutex<Vec<WorldPlacementOperation>>>,
    answers: Arc<Mutex<VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn world_placement(
        &self,
        op: &WorldPlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.seen.lock().unwrap().push(op.clone());
        match self.answers.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Err(SaveFailure::Storage {
                message: "definite failure after lost reply".into(),
                uncertain: false,
            }),
            _ => Ok(OperationOutcome::Committed(
                op.inventory
                    .snapshots
                    .iter()
                    .map(|s| SaveAck {
                        object_id: s.object_id,
                        mutation_revision: s.mutation_revision,
                        persisted_version: s.expected_version + 1,
                    })
                    .collect(),
            )),
        }
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("wrong death lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("missing world fence")
    }
}
fn fixture() -> (
    bace_simulation::Kernel,
    OnlinePlayerSaveService,
    PlayerDeathWork,
) {
    let binding = CharacterBinding {
        actor: EntityId(0x50000001),
        account: AccountId(1),
        session: bace_gameplay_api::SessionId(1),
    };
    let (mut kernel, mut admission) = crate::player_service::tests::fixture::fixture(binding);
    admission.combatant.damage(50).unwrap();
    admission.state.world.as_mut().unwrap().vitals[0]
        .as_mut()
        .unwrap()
        .current = 0;
    let formulas = admission.vital_inputs.formulas;
    assert!(kernel.admit_player(admission).is_ok());
    kernel
        .configure_player_death_random(
            Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
            7,
        )
        .unwrap();
    kernel
        .register_vitae_template(bace_magic::EnchantmentEntry {
            spell: 666,
            caster: binding.actor.0,
            school: bace_magic::MagicSchool::Life,
            spec: bace_magic::EnchantmentSpec {
                category: 204,
                power: 1,
                duration: -1.,
                layer: 1,
                stat_type: 0,
                stat_key: 0,
                value: 1.,
                beneficial: true,
                set_id: None,
            },
            start_time: 0.,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: Default::default(),
        })
        .unwrap();
    for _ in 0..3 {
        kernel.step().unwrap();
    }
    let snapshot = kernel.read_player_snapshot(binding).unwrap();
    let state = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "death_service_fixture".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let base = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: binding.actor.0,
            template_revision: 1,
            mutation_revision: 1,
            state,
        },
        account_id: 1,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let saved = crate::player_saves::freeze_player_snapshot(&base, &snapshot, 10_000).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    online
        .register(
            binding,
            CharacterLease {
                character_id: binding.actor.0,
                epoch: 1,
                state: OwnershipState::Online,
            },
            saved,
            3,
            Duration::ZERO,
        )
        .unwrap();
    kernel.begin_player_death(binding.actor, None).unwrap();
    let PlayerDeathEvent::Prepare {
        operation,
        before_revision,
        ..
    } = kernel.take_player_death_event().unwrap()
    else {
        panic!("death start")
    };
    assert_eq!(
        before_revision,
        kernel.character(binding.actor).unwrap().revision()
    );
    let world = snapshot.world();
    let corpse = EntityId(0x80000001);
    let body = bace_physics::Body::spawn_geometry(
        kernel.world().geometry().unwrap(),
        bace_physics::GeometrySpawn {
            cell: world.cell.0,
            position: world.position,
            shape: kernel
                .world()
                .body(binding.actor)
                .unwrap()
                .collision_shape()
                .unwrap()
                .clone(),
            capabilities: bace_motion::Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 0.,
        },
    )
    .unwrap();
    let mut source = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "corpse_service_fixture".into(),
        weenie_type: 14,
        last_modified: None,
        properties: Default::default(),
    };
    source.properties.ints = vec![
        bace_content::Property { id: 6, value: 120 },
        bace_content::Property { id: 7, value: 10 },
    ];
    let (item, container) = crate::generator_items::prepare_inventory_item(
        &source,
        corpse,
        0,
        bace_inventory::ItemPlace::World,
    )
    .unwrap();
    let position = bace_content::Position {
        obj_cell_id: world.cell.0,
        position_x: world.position.x,
        position_y: world.position.y,
        position_z: world.position.z,
        rotation_w: 1.,
        ..Default::default()
    };
    let prepared = bace_simulation::PreparedPlayerDeath {
        operation,
        actor: binding.actor,
        corpse: bace_entity::Actor {
            id: corpse,
            cell: world.cell,
            body,
        },
        corpse_item: item,
        corpse_container: container.unwrap(),
        possessions: vec![],
        fresh_stacks: vec![],
        coin_stacks: vec![],
        animation_seconds: 0.,
        vital_formulas: formulas,
        equipped_health: vec![],
        instantiation: None,
        olthoi: None,
    };
    assert!(
        kernel
            .apply_player_death_service(PlayerDeathServiceCommand {
                correlation: 1,
                command: PlayerDeathCommand::Prepare(Box::new(prepared))
            })
            .result
            .is_ok()
    );
    let ticket = kernel.take_player_death_proposal().unwrap();
    let fresh_items = vec![FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: None,
        entity: EntitySaveV1 {
            object_id: corpse.0,
            template_revision: 1,
            mutation_revision: 0,
            state: source,
        },
        placement: None,
        persisted_version: 0,
        enchantments: vec![],
    }];
    (
        kernel,
        online,
        PlayerDeathWork {
            epoch: 7,
            binding,
            ticket,
            fresh_items,
            positions: BTreeMap::from([(corpse.0, position)]),
        },
    )
}
#[tokio::test]
async fn uncertain_death_keeps_exact_world_checkpoint_and_waits_for_real_respawn() {
    let (mut kernel, mut online, work) = fixture();
    let actor = work.binding.actor;
    let corpse = work.ticket.corpse;
    let backend = Backend::default();
    backend.answers.lock().unwrap().extend([1, 2, 0]);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = PlayerDeathService::new();
    assert!(service.stage(work).is_ok());
    let mut token = 10;
    let mut failures = 0;
    let mut pressure = 3;
    let mut receipt_tick = None;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            token += 1;
            let result = service.poll_with(
                token,
                10_000,
                kernel.ticks(),
                &mut online,
                &worker.handle,
                |command| {
                    if matches!(command, Command::PlayerDeathService(_)) && pressure > 0 {
                        pressure -= 1;
                        return Err(Box::new(TrySendError::Full(command)));
                    }
                    kernel
                        .try_enqueue(command)
                        .map_err(|c| Box::new(TrySendError::Full(c)))
                },
            );
            if let Err(error) = result {
                assert!(
                    matches!(
                        service.pending.as_ref().unwrap().phase,
                        Phase::Saving { .. }
                    ),
                    "{error}"
                );
                failures += 1;
                assert!(service.retry().unwrap());
            }
            if service.completion.is_some() {
                break;
            }
            assert_eq!(
                online.baseline(actor.0).unwrap().1,
                3,
                "receipt alone cannot advance online baseline"
            );
            kernel.step().unwrap();
            while let Some(outcome) = kernel.take_player_snapshot_outcome() {
                assert!(outcome.result.is_ok(), "{:?}", outcome.result.err());
                service.accept_capture(outcome, 10_000).unwrap();
            }
            while let Some(outcome) = kernel.take_player_death_service_outcome() {
                assert!(outcome.result.is_ok());
                receipt_tick = Some(kernel.ticks());
                assert!(service.accept_outcome(outcome).is_ok());
            }
            while let Some(event) = kernel.take_player_death_event() {
                service.observe_event(&event);
            }
            while kernel.take_magic_event().is_some() {}
            while kernel.take_portal_event().is_some() {}
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Commands adopt at the start of tick N; their receipt is observed after
    // that step advances to N+1. The 30+90 deadlines start at adoption.
    assert_eq!(failures, 2);
    assert_eq!(pressure, 0);
    assert!(kernel.ticks() >= receipt_tick.unwrap() + 119);
    assert!(!kernel.player_death_pending(actor));
    assert!(kernel.world().contains_identity(corpse));
    assert_eq!(online.baseline(actor.0).unwrap().1, 4);
    let complete = service.take_completion().unwrap();
    assert_eq!(complete.committed.len(), 2);
    {
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        for pair in seen.windows(2) {
            assert_eq!(pair[0].world_epoch, 7);
            assert_eq!(pair[0].inventory.snapshots, pair[1].inventory.snapshots);
            assert_eq!(
                pair[0].inventory.operation_id,
                pair[1].inventory.operation_id
            );
        }
    }
    worker.handle.close();
    worker.task.await.unwrap();
}
