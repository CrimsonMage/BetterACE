use super::*;
use bace_inventory::{InventoryItem, ItemPlace};
fn item() -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(10),
        revision: 1,
        template: 100,
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
        valid_wield: 1,
        incompatible_wield: 0,
        wield_requirements_met: true,
    }
}
fn take_kernel() -> Kernel {
    let mut k = xp_kernel();
    k.register_inventory_item(item()).unwrap();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 74,
                weenie_class_id: Some(100),
                stack_size: Some(3),
                ..Default::default()
            }],
        )],
    );
    assert!(k.step().unwrap().is_empty());
    k
}
fn receipt(ticket: &bace_simulation::NpcInventoryTicket) -> bace_simulation::InventoryReceipt {
    bace_simulation::InventoryReceipt {
        operation: ticket.inventory.operation,
        revisions: ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
#[test]
fn take_items_is_reserved_until_exact_joint_receipt_and_recovery_does_not_repeat() {
    let mut k = take_kernel();
    let npc = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_inventory(&npc, None).unwrap();
    let accepted = receipt(&ticket);
    assert!(
        k.take_inventory_proposal().is_none(),
        "joint NPC stage owns this proposal"
    );
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    assert!(k.confirm_inventory_committed(&accepted).is_err());
    assert!(k.reject_inventory(ticket.inventory.operation).is_err());
    assert!(
        k.complete_npc_service(
            &npc,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 }
        )
        .is_err()
    );
    let mut wrong = accepted.clone();
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_npc_inventory_committed(&ticket, &wrong).is_err());
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    let now = k.checkpoint_npc_source(EntityId(2)).unwrap().logical_now;
    let checkpoint = k
        .preview_npc_committed_checkpoint(
            &npc,
            bace_gameplay_api::NpcCompletion::Applied { post_delay: 0.0 },
            now,
        )
        .unwrap();
    k.confirm_npc_inventory_committed(&ticket, &accepted)
        .unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 7);
    assert!(
        k.confirm_npc_inventory_committed(&ticket, &accepted)
            .is_err()
    );
    k.restore_npc_source(checkpoint).unwrap();
    assert!(k.step().unwrap().is_empty());
    assert!(
        !k.checkpoint_npc_source(EntityId(2))
            .unwrap()
            .pending
            .is_empty()
    );
    k.acknowledge_npc_recovery_ready(EntityId(2)).unwrap();
    assert!(k.step().unwrap().is_empty());
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 7);
    assert!(
        k.checkpoint_npc_source(EntityId(2))
            .unwrap()
            .pending
            .is_empty()
    );
}
#[test]
fn rejected_take_releases_only_inventory_and_retains_authored_row() {
    let mut k = take_kernel();
    let npc = k.take_npc_proposal().unwrap();
    let ticket = k.prepare_npc_inventory(&npc, None).unwrap();
    k.reject_npc_inventory(&ticket).unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    assert_eq!(k.npc_pending_service(npc.ticket), Some(&npc));
    let retry = k.prepare_npc_inventory(&npc, None).unwrap();
    assert_ne!(ticket.inventory.operation, retry.inventory.operation);
    k.confirm_npc_inventory_committed(&retry, &receipt(&retry))
        .unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 7);
}
#[test]
fn give_requires_source_detachment_and_exact_prepared_template() {
    let mut k = xp_kernel();
    install(
        &mut k,
        vec![set(
            7,
            None,
            vec![EmoteAction {
                r#type: 3,
                weenie_class_id: Some(100),
                stack_size: Some(10),
                ..Default::default()
            }],
        )],
    );
    k.step().unwrap();
    let npc = k.take_npc_proposal().unwrap();
    let mut prepared = item();
    prepared.id = EntityId(0x80000010);
    assert!(matches!(
        k.prepare_npc_inventory(&npc, Some(prepared.clone())),
        Err(NpcFailure::DurabilityPending)
    ));
    k.detach_npc_service(&npc, 0.0).unwrap();
    let mut wrong = prepared.clone();
    wrong.template = 101;
    assert!(k.prepare_npc_inventory(&npc, Some(wrong)).is_err());
    assert!(k.inventory_item(prepared.id).is_none());
    let ticket = k.prepare_npc_inventory(&npc, Some(prepared)).unwrap();
    k.confirm_npc_inventory_committed(&ticket, &receipt(&ticket))
        .unwrap();
    assert_eq!(k.inventory_item(EntityId(0x80000010)).unwrap().stack, 10);
}

