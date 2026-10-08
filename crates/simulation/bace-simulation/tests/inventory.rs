use bace_gameplay_api::{
    ActionContext, CharacterBinding, InventoryRejection as Error, InventoryRequest as Request,
    SessionId,
};
use bace_inventory::*;
use bace_simulation::{InventoryReceipt, Kernel};
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
fn kernel() -> Kernel {
    let mut kernel = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let ranks = bace_character::RankTable::new(&[0, 10]).unwrap();
    let character = bace_character::CharacterProgression::new(
        &[],
        Arc::new(bace_character::ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        0,
        0,
    )
    .unwrap();
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
            slots: 10,
            pack_slots: 2,
            burden_limit: 1000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    kernel
}
fn item(id: u32, slot: u32) -> InventoryItem {
    InventoryItem {
        structure: None,
        id: EntityId(id),
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot,
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
fn authority() -> InventoryAuthority {
    InventoryAuthority {
        actor: EntityId(1),
        busy: false,
        in_range: true,
        clear_path: true,
        geometry_ready: true,
        drop_validated: true,
        source_view: None,
        destination_view: None,
        new_item: None,
    }
}
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(1),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
#[test]
fn accepted_ownership_changes_only_after_exact_receipt_and_retry_preserves_proposal() {
    let mut kernel = kernel();
    kernel.register_inventory_item(item(0x80000001, 0)).unwrap();
    let operation = kernel
        .propose_inventory(
            context(1),
            Request::Drop {
                item: EntityId(0x80000001),
            },
            authority(),
        )
        .unwrap();
    assert_eq!(kernel.inventory_count(EntityId(1), 100), 10);
    assert!(
        kernel
            .take_character(CharacterBinding {
                session: SessionId(1),
                account: AccountId(1),
                actor: EntityId(1)
            })
            .is_err()
    );
    let ticket = kernel.take_inventory_proposal().unwrap();
    assert_eq!(ticket.operation, operation);
    assert_eq!(
        kernel.propose_inventory(
            context(2),
            Request::Drop {
                item: EntityId(0x80000001)
            },
            authority()
        ),
        Err(Error::DurabilityPending)
    );
    assert!(
        kernel
            .confirm_inventory_committed(&InventoryReceipt {
                operation,
                revisions: vec![]
            })
            .is_err()
    );
    kernel.retry_inventory(operation).unwrap();
    assert_eq!(kernel.take_inventory_proposal().unwrap(), ticket);
    let receipt = InventoryReceipt {
        operation,
        revisions: ticket
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    kernel.confirm_inventory_committed(&receipt).unwrap();
    assert_eq!(
        kernel.inventory_item(EntityId(0x80000001)).unwrap().place,
        ItemPlace::World
    );
    assert!(kernel.confirm_inventory_committed(&receipt).is_err());
}
#[test]
fn native_take_and_component_burns_are_atomic_and_failed_requirements_do_not_reserve() {
    let mut kernel = kernel();
    kernel.register_inventory_item(item(0x80000001, 0)).unwrap();
    kernel.register_inventory_item(item(0x80000002, 1)).unwrap();
    assert!(
        kernel
            .has_spell_components(EntityId(1), &[(100, 20)])
            .unwrap()
    );
    assert!(
        kernel
            .propose_component_use(EntityId(1), &[(100, 21)], &[(100, 1)])
            .is_err()
    );
    let operation = kernel
        .propose_component_use(EntityId(1), &[(100, 20)], &[(100, 3)])
        .unwrap();
    let ticket = kernel.take_inventory_proposal().unwrap();
    assert_eq!(kernel.inventory_count(EntityId(1), 100), 20);
    kernel
        .confirm_inventory_committed(&InventoryReceipt {
            operation,
            revisions: ticket
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        })
        .unwrap();
    assert_eq!(kernel.inventory_count(EntityId(1), 100), 17);
    let operation = kernel
        .propose_template_take(EntityId(1), 100, None)
        .unwrap();
    let ticket = kernel.take_inventory_proposal().unwrap();
    kernel
        .confirm_inventory_committed(&InventoryReceipt {
            operation,
            revisions: ticket
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        })
        .unwrap();
    assert_eq!(kernel.inventory_count(EntityId(1), 100), 0);
    assert_eq!(kernel.inventory_free_slots(EntityId(1), false).unwrap(), 10);
}
