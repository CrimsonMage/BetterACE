//! Synthetic output-state-machine transcript. The loaded replication fixture
//! does not claim production world admission, DAT playback, or game-client use.
use super::*;
use bace_auth::{
    AccountName, AccountRepository, CreateAccountOutcome, NewAccount, PasswordService,
};
use bace_character::AttributeTransferProposal;
use bace_content::WeenieV1;
use bace_gameplay_api::{
    AttributeId, ProgressionProjection, ProgressionTarget, SkillAdvancement, TraitDetails,
};
use bace_inventory::{InventoryItem, InventoryProposal, ItemChange, ItemPlace};
use bace_persistence::{CharacterLease, OwnershipState};
use bace_storage_codec::{EntitySaveV1, PlayerSaveV1, PlayerSaveV2, PlayerSaveV6};

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn attribute(id: AttributeId, starting_value: u32) -> ProgressionProjection {
    ProgressionProjection {
        target: ProgressionTarget::Attribute(id),
        experience_spent: 0,
        ranks: 0,
        advancement: SkillAdvancement::Inactive,
        details: Some(TraitDetails::Attribute { starting_value }),
    }
}
fn item(id: EntityId, actor: EntityId) -> InventoryItem {
    InventoryItem {
        structure: None,
        id,
        revision: 1,
        template: 63,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: actor,
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
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}

#[tokio::test]
async fn prompt_then_committed_private_output_retains_disconnect_obligation() {
    let (_cluster, _directory, mut runtime) =
        Box::pin(crate::game_runtime::tests::fixture::fixture()).await;
    let CreateAccountOutcome::Created(account) = runtime
        .bootstrap
        .store
        .create(NewAccount {
            name: AccountName::parse("attribute-transfer-output").unwrap(),
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
    let device_item = EntityId(0x5000_0002);
    let binding = CharacterBinding {
        actor,
        account: account.id,
        session: bace_gameplay_api::SessionId(7),
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
    runtime
        .players
        .admit_item_sequences(actor, device_item)
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
    let context = ActionContext {
        actor,
        account: binding.account,
        session: binding.session,
        sequence: 9,
    };
    let device = PreparedAttributeTransfer::source(1, 2).unwrap();
    let quote = Quote {
        confirmation: bace_simulation::AttributeTransferConfirmation {
            token: 17,
            actor,
            item: device_item,
            expires: 1800,
            device,
        },
        prompt: preparation::prompt(device),
        name: "Gem".into(),
    };
    runtime.attribute_transfers.pending = Some(Pending {
        key,
        context,
        item: device_item,
        identity: SkillOperationId::new([7; 16]).unwrap(),
        phase: Phase::Prompt(quote.clone()),
        quote: Some(quote.clone()),
        ticket: None,
        proposal_seen: false,
        request_retry: None,
        use_action: true,
    });
    runtime.project_attribute_transfer_output().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("ordered prompt")
    };
    assert_eq!(messages.len(), 2);
    assert_eq!(word(&messages[0].1, 12), 0x0274); // CharacterConfirmationRequest
    assert_eq!(word(&messages[0].1, 16), 3);
    assert_eq!(word(&messages[1].1, 12), 0x01c7); // UseDone(None)
    assert!(runtime.attribute_transfers.quotes.contains_key(&key));

    let mut after = item(device_item, actor);
    after.revision = 2;
    after.stack = 0;
    after.place = ItemPlace::Removed;
    let ticket = AttributeTransferDeviceTicket {
        character: bace_simulation::AttributeTransferTicket {
            operation: 1,
            context: ActionContext {
                sequence: 10,
                ..context
            },
            proposal: AttributeTransferProposal {
                from_before: attribute(AttributeId::Strength, 50),
                from_after: attribute(AttributeId::Strength, 47),
                to_before: attribute(AttributeId::Endurance, 97),
                to_after: attribute(AttributeId::Endurance, 100),
                amount: 3,
                expected_revision: 4,
                revision: 5,
            },
        },
        inventory: bace_simulation::InventoryTicket {
            operation: 22,
            actor,
            proposal: InventoryProposal {
                changes: vec![ItemChange {
                    before: Some(item(device_item, actor)),
                    after,
                }],
                participants: vec![(actor, 4), (device_item, 1)],
                actor_burden: 0,
                requires_pickup_motion: false,
            },
        },
    };
    runtime.sessions.get_mut(&key).unwrap().disconnected = true;
    runtime.attribute_transfers.quotes.remove(&key);
    runtime.attribute_transfers.pending = Some(Pending {
        key,
        context: ticket.character.context,
        item: device_item,
        identity: SkillOperationId::new([7; 16]).unwrap(),
        phase: Phase::Finished(SkillCompletion {
            owner: SkillSaveOwner::AttributeTransfer(ticket.clone()),
            committed: true,
        }),
        quote: Some(quote),
        ticket: Some(ticket),
        proposal_seen: true,
        request_retry: None,
        use_action: false,
    });
    runtime.project_attribute_transfer_output().unwrap();
    let NetworkCommand::SendOrderedBatch { messages, .. } =
        runtime.network_output.pop_front().unwrap()
    else {
        panic!("retained committed output")
    };
    assert_eq!(messages.len(), 4);
    assert_eq!(word(&messages[0].1, 0), 0x02e3);
    assert_eq!(word(&messages[1].1, 0), 0x02e3);
    assert_eq!(word(&messages[2].1, 12), 0x028a);
    assert_eq!(word(&messages[2].1, 16), 0x04e1);
    assert!(runtime.attribute_transfers.pending.is_none());
    runtime
        .players
        .test_clear_replication(key, binding)
        .unwrap();
    runtime.sessions.remove(&key);
    runtime.quiesce(Duration::ZERO).unwrap();
}
