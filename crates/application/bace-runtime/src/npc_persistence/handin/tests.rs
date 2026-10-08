use super::*;
use bace_gameplay_api::{ActionContext, SessionId};
use bace_simulation::{InventoryTicket, NpcHandInRequest, NpcSourceCheckpoint};
use bace_types::{AccountId, EntityId};
fn pending() -> PendingNpcHandIn {
    PendingNpcHandIn {
        operation: NpcStageOperation {
            inventory: bace_persistence::PlacementOperation {
                operation_id: "npc:test:0".into(),
                snapshots: vec![bace_persistence::SaveSnapshot {
                    object_id: 10,
                    mutation_revision: 2,
                    expected_version: 5,
                    bytes: vec![1],
                }],
                participants: vec![10],
                leases: vec![],
                changes: vec![],
                storage_views: vec![],
            },
            workflow: NpcWorkflowUpdate {
                invocation: [1; 16],
                world_epoch: 1,
                expected_version: 0,
                checkpoint: vec![2],
            },
        },
        ticket: NpcHandInTicket {
            category: 6,
            character_revision: 1,
            accepted_count: 3,
            request: NpcHandInRequest {
                context: ActionContext {
                    actor: EntityId(1),
                    account: AccountId(1),
                    session: SessionId(1),
                    sequence: 1,
                },
                source: EntityId(2),
                item: EntityId(10),
                count: 1,
                event: [1; 16],
                operation: 1,
            },
            inventory: InventoryTicket {
                operation: 7,
                actor: EntityId(1),
                proposal: bace_inventory::InventoryProposal {
                    changes: vec![],
                    participants: vec![],
                    actor_burden: 0,
                    requires_pickup_motion: false,
                },
            },
            checkpoint: NpcSourceCheckpoint {
                inventory: None,
                location: None,
                properties: None,
                source_quests: None,
                archive: None,
                source: EntityId(2),
                active_operation: 1,
                invocations: vec![],
                logical_now: 0.0,
                event_id: [1; 16],
                key_version: 1,
                random_position: 0,
                vm: bace_emotes::NativeCheckpoint {
                    order: 0,
                    remaining: 1,
                    work: vec![],
                    pending: vec![],
                    detached: vec![],
                },
                pending: vec![],
            },
        },
        receiver: None,
        uncertain: false,
        terminal: false,
    }
}
fn reply(
    p: &mut PendingNpcHandIn,
    result: Result<WriteOutcome, SaveFailure>,
) -> NpcHandInResolution {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    p.receiver = Some(receiver);
    sender
        .send(crate::saves::SaveReport {
            result,
            dirty_since: None,
            started_late_by: std::time::Duration::ZERO,
        })
        .unwrap();
    p.poll().unwrap()
}
#[test]
fn uncertain_handin_retains_exact_operation_until_matching_resolution() {
    let mut pending = pending();
    let id = pending.operation.inventory.operation_id.clone();
    let bytes = pending.operation.workflow.checkpoint.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    pending.receiver = Some(receiver);
    drop(sender);
    assert!(matches!(
        pending.poll(),
        Some(NpcHandInResolution::Uncertain(_))
    ));
    assert!(matches!(
        reply(
            &mut pending,
            Err(SaveFailure::Storage {
                message: "later rollback".into(),
                uncertain: false
            })
        ),
        NpcHandInResolution::Uncertain(_)
    ));
    assert!(!pending.terminal);
    assert_eq!(pending.operation.inventory.operation_id, id);
    assert_eq!(pending.operation.workflow.checkpoint, bytes);
    assert!(matches!(
        reply(
            &mut pending,
            Ok(WriteOutcome::Valuable(OperationOutcome::Committed(vec![
                SaveAck {
                    object_id: 10,
                    mutation_revision: 3,
                    persisted_version: 6
                }
            ])))
        ),
        NpcHandInResolution::Uncertain(_)
    ));
    let NpcHandInResolution::Committed {
        receipt,
        acknowledgments,
        ..
    } = reply(
        &mut pending,
        Ok(WriteOutcome::Valuable(OperationOutcome::AlreadyCommitted)),
    )
    else {
        panic!("exact journal resolution");
    };
    assert_eq!(receipt.operation, 7);
    assert_eq!(
        acknowledgments,
        vec![SaveAck {
            object_id: 10,
            mutation_revision: 2,
            persisted_version: 6
        }]
    );
    assert!(pending.terminal);
}
