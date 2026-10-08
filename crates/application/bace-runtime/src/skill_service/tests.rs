use super::*;
use crate::saves::{SaveBackend, SaveFailure, SaveWorkerConfig, spawn_save_worker};
use bace_gameplay_api::{
    ActionContext, ActionResult, ProgressionProjection, ProgressionTarget, SkillAdvancement,
    TraitDetails,
};
use bace_persistence::{
    CharacterLease, OperationOutcome, OwnershipState, PlacementOperation, SaveAck,
};
use bace_simulation::SkillTicket;
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV6};
use bace_types::{AccountId, EntityId};
use std::{collections::VecDeque, sync::Mutex, time::Duration};
mod owner;
fn fixture() -> (
    SkillTicket,
    PlayerSaveV6,
    CharacterLease,
    OnlinePlayerSaveService,
) {
    let before = ProgressionProjection {
        target: ProgressionTarget::Skill(6),
        experience_spent: 0,
        ranks: 0,
        advancement: SkillAdvancement::Untrained,
        details: Some(TraitDetails::Skill {
            initial_level: 0,
            resistance_at_last_check: 0,
            last_used_time: 0.,
        }),
    };
    let ticket = SkillTicket {
        operation: 9,
        context: ActionContext {
            actor: EntityId(0x50000001),
            account: AccountId(1),
            session: bace_gameplay_api::SessionId(7),
            sequence: 42,
        },
        expected_revision: 1,
        change: bace_character::SkillTransitionChange {
            before,
            after: ProgressionProjection {
                advancement: SkillAdvancement::Trained,
                ..before
            },
            available_experience: 100,
            available_skill_credits: 10,
            revision: 2,
            augmentation_added: false,
        },
    };
    let saved = PlayerSaveV6::migrate_v1(PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: ticket.context.actor.0,
            template_revision: 1,
            mutation_revision: 1,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "skill_service_test".into(),
                weenie_type: 10,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: 1,
        name: "Alice".into(),
        metadata: Default::default(),
        quests: vec![],
    })
    .unwrap();
    let lease = CharacterLease {
        character_id: ticket.context.actor.0,
        epoch: 1,
        state: OwnershipState::Online,
    };
    let mut online =
        OnlinePlayerSaveService::new(2, 1024 * 1024, tokio::time::Instant::now()).unwrap();
    online
        .register(
            binding(reference(&SkillSaveOwner::Plain(ticket))),
            lease,
            saved.clone(),
            3,
            Duration::ZERO,
        )
        .unwrap();
    (ticket, saved, lease, online)
}
#[derive(Clone, Default)]
struct Backend {
    seen: Arc<Mutex<Vec<PlacementOperation>>>,
    answers: Arc<Mutex<VecDeque<u8>>>,
}
impl SaveBackend for Backend {
    async fn placement(
        &self,
        operation: &PlacementOperation,
    ) -> Result<OperationOutcome, SaveFailure> {
        self.seen.lock().unwrap().push(operation.clone());
        match self.answers.lock().unwrap().pop_front().unwrap_or(0) {
            1 => Err(SaveFailure::Timeout),
            2 => Err(SaveFailure::Storage {
                message: "definite CAS rejection".into(),
                uncertain: false,
            }),
            _ => Ok(OperationOutcome::Committed(
                operation
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
        panic!("wrong write lane")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("unfenced write")
    }
}
fn saving(
    ticket: SkillTicket,
    saved: &PlayerSaveV6,
    lease: CharacterLease,
    online: &mut OnlinePlayerSaveService,
) -> SkillService {
    online.begin_critical(&[ticket.context.actor.0]).unwrap();
    let identity = SkillOperationId::new([8; 16]).unwrap();
    let save = crate::skill_saves::freeze_skill_ticket(identity, ticket, saved, 3, lease).unwrap();
    let mut service = SkillService::new();
    service.pending = Some(Pending {
        identity,
        owner: SkillSaveOwner::Plain(ticket),
        phase: Phase::Saving {
            save: Box::new(save),
            submitted: false,
        },
        baseline_reserved: true,
    });
    service
}
async fn until_receipt(
    service: &mut SkillService,
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
                    panic!("owner command before receipt")
                })
                .ok();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn durable_skill_waits_for_owner_ack_and_retains_command_under_pressure() {
    let (ticket, saved, lease, mut online) = fixture();
    let worker = spawn_save_worker(Backend::default(), SaveWorkerConfig::default()).unwrap();
    let mut service = saving(ticket, &saved, lease, &mut online);
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none());
    assert_eq!(online.baseline(ticket.context.actor.0).unwrap().1, 3);
    assert!(service.take_completion().is_none());
    let reply = ActionResult {
        context: ticket.context,
        result: Ok(SkillStage::Committed(ticket)),
    };
    assert!(
        service.accept_skill(reply.clone()).is_err(),
        "unsent command cannot accept a receipt"
    );
    service
        .poll_with(&mut online, &worker.handle, 1, |command| {
            Err(Box::new(TrySendError::Full(command)))
        })
        .unwrap();
    assert!(service.accept_skill(reply.clone()).is_err());
    service
        .poll_with(&mut online, &worker.handle, 1, |command| {
            assert!(matches!(command, Command::CommitSkill { ticket: t } if t == ticket));
            Ok(())
        })
        .unwrap();
    let mut stale = reply.clone();
    stale.context.session.0 += 1;
    assert!(service.accept_skill(stale).is_err());
    service.accept_skill(reply).unwrap();
    assert!(service.take_completion().is_none());
    service
        .poll_with(&mut online, &worker.handle, 1, |_| panic!())
        .unwrap();
    assert_eq!(online.baseline(ticket.context.actor.0).unwrap().1, 4);
    assert_eq!(
        online
            .baseline(ticket.context.actor.0)
            .unwrap()
            .0
            .player
            .entity
            .mutation_revision,
        2
    );
    assert!(
        service.requires_drain(),
        "output must be retained until consumed"
    );
    assert!(service.take_completion().unwrap().committed);
    assert!(!service.requires_drain());
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn uncertain_skill_never_rolls_back_and_retries_identical_frozen_operation() {
    let (ticket, saved, lease, mut online) = fixture();
    let backend = Backend::default();
    backend.answers.lock().unwrap().extend([1, 2, 0]);
    let worker = spawn_save_worker(backend.clone(), SaveWorkerConfig::default()).unwrap();
    let mut service = saving(ticket, &saved, lease, &mut online);
    for _ in 0..2 {
        until_receipt(&mut service, &mut online, &worker.handle).await;
        assert!(service.blocked().is_some());
        assert!(service.take_completion().is_none());
        assert!(online.begin_critical(&[ticket.context.actor.0]).is_err());
        service.retry();
    }
    until_receipt(&mut service, &mut online, &worker.handle).await;
    assert!(service.blocked().is_none());
    {
        let log = backend.seen.lock().unwrap();
        assert_eq!(log.len(), 3);
        for request in &log[1..] {
            assert_eq!(request.snapshots, log[0].snapshots);
            assert_eq!(request.operation_id, log[0].operation_id);
        }
    }
    service
        .poll_with(&mut online, &worker.handle, 1, |command| {
            assert!(matches!(command, Command::CommitSkill { .. }));
            Ok(())
        })
        .unwrap();
    service
        .accept_skill(ActionResult {
            context: ticket.context,
            result: Err(bace_simulation::SkillActionError::Busy),
        })
        .unwrap();
    assert!(service.blocked().is_some());
    service.retry();
    service
        .poll_with(&mut online, &worker.handle, 1, |command| {
            assert!(matches!(command, Command::CommitSkill { ticket: t } if t == ticket));
            Ok(())
        })
        .unwrap();
    service
        .accept_skill(ActionResult {
            context: ticket.context,
            result: Ok(SkillStage::Committed(ticket)),
        })
        .unwrap();
    service
        .poll_with(&mut online, &worker.handle, 1, |_| panic!())
        .unwrap();
    assert!(service.take_completion().unwrap().committed);
    worker.handle.close();
    worker.task.await.unwrap();
}
#[tokio::test]
async fn definite_rejection_releases_baseline_only_after_exact_owner_rollback() {
    let (ticket, saved, lease, mut online) = fixture();
    let backend = Backend::default();
    backend.answers.lock().unwrap().push_back(2);
    let worker = spawn_save_worker(backend, SaveWorkerConfig::default()).unwrap();
    let mut service = saving(ticket, &saved, lease, &mut online);
    until_receipt(&mut service, &mut online, &worker.handle).await;
    service
        .poll_with(&mut online, &worker.handle, 1, |command| {
            assert!(matches!(command, Command::RollbackSkill { ticket: t } if t == ticket));
            Ok(())
        })
        .unwrap();
    assert!(online.begin_critical(&[ticket.context.actor.0]).is_err());
    service
        .accept_skill(ActionResult {
            context: ticket.context,
            result: Ok(SkillStage::RolledBack(ticket)),
        })
        .unwrap();
    service
        .poll_with(&mut online, &worker.handle, 1, |_| panic!())
        .unwrap();
    assert!(!service.take_completion().unwrap().committed);
    assert_eq!(online.baseline(ticket.context.actor.0).unwrap().1, 3);
    online.begin_critical(&[ticket.context.actor.0]).unwrap();
    online.cancel_critical(&[ticket.context.actor.0]).unwrap();
    worker.handle.close();
    worker.task.await.unwrap();
}

#[tokio::test]
async fn preparation_failure_can_release_only_after_owner_rollback_and_never_cancel_a_write() {
    let worker = spawn_save_worker(Backend::default(), SaveWorkerConfig::default()).unwrap();
    for reserve in [false, true] {
        let (ticket, _, _, mut online) = fixture();
        let mut service = SkillService::new();
        service
            .stage(
                SkillOperationId::new([3; 16]).unwrap(),
                SkillSaveOwner::Plain(ticket),
            )
            .unwrap();
        if reserve {
            service
                .poll_with(&mut online, &worker.handle, 1, |_| panic!())
                .unwrap();
        }
        service.reject_unsubmitted().unwrap();
        assert!(service.requires_drain());
        service
            .poll_with(&mut online, &worker.handle, 1, |command| {
                assert!(matches!(command, Command::RollbackSkill { ticket: t } if t == ticket));
                Ok(())
            })
            .unwrap();
        service
            .accept_skill(ActionResult {
                context: ticket.context,
                result: Ok(SkillStage::RolledBack(ticket)),
            })
            .unwrap();
        service
            .poll_with(&mut online, &worker.handle, 1, |_| panic!())
            .unwrap();
        assert!(!service.take_completion().unwrap().committed);
        assert!(online.critical_ready(&[ticket.context.actor.0]).unwrap());
    }
    let (ticket, saved, lease, mut online) = fixture();
    let mut pending = saving(ticket, &saved, lease, &mut online);
    assert!(pending.reject_unsubmitted().is_err());
    assert!(pending.requires_drain());
    worker.handle.close();
    worker.task.await.unwrap();
}
