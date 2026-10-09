#[allow(dead_code, unused_imports)]
mod magic_common;
use bace_inventory::{
    InventoryAuthority, InventoryContainer, InventoryItem, ItemPlace, StackSplitPreparation,
};
use bace_simulation::{InventoryReceipt, PreparedStackDrop};
use magic_common::*;
const SOURCE: EntityId = EntityId(0x80000010);
const FRESH: EntityId = EntityId(0x80000011);
fn item(id: EntityId) -> InventoryItem {
    InventoryItem {
        id,
        revision: 1,
        template: 500,
        structure: None,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 10,
        maximum_stack: 100,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
#[allow(dead_code)]
fn preparation() -> StackSplitPreparation {
    let mut fresh = item(FRESH);
    fresh.revision = 0;
    fresh.stack = 3;
    fresh.place = ItemPlace::World;
    StackSplitPreparation {
        fresh,
        source_stackable: true,
        source_stuck: false,
        source_vendor: false,
        destination_corpse: false,
    }
}
fn authority() -> InventoryAuthority {
    InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: None,
        destination_view: None,
        new_item: Some(FRESH),
    }
}
fn drop(k: &Kernel, cell: u32) -> PreparedStackDrop {
    PreparedStackDrop {
        source_epoch: k.world().body(EntityId(1)).unwrap().accepted().epoch(),
        spawn: bace_physics::GeometrySpawn {
            cell,
            position: Vec3::new(15., 15., 0.2),
            shape: Arc::new(
                bace_physics::CollisionShape::prepare(
                    vec![bace_physics::CollisionSphere {
                        center: Vec3::new(0., 0., 0.2),
                        radius: 0.2,
                    }],
                    0.,
                    0.1,
                )
                .unwrap(),
            ),
            capabilities: Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 1.,
        },
    }
}
fn fixture() -> Kernel {
    let mut k = kernel_fixture(64, false, true);
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 1,
        burden_limit: 1000,
        accessible: true,
        open: true,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(item(SOURCE)).unwrap();
    k
}
#[allow(dead_code)]
fn request() -> bace_gameplay_api::InventoryRequest {
    bace_gameplay_api::InventoryRequest::SplitToWorld {
        item: SOURCE,
        amount: 3,
    }
}

use bace_simulation::{
    Command, InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryOperation,
    InventoryPreparedRequest,
};
fn propose(
    k: &mut Kernel,
    sequence: u32,
    request: bace_gameplay_api::InventoryRequest,
    drop: Option<PreparedStackDrop>,
) -> InventoryOperation {
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: u64::from(sequence),
        kind: InventoryCommandKind::Propose(Box::new(InventoryPreparedRequest {
            context: context(sequence),
            request,
            authority: authority(),
            split: None,
            drop: drop.map(Box::new),
        })),
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    let out = k.take_inventory_outcome().unwrap();
    let InventoryDecision::Proposed(p) = out.result.unwrap() else {
        panic!("proposal expected")
    };
    *p
}
fn receipt(p: &InventoryOperation) -> InventoryReceipt {
    InventoryReceipt {
        operation: p.ticket.operation,
        revisions: p
            .ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
fn commit(k: &mut Kernel, p: &InventoryOperation, correlation: u64) {
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation,
        kind: InventoryCommandKind::Commit(receipt(p)),
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_inventory_outcome().unwrap().result,
        Ok(InventoryDecision::Committed(_))
    ));
}
#[test]
fn drop_and_pickup_transfer_one_physical_owner_only_after_exact_durable_receipt() {
    let mut k = fixture();
    let physical = drop(&k, 1);
    let operation = propose(
        &mut k,
        1,
        bace_gameplay_api::InventoryRequest::Drop { item: SOURCE },
        Some(physical),
    );
    assert!(!k.world().contains_identity(SOURCE));
    assert!(k.take_inventory_proposal().is_none());
    assert!(k.confirm_inventory_committed(&receipt(&operation)).is_err());
    assert!(k.reject_inventory(operation.ticket.operation).is_err());
    assert!(k.retry_inventory(operation.ticket.operation).is_err());
    commit(&mut k, &operation, 2);
    assert!(k.world().contains_identity(SOURCE));
    assert_eq!(k.inventory_item(SOURCE).unwrap().place, ItemPlace::World);
    let _ = k.take_stack_world_placement();
    let operation = propose(
        &mut k,
        3,
        bace_gameplay_api::InventoryRequest::Move {
            item: SOURCE,
            container: EntityId(1),
            placement: 0,
        },
        None,
    );
    assert!(k.world().contains_identity(SOURCE));
    commit(&mut k, &operation, 4);
    assert!(!k.world().contains_identity(SOURCE));
    assert!(matches!(
        k.inventory_item(SOURCE).unwrap().place,
        ItemPlace::Contained {
            container: EntityId(1),
            ..
        }
    ));
    assert!(!k.has_inventory_command_state());
}
#[test]
fn full_correlated_output_keeps_new_drop_unexecuted_and_recoverable() {
    let mut k = fixture();
    for n in 1..=64 {
        k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
            correlation: n,
            kind: InventoryCommandKind::Reject { operation: 999 },
        })))
        .ok()
        .unwrap();
        k.step().unwrap();
    }
    assert!(k.inventory_commands_backpressured());
    let physical = drop(&k, 1);
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: 65,
        kind: InventoryCommandKind::Propose(Box::new(InventoryPreparedRequest {
            context: context(1),
            request: bace_gameplay_api::InventoryRequest::Drop { item: SOURCE },
            authority: authority(),
            split: None,
            drop: Some(Box::new(physical)),
        })),
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.inventory_item(SOURCE).unwrap().place,
        ItemPlace::Contained { .. }
    ));
    assert!(!k.world().contains_identity(SOURCE));
    let first = k.take_inventory_outcome().unwrap();
    k.restore_inventory_outcome(first).unwrap();
    assert_eq!(k.peek_inventory_outcome().unwrap().correlation, 1);
    for _ in 0..64 {
        k.take_inventory_outcome().unwrap();
    }
    k.step().unwrap();
    assert!(matches!(
        k.take_inventory_outcome().unwrap().result,
        Ok(InventoryDecision::Proposed(_))
    ));
    assert!(k.has_inventory_command_state());
}

