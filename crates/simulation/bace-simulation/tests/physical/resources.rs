//! Owner-side protocol; separate runtime/PostgreSQL tests prove durable receipts.
use super::*;
use bace_gameplay_api::InventoryRejection;
use bace_simulation::{
    PhysicalResourceAction as A, PhysicalResourceCommand, PhysicalResourceDecision as D,
};
fn pending_launch() -> (
    Kernel,
    bace_gameplay_api::weapon_combat::PhysicalLaunchProposal,
) {
    let mut k = native();
    let mut p = prepared(true);
    let mut launcher = p.main.clone().unwrap();
    launcher.entity = 101;
    let mut ammunition = launcher.clone();
    ammunition.entity = 202;
    ammunition.revision = 5;
    ammunition.damage = 5.;
    p.equipment.extend([
        PhysicalEquipmentStamp {
            entity: 101,
            revision: 1,
            location: 0x400000,
        },
        PhysicalEquipmentStamp {
            entity: 202,
            revision: 5,
            location: 0x800000,
        },
    ]);
    gear(&mut k, 1, 101, 1, 0x400000, 1);
    gear(&mut k, 1, 202, 5, 0x800000, 2);
    p.launcher = Some(launcher);
    p.ammunition = Some(ammunition);
    p.missile = Some(PhysicalMissileSpec {
        ammunition_count: 2,
        speed: 20.,
        radius: 0.05,
        gravity: false,
        tracking: false,
        attack_motion: 0x10000062,
        launch_seconds: 0.01,
        duration_seconds: 2.,
        damage_modifier: 1.,
    });
    k.register_physical_combat(EntityId(1), Arc::new(p))
        .unwrap();
    k.supply_physical_projectile_id(EntityId(1000)).unwrap();
    k.enqueue(combat(1, CombatRequest::ChangeMode(4))).unwrap();
    k.enqueue(combat(
        2,
        CombatRequest::TargetedMissile {
            target: EntityId(2),
            height: 3,
            accuracy: 0.5,
        },
    ))
    .unwrap();
    k.step().unwrap();
    let launch = k.take_physical_launch().unwrap();
    (k, launch)
}
fn command(
    k: &mut Kernel,
    correlation: u64,
    action: A,
) -> Arc<bace_simulation::PhysicalResourceOutcome> {
    k.enqueue(Command::PhysicalResource(Box::new(
        PhysicalResourceCommand {
            correlation,
            action,
        },
    )))
    .unwrap();
    k.step().unwrap();
    let result = k.take_physical_resource_outcome().unwrap();
    assert_eq!(result.correlation, correlation);
    result
}
#[test]
fn claimed_ammunition_waits_for_receipt_and_repeated_owner_resolution_is_idempotent() {
    let (mut k, launch) = pending_launch();
    let outcome = command(
        &mut k,
        1,
        A::Prepare {
            binding: binding(),
            launch: Box::new(launch.clone()),
        },
    );
    let D::Prepared(ticket) = outcome.result.as_ref().unwrap() else {
        panic!("missing prepared resource")
    };
    assert_eq!(ticket.snapshot.binding(), binding());
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().stack, 2);
    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert!(
        k.take_inventory_proposal().is_none(),
        "claimed launch must never reach generic consumer"
    );
    assert_eq!(
        k.retry_inventory(ticket.inventory.operation),
        Err(InventoryRejection::DurabilityPending)
    );
    assert_eq!(
        k.reject_inventory(ticket.inventory.operation),
        Err(InventoryRejection::DurabilityPending)
    );
    let receipt = bace_simulation::InventoryReceipt {
        operation: ticket.inventory.operation,
        revisions: ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert_eq!(
        k.confirm_inventory_committed(&receipt),
        Err(InventoryRejection::DurabilityPending)
    );
    let result = command(
        &mut k,
        2,
        A::Resolve {
            operation: launch.operation,
            committed: true,
        },
    );
    assert!(matches!(
        result.result,
        Ok(D::Resolved {
            committed: true,
            ..
        })
    ));
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().stack, 1);
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().revision, 6);
    let result = command(
        &mut k,
        3,
        A::Resolve {
            operation: launch.operation,
            committed: true,
        },
    );
    assert!(matches!(
        result.result,
        Ok(D::Resolved {
            committed: true,
            ..
        })
    ));
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().stack, 1);
    let result = command(
        &mut k,
        4,
        A::Resolve {
            operation: launch.operation,
            committed: false,
        },
    );
    assert!(matches!(
        result.result,
        Err(InventoryRejection::InvalidState)
    ));
    command(
        &mut k,
        5,
        A::Acknowledge {
            operation: launch.operation,
        },
    );
    assert!(!k.has_physical_resource_work());
}
#[test]
fn definite_rejection_preserves_ammunition_and_attack_recovery() {
    let (mut k, launch) = pending_launch();
    command(
        &mut k,
        1,
        A::Prepare {
            binding: binding(),
            launch: Box::new(launch.clone()),
        },
    );
    let result = command(
        &mut k,
        2,
        A::Resolve {
            operation: launch.operation,
            committed: false,
        },
    );
    assert!(matches!(
        result.result,
        Ok(D::Resolved {
            committed: false,
            ..
        })
    ));
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().stack, 2);
    assert!(k.world().projectile(EntityId(1000)).is_none());
    assert!(k.physical_recovery_pending(EntityId(1)));
    let result = command(
        &mut k,
        3,
        A::Resolve {
            operation: launch.operation,
            committed: false,
        },
    );
    assert!(matches!(
        result.result,
        Ok(D::Resolved {
            committed: false,
            ..
        })
    ));
    command(
        &mut k,
        4,
        A::Acknowledge {
            operation: launch.operation,
        },
    );
    assert!(!k.has_physical_resource_work());
}
#[test]
fn mismatched_launch_cannot_reserve_ammunition_and_exact_unprepared_abort_retains_timing() {
    let (mut k, launch) = pending_launch();
    let mut wrong = launch.clone();
    wrong.expected_count += 1;
    assert!(matches!(
        command(
            &mut k,
            1,
            A::Prepare {
                binding: binding(),
                launch: Box::new(wrong)
            }
        )
        .result,
        Err(InventoryRejection::InvalidState)
    ));
    assert_eq!(k.inventory_item(EntityId(202)).unwrap().stack, 2);
    let result = command(
        &mut k,
        2,
        A::Abort {
            binding: binding(),
            operation: launch.operation,
        },
    );
    assert!(matches!(result.result, Ok(D::Aborted { .. })));
    assert!(k.physical_recovery_pending(EntityId(1)));
    assert!(k.world().projectile(EntityId(1000)).is_none());
}
