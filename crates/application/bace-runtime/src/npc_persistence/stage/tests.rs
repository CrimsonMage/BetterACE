use super::*;
use bace_gameplay_api::NpcContext;
use bace_types::EntityId;
fn pending() -> PendingNpcStage {
    PendingNpcStage {
        operation: NpcStageOperation {
            inventory: PlacementOperation {
                operation_id: "npc:synthetic:0".into(),
                snapshots: vec![SaveSnapshot {
                    object_id: 0x50000001,
                    mutation_revision: 5,
                    expected_version: 7,
                    bytes: vec![1],
                }],
                participants: vec![0x50000001],
                leases: vec![],
                changes: vec![],
                storage_views: vec![],
            },
            workflow: NpcWorkflowUpdate {
                invocation: [1; 16],
                world_epoch: 3,
                expected_version: 0,
                checkpoint: vec![2],
            },
        },
        proposal: NpcProposal {
            ticket: 1,
            context: NpcContext {
                source: EntityId(2),
                target: Some(EntityId(0x50000001)),
                operation: 1,
            },
            effect: bace_simulation::NpcEffect::Experience {
                actor: EntityId(0x50000001),
                credit: bace_character::ExperienceCredit {
                    before_revision: 4,
                    after_revision: 5,
                    before_available: 10,
                    after_available: 20,
                },
            },
        },
        adoption: NpcStageAdoption::Effect(NpcCompletion::Applied { post_delay: 1.5 }),
        receiver: None,
        uncertain: false,
        terminal: false,
    }
}
fn reply(p: &mut PendingNpcStage, result: Result<WriteOutcome, SaveFailure>) -> NpcStageResolution {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    p.receiver = Some(receiver);
    sender
        .send(SaveReport {
            result,
            dirty_since: None,
            started_late_by: std::time::Duration::ZERO,
        })
        .unwrap();
    p.poll().unwrap()
}
#[test]
fn uncertain_stage_cannot_be_rolled_back_by_a_later_rejection() {
    let mut p = pending();
    let id = p.operation.inventory.operation_id.clone();
    let bytes = p.operation.workflow.checkpoint.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    p.receiver = Some(receiver);
    drop(sender);
    assert!(matches!(
        p.poll(),
        Some(NpcStageResolution::Uncertain { .. })
    ));
    assert!(matches!(
        reply(
            &mut p,
            Err(SaveFailure::Storage {
                message: "disconnected".into(),
                uncertain: false
            })
        ),
        NpcStageResolution::Uncertain { .. }
    ));
    assert!(!p.terminal);
    assert_eq!(p.operation.inventory.operation_id, id);
    assert_eq!(p.operation.workflow.checkpoint, bytes);
    let result = reply(
        &mut p,
        Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)),
    );
    let NpcStageResolution::Committed {
        proposal,
        adoption,
        acknowledgments,
    } = result
    else {
        panic!("resolved committed stage")
    };
    assert_eq!(proposal, p.proposal);
    assert_eq!(adoption, p.adoption);
    assert_eq!(
        acknowledgments,
        vec![SaveAck {
            object_id: 0x50000001,
            mutation_revision: 5,
            persisted_version: 8
        }]
    );
    assert!(p.terminal);
}
#[test]
fn mismatched_receipt_never_releases_a_stage_and_definite_initial_rejection_can() {
    let mut p = pending();
    let wrong = OperationOutcome::Committed(vec![SaveAck {
        object_id: 0x50000001,
        mutation_revision: 4,
        persisted_version: 8,
    }]);
    assert!(matches!(
        reply(&mut p, Ok(WriteOutcome::Valuable(wrong))),
        NpcStageResolution::Uncertain { .. }
    ));
    assert!(!p.terminal);
    let exact = OperationOutcome::Committed(vec![SaveAck {
        object_id: 0x50000001,
        mutation_revision: 5,
        persisted_version: 8,
    }]);
    assert!(matches!(
        reply(&mut p, Ok(WriteOutcome::Valuable(exact))),
        NpcStageResolution::Committed { .. }
    ));
    assert!(p.terminal);
    let mut rejected = pending();
    assert!(matches!(
        reply(
            &mut rejected,
            Err(SaveFailure::Storage {
                message: "CAS conflict before write".into(),
                uncertain: false
            })
        ),
        NpcStageResolution::Rejected { .. }
    ));
    assert!(rejected.terminal);
}
