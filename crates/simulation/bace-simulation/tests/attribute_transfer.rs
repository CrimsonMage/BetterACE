//! Pinned AttributeTransferDevice Yes plus one exact durable character/item
//! receipt. Numerical rules are independently covered by bace-character tests.
use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, TraitProgress, TraitState,
};
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
    TraitDetails,
};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_simulation::{
    AttributeTransferCommand, AttributeTransferDeviceError, AttributeTransferResult, Command,
    InventoryReceipt, Kernel, PreparedAttributeTransfer, synthetic_scenario,
};
use bace_types::{AccountId, EntityId};
use std::sync::Arc;

fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn kernel() -> Kernel {
    let table = RankTable::new(&[0, 100]).unwrap();
    let traits = [
        TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::Strength),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute { starting_value: 50 },
        },
        TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::Endurance),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute { starting_value: 97 },
        },
    ];
    let character = CharacterProgression::with_state(
        &traits,
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        0,
        4,
    )
    .unwrap();
    let mut kernel = synthetic_scenario(1, 0).unwrap();
    kernel
        .register_character(
            CharacterBinding {
                session: SessionId(1),
                account: AccountId(1),
                actor: EntityId(1),
            },
            character,
        )
        .unwrap();
    kernel
        .register_inventory_container(InventoryContainer {
            id: EntityId(1),
            revision: 1,
            root_owner: Some(EntityId(1)),
            slots: 20,
            pack_slots: 2,
            burden_limit: 1000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    kernel
        .register_inventory_item(InventoryItem {
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
            valid_wield: 1,
            incompatible_wield: 0,
            wield_requirements_met: true,
        })
        .unwrap();
    kernel
}
fn request() -> AttributeTransferCommand {
    AttributeTransferCommand::RequestPrepared {
        context: context(1),
        item: EntityId(10),
        revision: 1,
        device: PreparedAttributeTransfer::source(1, 2).unwrap(),
        activation: None,
        wielded: vec![],
        lifetime: 1800,
    }
}
#[test]
fn inactive_attribute_device_authorizes_without_quote_or_character_item_mutation() {
    // Pinned ACE WorldObject_Use.OnActivate returns before CheckUseRequirements,
    // cooldown and AttributeTransferDevice.ActOnUse for Int119 Active=0.
    let mut k = kernel();
    let before_item = k.inventory_item(EntityId(10)).unwrap().clone();
    let mut idle = kernel();
    idle.step().unwrap();
    let request = |sequence, revision| {
        Command::AttributeTransfer(AttributeTransferCommand::RequestInactive {
            context: context(sequence),
            item: EntityId(10),
            revision,
        })
    };
    k.enqueue(request(1, 1)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_attribute_transfer_outcome().unwrap().result,
        Ok(AttributeTransferResult::Inactive)
    ));
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        idle.character(EntityId(1)).unwrap().revision()
    );
    assert_eq!(k.inventory_item(EntityId(10)), Some(&before_item));
    assert!(k.take_attribute_transfer_proposal().is_none());
    k.enqueue(request(2, 2)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_attribute_transfer_outcome().unwrap().result,
        Err(AttributeTransferDeviceError::Stale)
    ));
    k.enqueue(request(1, 1)).unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_attribute_transfer_outcome().unwrap().result,
        Err(AttributeTransferDeviceError::Ownership)
    ));
    assert!(k.take_attribute_transfer_proposal().is_none());
}
#[test]
fn yes_retains_both_owners_until_exact_combined_receipt() {
    let mut kernel = kernel();
    kernel
        .enqueue(Command::AttributeTransfer(request()))
        .unwrap();
    kernel.step().unwrap();
    let AttributeTransferResult::Confirmation(quote) = kernel
        .take_attribute_transfer_outcome()
        .unwrap()
        .result
        .unwrap()
    else {
        panic!("confirmation")
    };
    kernel
        .enqueue(Command::AttributeTransfer(
            AttributeTransferCommand::Confirm {
                context: context(2),
                token: quote.token,
                accept: true,
            },
        ))
        .unwrap();
    kernel.step().unwrap();
    let AttributeTransferResult::Proposed(Some(ticket)) = kernel
        .take_attribute_transfer_outcome()
        .unwrap()
        .result
        .unwrap()
    else {
        panic!("proposal")
    };
    assert_eq!(
        kernel.character(EntityId(1)).unwrap().revision(),
        ticket.character.proposal.expected_revision
    );
    assert!(kernel.inventory_item(EntityId(10)).is_some());
    let receipt = InventoryReceipt {
        operation: ticket.inventory.operation,
        revisions: ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|change| (change.after.id, change.after.revision))
            .collect(),
    };
    assert_eq!(
        kernel.confirm_attribute_transfer_committed(&receipt),
        Err(AttributeTransferDeviceError::Stale)
    );
    assert_eq!(
        kernel.take_attribute_transfer_proposal(),
        Some(ticket.clone())
    );
    let mut forged = receipt.clone();
    forged.revisions[0].1 += 1;
    assert!(
        kernel
            .confirm_attribute_transfer_committed(&forged)
            .is_err()
    );
    assert_eq!(
        kernel.character(EntityId(1)).unwrap().revision(),
        ticket.character.proposal.expected_revision
    );
    kernel
        .confirm_attribute_transfer_committed(&receipt)
        .unwrap();
    let character = kernel.character(EntityId(1)).unwrap();
    assert_eq!(character.revision(), ticket.character.proposal.revision);
    assert_eq!(
        character
            .projection(ProgressionTarget::Attribute(AttributeId::Strength))
            .unwrap()
            .details,
        Some(TraitDetails::Attribute { starting_value: 47 })
    );
    assert_eq!(
        character
            .projection(ProgressionTarget::Attribute(AttributeId::Endurance))
            .unwrap()
            .details,
        Some(TraitDetails::Attribute {
            starting_value: 100
        })
    );
    assert!(kernel.inventory_item(EntityId(10)).is_none());
    assert!(!kernel.has_attribute_transfer_work());
}
