use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_content::WeenieV1;
use bace_gameplay_api::{ActionContext, SessionId};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_simulation::{PetError, PetOutcome};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};
use bace_types::{AccountId, EntityId};

fn pending(phase: use_action::Phase) -> use_action::PendingPetUse {
    use_action::PendingPetUse {
        key: SessionKey {
            id: 1,
            generation: 1,
        },
        context: ActionContext {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
            sequence: 1,
        },
        device: EntityId(10),
        device_name: "fixture essence".into(),
        phase,
        prepared: None,
        pet: Some(EntityId(0x8000_0001)),
        identity: PetOperationId::new([1; 16]).unwrap(),
        item_output: false,
    }
}

#[test]
fn unrelated_pet_outcome_is_returned_and_rejected_use_stays_owned() {
    let mut runtime = PetRuntime::new();
    runtime.use_pending = Some(pending(use_action::Phase::Submitted(7)));
    let unrelated = PetOutcome {
        correlation: 8,
        result: Err(PetError::Geometry),
    };
    assert_eq!(
        runtime
            .accept_use_outcome(unrelated)
            .unwrap_err()
            .correlation,
        8
    );
    assert!(matches!(
        runtime.use_pending.as_ref().unwrap().phase,
        use_action::Phase::Submitted(7)
    ));
    runtime
        .accept_use_outcome(PetOutcome {
            correlation: 7,
            result: Err(PetError::Geometry),
        })
        .unwrap();
    assert!(runtime.has_pending());
    assert!(matches!(
        runtime.use_pending.as_ref().unwrap().phase,
        use_action::Phase::Rejected(PetError::Geometry)
    ));
}
#[test]
fn passive_stow_outcome_retains_exact_release_operation() {
    let mut runtime = PetRuntime::new();
    runtime.use_pending = Some(pending(use_action::Phase::Submitted(7)));
    runtime
        .accept_use_outcome(PetOutcome {
            correlation: 7,
            result: Ok(bace_simulation::PetDecision::Stowing {
                operation: Some(55),
            }),
        })
        .unwrap();
    let pending = runtime.use_pending.as_ref().unwrap();
    assert!(matches!(pending.phase, use_action::Phase::WaitingStow(55)));
    assert!(pending.pet.is_none());
    assert!(pending.prepared.is_none());
}

#[tokio::test]
async fn disconnected_passive_stow_drains_committed_completion_and_despawn() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    runtime.pets.use_pending = Some(pending(use_action::Phase::WaitingStow(55)));
    runtime.pets.publications.push_back(PetEvent::Despawned {
        pet: EntityId(0x8000_0001),
        owner: EntityId(1),
    });
    runtime.pets.completions.push_back(PetCompletion {
        ticket: bace_simulation::InventoryTicket {
            operation: 55,
            actor: EntityId(1),
            proposal: bace_inventory::InventoryProposal {
                changes: vec![],
                participants: vec![],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
        summon: false,
        committed: true,
        snapshots: vec![],
    });
    runtime.project_pet_output().unwrap();
    assert!(runtime.pets.publications.is_empty());
    assert!(runtime.pets.completions.is_empty());
    assert!(runtime.pets.use_pending.is_none());
    assert!(runtime.network_output.is_empty());
}

#[tokio::test]
async fn disconnected_rejected_summon_releases_only_matching_use_owner() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    runtime.pets.use_pending = Some(pending(use_action::Phase::Saving));
    let mut device = bace_inventory::InventoryItem {
        structure: Some(3),
        id: EntityId(11),
        revision: 2,
        template: 70,
        stack_key: 1,
        place: bace_inventory::ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: true,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    };
    runtime.pets.completions.push_back(PetCompletion {
        ticket: bace_simulation::InventoryTicket {
            operation: 55,
            actor: EntityId(1),
            proposal: bace_inventory::InventoryProposal {
                changes: vec![bace_inventory::ItemChange {
                    before: None,
                    after: device.clone(),
                }],
                participants: vec![],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
        summon: true,
        committed: false,
        snapshots: vec![],
    });
    assert!(runtime.project_pet_output().is_err());
    assert!(runtime.pets.has_pending());
    assert!(runtime.network_output.is_empty());
    device.id = EntityId(10);
    runtime
        .pets
        .completions
        .front_mut()
        .unwrap()
        .ticket
        .proposal
        .changes[0]
        .after = device;
    runtime.project_pet_output().unwrap();
    assert!(!runtime.pets.has_pending());
    assert!(runtime.network_output.is_empty());
}

#[test]
fn unsupported_spawn_publication_remains_queued() {
    let mut runtime = PetRuntime::new();
    runtime.publications.push_back(PetEvent::Spawned {
        pet: EntityId(0x8000_0001),
        owner: EntityId(1),
        device: EntityId(10),
    });
    assert!(runtime.has_pending());
    assert_eq!(runtime.publications.len(), 1);
}

#[test]
fn use_capture_cannot_be_claimed_by_pet_receipt_owner() {
    let mut runtime = PetRuntime::new();
    runtime.use_pending = Some(pending(use_action::Phase::Capturing(17)));
    let outcome = bace_simulation::PlayerSnapshotOutcome {
        correlation: 17,
        result: Err(bace_simulation::CharacterRegistrationError::MissingActor),
    };
    assert!(!runtime.service.owns_capture(&outcome));
    assert!(runtime.owns_capture(&outcome));
    runtime.accept_capture(outcome).unwrap();
    assert!(matches!(
        runtime.use_pending.as_ref().unwrap().phase,
        use_action::Phase::CaptureReady
    ));
}

#[tokio::test]
async fn disconnected_rejection_releases_use_owner_without_private_output() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    runtime.pets.use_pending = Some(pending(use_action::Phase::Rejected(PetError::Geometry)));
    runtime.project_pet_output().unwrap();
    assert!(runtime.pets.use_pending.is_none());
    assert!(runtime.network_output.is_empty());
}

#[tokio::test]
async fn disconnected_snapshot_capture_remains_owned_until_result_arrives() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    runtime.pets.use_pending = Some(pending(use_action::Phase::Capturing(17)));
    runtime.poll_pet_use().unwrap();
    assert!(matches!(
        runtime.pets.use_pending.as_ref().unwrap().phase,
        use_action::Phase::Capturing(17)
    ));
}