#[test]
fn reserved_progression_before_inventory_receipt_rejects_without_consuming_sequence_or_deadlock() {
    let mut k = fixture();
    let physical = drop(&k, 1);
    let p = propose(
        &mut k,
        1,
        bace_gameplay_api::InventoryRequest::Drop { item: SOURCE },
        Some(physical),
    );
    let request = bace_gameplay_api::RaiseProgression {
        target: bace_gameplay_api::ProgressionTarget::Attribute(
            bace_gameplay_api::AttributeId::Strength,
        ),
        amount: 1,
    };
    k.try_enqueue(Command::RaiseProgression {
        context: context(2),
        request,
    })
    .ok()
    .unwrap();
    commit(&mut k, &p, 3);
    assert_eq!(
        k.take_progression_outcome().unwrap().result,
        Err(bace_gameplay_api::ProgressionActionRejection::DurabilityPending)
    );
    assert!(k.world().contains_identity(SOURCE));
    k.try_enqueue(Command::RaiseProgression {
        context: context(2),
        request,
    })
    .ok()
    .unwrap();
    k.step().unwrap();
    let result = k.take_progression_outcome().unwrap().result;
    assert_ne!(
        result,
        Err(bace_gameplay_api::ProgressionActionRejection::StaleSequence)
    );
    assert_ne!(
        result,
        Err(bace_gameplay_api::ProgressionActionRejection::DurabilityPending)
    );
}