fn handin_kernel() -> Kernel {
    let mut k = xp_kernel();
    k.register_inventory_item(item()).unwrap();
    let mut give = set(
        6,
        None,
        vec![EmoteAction {
            r#type: 47,
            amount: Some(2),
            ..Default::default()
        }],
    );
    give.weenie_class_id = Some(100);
    k.register_native_npc(
        EntityId(2),
        Arc::new(NativeProgram::prepare(vec![give], NativeLimits::default()).unwrap()),
        3.0,
    )
    .unwrap();
    k
}
fn handin_request() -> bace_simulation::NpcHandInRequest {
    bace_simulation::NpcHandInRequest {
        context: bace_gameplay_api::ActionContext {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
            sequence: 1,
        },
        source: EntityId(2),
        item: EntityId(10),
        count: 3,
        event: [55; 16],
        operation: 77,
    }
}
#[test]
fn handin_consumption_commits_before_give_row_and_recovery_never_reconsumes() {
    let mut k = handin_kernel();
    let ticket = k.prepare_npc_handin(handin_request()).unwrap();
    let accepted = bace_simulation::InventoryReceipt {
        operation: ticket.inventory.operation,
        revisions: ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(k.validate_npc_handin_snapshot(
        ticket.inventory.operation,
        EntityId(1),
        k.character(EntityId(1)).unwrap().revision()
    ));
    assert!(!k.validate_npc_handin_snapshot(
        ticket.inventory.operation + 1,
        EntityId(1),
        k.character(EntityId(1)).unwrap().revision()
    ));
    assert!(!k.validate_npc_handin_snapshot(ticket.inventory.operation, EntityId(2), 0));
    assert!(!k.validate_npc_handin_snapshot(ticket.inventory.operation, EntityId(1), 999));
    for _ in 0..5 {
        k.step().unwrap();
        assert!(k.take_npc_proposal().is_none());
    }
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    assert!(k.confirm_inventory_committed(&accepted).is_err());
    assert!(k.reject_inventory(ticket.inventory.operation).is_err());
    assert!(matches!(
        k.take_player_state(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    let mut wrong = accepted.clone();
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_npc_handin_committed(&ticket, &wrong).is_err());
    k.confirm_npc_handin_committed(&ticket, &accepted).unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 9);
    assert!(k.confirm_npc_handin_committed(&ticket, &accepted).is_err());
    assert!(k.take_npc_proposal().is_none());
    k.step().unwrap();
    let reward = k.take_npc_proposal().unwrap();
    assert!(matches!(
        reward.effect,
        NpcEffect::Service(bace_gameplay_api::NpcOperation::Reward {
            kind: bace_gameplay_api::NpcRewardKind::TrainingCredits,
            amount: 2,
            ..
        })
    ));
    assert_eq!(
        k.character(EntityId(1)).unwrap().available_skill_credits(),
        Some(5),
        "later reward is an independent durable stage"
    );
    // Restart loads the accepted consumed stack, then the same initial workflow.
    let mut recovered = handin_kernel();
    let take = recovered
        .propose_item_take(EntityId(1), EntityId(10), 1)
        .unwrap();
    let inventory = recovered.take_inventory_proposal().unwrap();
    recovered
        .confirm_inventory_committed(&bace_simulation::InventoryReceipt {
            operation: take,
            revisions: inventory
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        })
        .unwrap();
    recovered.restore_npc_source(ticket.checkpoint).unwrap();
    recovered.step().unwrap();
    assert!(recovered.take_npc_proposal().is_none());
    recovered
        .acknowledge_npc_recovery_ready(EntityId(2))
        .unwrap();
    recovered.step().unwrap();
    assert!(recovered.take_npc_proposal().is_some());
    assert_eq!(recovered.inventory_item(EntityId(10)).unwrap().stack, 9);
}
#[test]
fn failed_handin_and_wrong_identity_preserve_item_and_do_not_enqueue_give() {
    let mut k = handin_kernel();
    let mut wrong = handin_request();
    wrong.context.account = AccountId(7);
    assert!(k.prepare_npc_handin(wrong).is_err());
    let ticket = k.prepare_npc_handin(handin_request()).unwrap();
    k.reject_npc_handin(&ticket).unwrap();
    k.step().unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    assert!(k.take_npc_proposal().is_none());
    let mut retry = handin_request();
    retry.context.sequence = 2;
    assert!(k.prepare_npc_handin(retry).is_ok());
}

#[test]
fn source_refuse_precedes_give_and_commits_inspection_without_consuming() {
    let mut k = xp_kernel();
    k.register_inventory_item(item()).unwrap();
    let mut give = set(
        6,
        None,
        vec![EmoteAction {
            r#type: 47,
            amount: Some(2),
            ..Default::default()
        }],
    );
    give.weenie_class_id = Some(100);
    let mut refuse = set(
        1,
        None,
        vec![EmoteAction {
            r#type: 8,
            message: Some("No thanks".into()),
            ..Default::default()
        }],
    );
    refuse.weenie_class_id = Some(100);
    k.register_native_npc(
        EntityId(2),
        Arc::new(NativeProgram::prepare(vec![give, refuse], NativeLimits::default()).unwrap()),
        3.,
    )
    .unwrap();
    let ticket = k.prepare_npc_handin(handin_request()).unwrap();
    assert_eq!(ticket.category, 1);
    assert_eq!(ticket.accepted_count, 0);
    assert!(ticket.inventory.proposal.changes.is_empty());
    for _ in 0..5 {
        k.step().unwrap();
    }
    assert!(k.take_npc_notification().is_none());
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    let receipt = bace_simulation::InventoryReceipt {
        operation: ticket.inventory.operation,
        revisions: vec![],
    };
    assert!(k.confirm_inventory_committed(&receipt).is_err());
    k.confirm_npc_handin_committed(&ticket, &receipt).unwrap();
    k.step().unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 10);
    assert!(k.take_npc_proposal().is_none());
    let message = k.take_npc_notification().unwrap();
    assert!(
        matches!(message.operation,bace_gameplay_api::NpcOperation::Text{text,..}if text=="No thanks")
    );
}
