use super::*;
use crate::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_persistence::{AllegianceCommit, OperationOutcome, SaveAck};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
#[test]
fn rejected_baseline_constructor_is_bounded() {
    assert!(AllegianceService::new(0, &[], &[]).is_err());
    let row = StoredAllegiance {
        character: 1,
        mutation_revision: 1,
        persisted_version: 1,
        bytes: vec![7],
    };
    assert!(AllegianceService::new(1, &[row.clone(), row], &[]).is_err());
}

fn ticket() -> AllegianceTicket {
    let mut metadata =
        bace_allegiance::AllegianceMetadata::new(bace_types::EntityId(0x50000001), 0x80000001);
    metadata.name = Some("New allegiance".into());
    AllegianceTicket {
        operation: 1,
        actor: metadata.monarch,
        patch: bace_allegiance::AllegiancePatch {
            before_revision: 0,
            after_revision: 1,
            nodes: vec![],
            metadata: vec![(None, Some(metadata))],
        },
        credits: vec![],
        player_changes: vec![],
        rare: None,
        npc: None,
        item_experience: vec![],
        vitae: vec![],
        vitals: vec![],
        quest_messages: vec![],
    }
}
#[derive(Clone, Default)]
struct Backend {
    seen: Arc<Mutex<Vec<AllegianceOperation>>>,
    answers: Arc<Mutex<VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn allegiance(
        &self,
        operation: &AllegianceOperation,
    ) -> Result<AllegianceCommit, SaveFailure> {
        self.seen.lock().unwrap().push(operation.clone());
        match self.answers.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Err(SaveFailure::Storage {
                message: "definite rejection".into(),
                uncertain: false,
            }),
            _ => Ok(AllegianceCommit::Committed {
                nodes: operation.nodes.iter().map(ack).collect(),
                metadata: operation.metadata.iter().map(ack).collect(),
                players: vec![],
            }),
        }
    }
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("wrong write lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("unfenced write")
    }
}
fn ack(w: &bace_persistence::AllegianceWrite) -> SaveAck {
    SaveAck {
        object_id: w.character,
        mutation_revision: w.mutation_revision,
        persisted_version: w.expected_version + 1,
    }
}
async fn until_receipt(
    service: &mut AllegianceService,
    online: &mut OnlinePlayerSaveService,
    handle: &SaveHandle,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if service.blocked().is_some()
                || service
                    .pending
                    .as_ref()
                    .is_some_and(|p| matches!(p.phase, Phase::Delivering { .. }))
            {
                break;
            }
            service
                .poll_with(online, handle, 1, |_| {
                    panic!("no owner completion before database receipt")
                })
                .ok();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn metadata_commit_waits_for_exact_owner_and_preserves_next_before_image() {
    let backend = Backend::default();
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    let mut service = AllegianceService::new(9, &[], &[]).unwrap();
    let first = ticket();
    service.stage(first.clone(), vec![], leases()).unwrap();
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none());
    let response = SocialControlOutcome {
        sequence: 2,
        result: Ok(Some(first.operation)),
    };
    assert!(service.accept_control(response.clone()).is_err());
    service
        .poll_with(&mut online, &worker.handle, 2, |command| {
            Err(Box::new(TrySendError::Full(command)))
        })
        .unwrap();
    assert!(service.accept_control(response.clone()).is_err());
    service.poll_with(&mut online,&worker.handle,2,|command|{assert!(matches!(command,Command::SocialControl(SocialControl{action:SocialControlAction::Commit(t),..}) if *t==first));Ok(())}).unwrap();
    assert!(
        service
            .accept_control(SocialControlOutcome {
                sequence: 3,
                ..response.clone()
            })
            .is_err()
    );
    service.accept_control(response).unwrap();
    assert!(service.take_completion().is_none());
    service
        .poll_with(&mut online, &worker.handle, 3, |_| panic!())
        .unwrap();
    assert!(service.take_completion().unwrap().committed);
    let before = first.patch.metadata[0].1.clone().unwrap();
    let mut after = before.clone();
    after.motd = Some("Persisted next operation".into());
    let mut second = first;
    second.operation = 2;
    second.patch.before_revision = 1;
    second.patch.after_revision = 2;
    second.patch.metadata = vec![(Some(before), Some(after))];
    service.stage(second, vec![], leases()).unwrap();
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none());
    assert_eq!(
        backend.seen.lock().unwrap()[1].metadata[0].expected_version,
        1
    );
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn ambiguous_commit_retries_same_bytes_and_cannot_be_rejected_later() {
    let backend = Backend::default();
    backend.answers.lock().unwrap().extend([1, 2, 0]);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    let mut service = AllegianceService::new(9, &[], &[]).unwrap();
    service.stage(ticket(), vec![], leases()).unwrap();
    for _ in 0..2 {
        until_receipt(&mut service, &mut online, &worker.handle).await;
        assert!(service.blocked().is_some());
        assert!(service.reject_unsubmitted().is_err());
        assert!(service.requires_drain());
        service.retry();
    }
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none());
    {
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        for op in &seen[1..] {
            assert_eq!(op.operation_id, seen[0].operation_id);
            assert_eq!(op.metadata, seen[0].metadata);
        }
    }
    service
        .poll_with(&mut online, &worker.handle, 2, |command| {
            assert!(matches!(
                command,
                Command::SocialControl(SocialControl {
                    action: SocialControlAction::Commit(_),
                    ..
                })
            ));
            Ok(())
        })
        .unwrap();
    worker.handle.close();
    worker.task.await.unwrap();
}

fn leases() -> Vec<bace_persistence::CharacterLease> {
    vec![bace_persistence::CharacterLease {
        character_id: 0x50000001,
        epoch: 0,
        state: bace_persistence::OwnershipState::Offline,
    }]
}