#[test]
fn owned_ingress_rechecks_live_ancestry_without_trusting_wire_geometry_flags() {
    use bace_simulation::{
        InventoryCommand, InventoryCommandKind, InventoryDecision, InventoryPreparedRequest,
    };
    let mut k = fixture();
    let mut flags = authority();
    flags.geometry_ready = false;
    flags.in_range = false;
    flags.clear_path = false;
    let command = |sequence, correlation| {
        Command::Inventory(Box::new(InventoryCommand {
            correlation,
            kind: InventoryCommandKind::ProposeOwned(Box::new(InventoryPreparedRequest {
                context: context(sequence),
                request: bace_gameplay_api::InventoryRequest::Move {
                    item: SOURCE,
                    container: EntityId(1),
                    placement: 1,
                },
                authority: flags,
                split: None,
                drop: None,
            })),
        }))
    };
    k.try_enqueue(command(1, 1)).ok().unwrap();
    k.step().unwrap();
    let InventoryDecision::Proposed(p) = k.take_inventory_outcome().unwrap().result.unwrap() else {
        panic!("proposal");
    };
    commit(&mut k, &p, 2);
    let physical = drop(&k, 1);
    let p = propose(
        &mut k,
        2,
        bace_gameplay_api::InventoryRequest::Drop { item: SOURCE },
        Some(physical),
    );
    commit(&mut k, &p, 4);
    k.try_enqueue(command(3, 5)).ok().unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_inventory_outcome().unwrap().result,
        Err(bace_gameplay_api::InventoryRejection::OwnershipMismatch)
    ));
    assert_eq!(k.inventory_item(SOURCE).unwrap().place, ItemPlace::World);
    assert!(k.world().contains_identity(SOURCE));
    // Rejection happened before authoritative sequence admission.
    let p = propose(
        &mut k,
        3,
        bace_gameplay_api::InventoryRequest::Move {
            item: SOURCE,
            container: EntityId(1),
            placement: 0,
        },
        None,
    );
    assert_eq!(
        p.request,
        bace_gameplay_api::InventoryRequest::Move {
            item: SOURCE,
            container: EntityId(1),
            placement: 0
        }
    );
}

fn pickup_chains() -> std::collections::BTreeMap<u32, Arc<bace_motion::PreparedMotionChain>> {
    use bace_motion::{
        ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
        SourceMotionTransition,
    };
    let ready = SourceMotionState {
        style: 0x8000003d,
        substate: 0x41000003,
        speed: 1.,
    };
    let clip = ExecutionClip {
        animation: 0x03000001,
        frame_count: 2,
        low: 0,
        high: 1,
        framerate: 30.,
        frames: vec![RootFrame::default(); 2].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone()], 0, 0, physics, ready.substate, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    [0x40000018, 0x40000136, 0x40000137, 0x40000138, 0x40000139]
        .into_iter()
        .map(|motion| {
            let chain = PreparedMotionChain::prepare(
                vec![clip.clone(), clip.clone()],
                1,
                1,
                physics,
                motion,
                1.,
            )
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop.clone())
            .unwrap();
            (motion, Arc::new(chain))
        })
        .collect()
}
fn inspect(
    k: &mut Kernel,
    sequence: u32,
    request: bace_gameplay_api::InventoryRequest,
) -> bace_simulation::InventoryInspection {
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: u64::from(sequence),
        kind: InventoryCommandKind::InspectLive {
            context: context(sequence),
            request,
        },
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    let InventoryDecision::Inspected(evidence) =
        k.take_inventory_outcome().unwrap().result.unwrap()
    else {
        panic!("inspection expected")
    };
    *evidence
}
#[test]
fn live_drop_inspection_animation_and_stop_gate_exact_durable_adoption() {
    let mut k = fixture();
    let request = bace_gameplay_api::InventoryRequest::Drop { item: SOURCE };
    let evidence = inspect(&mut k, 1, request);
    // Repeating the same authenticated sequence is legal until actual proposal admission.
    assert_eq!(inspect(&mut k, 1, request).rows, evidence.rows);
    let shape = drop(&k, 1).spawn.shape;
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: 2,
        kind: InventoryCommandKind::ProposeLive(Box::new(bace_simulation::InventoryLivePrepared {
            evidence,
            request: InventoryPreparedRequest {
                context: context(1),
                request,
                authority: authority(),
                split: None,
                drop: None,
            },
            motions: pickup_chains(),
            constructed_acquisition: false,
            drop_shape: Some(shape),
            use_radius: 0.6,
        })),
    })))
    .ok()
    .unwrap();
    let mut proposed = None;
    let mut motion = Vec::new();
    for tick in 0..60 {
        k.step().unwrap();
        while let Some(out) = k.take_inventory_outcome() {
            match out
                .result
                .unwrap_or_else(|e| panic!("tick {tick}, motions {motion:?}: {e:?}"))
            {
                InventoryDecision::MotionStarted => {}
                InventoryDecision::Motion(m) => motion.push(m.command),
                InventoryDecision::Proposed(p) => proposed = Some(*p),
                other => panic!("unexpected {other:?}"),
            }
        }
        if proposed.is_some() {
            break;
        }
    }
    let proposed = proposed.expect("authoritative animation must complete");
    assert_eq!(motion, vec![Some(0x40000018), None]);
    assert!(!k.world().contains_identity(SOURCE));
    assert!(matches!(
        k.inventory_item(SOURCE).unwrap().place,
        ItemPlace::Contained { .. }
    ));
    let pose = proposed.world_placement.expect("accepted swept drop pose");
    assert!((pose.position.y - 1.1).abs() < 0.001);
    commit(&mut k, &proposed, 3);
    assert_eq!(k.take_stack_world_placement().unwrap(), pose);
    assert_eq!(
        k.world().body(SOURCE).unwrap().accepted().position().y,
        pose.position.y
    );
    for _ in 0..30 {
        k.step().unwrap();
        while k.take_inventory_outcome().is_some() {}
    }
    assert!(
        !k.has_inventory_command_state(),
        "stop callback must release the retained live operation"
    );
}