#[tokio::test]
async fn no_charges_uses_source_chat_then_use_done() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("pet-rejection-output").unwrap(),
            password_hash: PasswordService::new(1)
                .unwrap()
                .hash(b"synthetic-password")
                .unwrap(),
        })
        .await
        .unwrap()
    else {
        panic!("new account")
    };
    let key = SessionKey {
        id: 1,
        generation: 7,
    };
    runtime.players.authenticated(key, &account).unwrap();
    let actor = EntityId(0x5000_0001);
    let binding = bace_gameplay_api::CharacterBinding {
        actor,
        account: account.id,
        session: SessionId(7),
    };
    let player = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: actor.0,
                template_revision: 1,
                mutation_revision: 4,
                state: WeenieV1 {
                    schema_version: 1,
                    weenie_id: 1,
                    class_name: "test_player".into(),
                    weenie_type: 1,
                    last_modified: None,
                    properties: Default::default(),
                },
            },
            account_id: account.id.0,
            name: "Test Player".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    runtime
        .players
        .test_admit_replication(
            key,
            Arc::new(crate::game_login::LoadedPlayer {
                is_plussed: false,
                key,
                binding,
                lease: CharacterLease {
                    character_id: actor.0,
                    epoch: 1,
                    state: OwnershipState::Online,
                },
                persisted_version: 1,
                player,
                cached_experience: 0,
                inventory: vec![],
            }),
        )
        .unwrap();
    runtime.sessions.insert(
        key,
        Session {
            account,
            connected: true,
            closing: false,
            terminated: false,
            disconnected: false,
            loading: None,
            failure: None,
        },
    );
    let mut use_pending = pending(use_action::Phase::Rejected(PetError::Use(
        bace_simulation::PetUseError::NoCharges,
    )));
    use_pending.key = key;
    use_pending.context.actor = actor;
    use_pending.context.account = binding.account;
    use_pending.context.session = binding.session;
    runtime.pets.use_pending = Some(use_pending);
    runtime.project_pet_output().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("pet output batch")
    };
    assert_eq!(messages.len(), 2);
    assert_eq!(
        u32::from_le_bytes(messages[0].1[..4].try_into().unwrap()),
        0xf7e0
    );
    assert_eq!(
        u32::from_le_bytes(messages[1].1[12..16].try_into().unwrap()),
        0x1c7
    );
    assert!(runtime.pets.use_pending.is_none());
}
