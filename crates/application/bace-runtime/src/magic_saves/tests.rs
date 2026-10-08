use super::*;
use bace_types::EntityId;
#[test]
fn partial_or_uncertain_receipts_never_release_the_component_reservation() {
    // Receipt-only fixture; public freeze/codec tests cover actual payloads.
    let mut pending = PendingMagicSave {
        operation: PlacementOperation {
            operation_id: "magic:receipt-fixture".into(),
            snapshots: vec![
                SaveSnapshot {
                    object_id: 1,
                    mutation_revision: 5,
                    expected_version: 6,
                    bytes: vec![1],
                },
                SaveSnapshot {
                    object_id: 2,
                    mutation_revision: 8,
                    expected_version: 2,
                    bytes: vec![2],
                },
            ],
            participants: vec![1, 2],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        },
        receipt: InventoryReceipt {
            operation: 9,
            revisions: vec![(EntityId(2), 8)],
        },
        receiver: None,
        terminal: false,
        uncertain: false,
    };
    let original: Vec<_> = pending
        .operation
        .snapshots
        .iter()
        .map(|s| s.bytes.clone())
        .collect();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    pending.receiver = Some(receiver);
    drop(sender);
    assert!(matches!(
        pending.poll(),
        Some(MagicSaveResolution::Uncertain { .. })
    ));
    let result = reply(
        &mut pending,
        Ok(WriteOutcome::Valuable(OperationOutcome::Committed(vec![
            SaveAck {
                object_id: 2,
                mutation_revision: 8,
                persisted_version: 3,
            },
        ]))),
    );
    assert!(matches!(result, MagicSaveResolution::Uncertain { .. }));
    assert!(!pending.terminal);
    assert!(matches!(
        reply(
            &mut pending,
            Err(SaveFailure::Storage {
                message: "later conflict".into(),
                uncertain: false
            })
        ),
        MagicSaveResolution::Uncertain { .. }
    ));
    let result = reply(
        &mut pending,
        Ok(WriteOutcome::Valuable(OperationOutcome::Committed(vec![
            SaveAck {
                object_id: 2,
                mutation_revision: 8,
                persisted_version: 3,
            },
            SaveAck {
                object_id: 1,
                mutation_revision: 5,
                persisted_version: 7,
            },
        ]))),
    );
    let MagicSaveResolution::Committed { receipt, .. } = result else {
        panic!("complete matching receipt")
    };
    assert_eq!(receipt.operation, 9);
    assert_eq!(receipt.revisions, vec![(EntityId(2), 8)]);
    assert!(pending.terminal);
    assert_eq!(
        pending
            .operation
            .snapshots
            .iter()
            .map(|s| s.bytes.clone())
            .collect::<Vec<_>>(),
        original
    );
}
fn reply(
    pending: &mut PendingMagicSave,
    result: Result<WriteOutcome, SaveFailure>,
) -> MagicSaveResolution {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    pending.receiver = Some(receiver);
    sender
        .send(SaveReport {
            result,
            dirty_since: None,
            started_late_by: std::time::Duration::ZERO,
        })
        .unwrap();
    pending.poll().unwrap()
}