fn physical_fixture() -> Kernel {
    let source = fixture();
    let mut world = World::default();
    world
        .install_geometry(source.world().geometry().unwrap().clone())
        .unwrap();
    let shape = Arc::new(
        bace_physics::CollisionShape::prepare(
            vec![bace_physics::CollisionSphere {
                center: Vec3::new(0., 0., 0.5),
                radius: 0.5,
            }],
            0.,
            0.1,
        )
        .unwrap(),
    );
    let body = world
        .prepare_geometry_body(bace_physics::GeometrySpawn {
            cell: 1,
            position: Vec3::ZERO,
            shape,
            capabilities: Capabilities {
                speed: 5.,
                jump_impulse: 5.,
            },
            heading: 0.,
            maximum_turn_rate: std::f32::consts::PI,
        })
        .unwrap();
    world
        .insert(Actor {
            id: EntityId(1),
            cell: CellId(1),
            body,
        })
        .unwrap();
    let mut k = Kernel::with_gameplay_limits(world, 32, 2, 64).unwrap();
    k.register_character(
        CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(7),
        },
        CharacterProgression::new(
            &[TraitProgress {
                target: bace_gameplay_api::ProgressionTarget::Attribute(
                    bace_gameplay_api::AttributeId::Strength,
                ),
                experience_spent: 0,
                advancement: bace_gameplay_api::SkillAdvancement::Inactive,
            }],
            Arc::new(ProgressionTables {
                attributes: RankTable::new(&[0, 10, 100]).unwrap(),
                vitals: RankTable::new(&[0, 10, 100]).unwrap(),
                trained_skills: RankTable::new(&[0, 10, 100]).unwrap(),
                specialized_skills: RankTable::new(&[0, 10, 100]).unwrap(),
            }),
            100,
            0,
        )
        .unwrap(),
    )
    .unwrap();
    k.register_inventory_container(InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: Some(EntityId(1)),
        slots: 10,
        pack_slots: 1,
        burden_limit: 1000,
        accessible: true,
        open: true,
        generation: 1,
    })
    .unwrap();
    k.register_inventory_item(item(SOURCE)).unwrap();
    k
}
#[test]
fn live_pickup_moves_authoritatively_then_adopts_only_after_receipt() {
    let mut k = physical_fixture();
    let mut physical = drop(&k, 1);
    physical.spawn.position = Vec3::new(0., 4., 0.);
    let p = propose(
        &mut k,
        1,
        bace_gameplay_api::InventoryRequest::Drop { item: SOURCE },
        Some(physical),
    );
    commit(&mut k, &p, 2);
    k.take_stack_world_placement().unwrap();
    let request = bace_gameplay_api::InventoryRequest::Move {
        item: SOURCE,
        container: EntityId(1),
        placement: 0,
    };
    let evidence = inspect(&mut k, 3, request);
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: 4,
        kind: InventoryCommandKind::ProposeLive(Box::new(bace_simulation::InventoryLivePrepared {
            evidence,
            request: InventoryPreparedRequest {
                context: context(3),
                request,
                authority: authority(),
                split: None,
                drop: None,
            },
            motions: pickup_chains(),
            constructed_acquisition: false,
            drop_shape: None,
            use_radius: 0.6,
        })),
    })))
    .ok()
    .unwrap();
    let mut proposed = None;
    for _ in 0..300 {
        k.step().unwrap();
        while let Some(out) = k.take_inventory_outcome() {
            match out.result.unwrap() {
                InventoryDecision::MotionStarted | InventoryDecision::Motion(_) => {}
                InventoryDecision::Proposed(p) => proposed = Some(*p),
                other => panic!("unexpected {other:?}"),
            }
        }
        if proposed.is_some() {
            break;
        }
    }
    let proposed = proposed.expect("authoritative approach and pickup must complete");
    assert!(k.world().body(EntityId(1)).unwrap().accepted().position().y > 2.);
    assert!(k.world().contains_identity(SOURCE));
    assert_eq!(k.inventory_item(SOURCE).unwrap().place, ItemPlace::World);
    commit(&mut k, &proposed, 5);
    assert!(!k.world().contains_identity(SOURCE));
    assert!(matches!(
        k.inventory_item(SOURCE).unwrap().place,
        ItemPlace::Contained {
            container: EntityId(1),
            ..
        }
    ));
}

#[test]
fn teleport_during_pickup_cancels_without_consuming_action_sequence() {
    let mut k = physical_fixture();
    let request = bace_gameplay_api::InventoryRequest::Drop { item: SOURCE };
    let evidence = inspect(&mut k, 1, request);
    let shape = drop(&k, 1).spawn.shape;
    k.try_enqueue(Command::Inventory(Box::new(InventoryCommand {
        correlation: 2,
        kind: InventoryCommandKind::ProposeLive(Box::new(bace_simulation::InventoryLivePrepared {
            evidence,
            request: InventoryPreparedRequest {
                context: context(1),
                request,
                authority: authority(),
                split: None,
                drop: None,
            },
            motions: pickup_chains(),
            constructed_acquisition: false,
            drop_shape: Some(shape),
            use_radius: 0.6,
        })),
    })))
    .ok()
    .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_inventory_outcome().unwrap().result,
        Ok(InventoryDecision::MotionStarted)
    ));
    assert!(matches!(
        k.take_inventory_outcome().unwrap().result,
        Ok(InventoryDecision::Motion(_))
    ));
    k.try_enqueue(Command::ServerTeleport {
        actor: EntityId(1),
        cell: CellId(1),
        position: Vec3::new(5., 5., 0.),
    })
    .ok()
    .unwrap();
    for _ in 0..5 {
        k.step().unwrap();
    }
    assert!(k.take_inventory_outcome().unwrap().result.is_err());
    assert!(k.take_inventory_proposal().is_none());
    assert!(!k.has_inventory_command_state());
    assert!(!k.world().contains_identity(SOURCE));
    // The canceled inspection/animation never authorized this action sequence.
    inspect(&mut k, 1, request);
}
