use bace_crafting::*;
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_replication::{BatchLimits, EventSequencer};
use bace_runtime::crafting_output::project_crafting_outcome;
use bace_simulation::{CraftingDecision, CraftingOutcome, CraftingResult, CraftingTicket};
use bace_types::{AccountId, EntityId};
use std::collections::BTreeMap;
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(2),
        session: SessionId(3),
    }
}
fn limits() -> BatchLimits {
    BatchLimits {
        max_messages: 300,
        max_bytes: 100000,
        max_message_bytes: 4096,
        max_string_bytes: 4096,
    }
}
fn proposal(empty: bool) -> SalvageProposal {
    let items = [SalvageInput {
        id: 3,
        owner: 1,
        revision: 1,
        stack: 1,
        material: 61,
        raw_workmanship: 100,
        value: 100,
        retained: empty,
        equipped: false,
        in_trade: false,
        reserved: false,
        is_salvage: false,
        structure: 0,
        num_items: 0,
    }];
    propose_salvage(SalvageRequest {
        actor: 1,
        actor_revision: 1,
        operation_id: [1; 16],
        tool: SalvageTool {
            id: 2,
            owner: 1,
            revision: 1,
            is_ust: true,
            reserved: false,
        },
        skills: SalvageSkills {
            salvaging: 195,
            armor: 0,
            weapon: 0,
            magic_item: 0,
            item: 0,
            augmentations: 0,
        },
        items: &items,
        bag_templates: &BTreeMap::from([(61, 100)]),
        free_slots: 10,
    })
    .unwrap()
}
#[test]
fn pending_retry_and_rollback_have_no_success_output_but_committed_bags_do() {
    let b = binding();
    let mut sequence = EventSequencer::new(b, 1);
    for result in [
        Ok(CraftingResult::Pending(1)),
        Ok(CraftingResult::Retried(1)),
        Ok(CraftingResult::RolledBack(1)),
        Err(CraftError::Busy),
    ] {
        let outcome = CraftingOutcome {
            correlation: 1,
            result,
        };
        assert!(
            project_crafting_outcome(&mut sequence, b, &outcome, limits())
                .unwrap()
                .is_none()
        );
        assert_eq!(sequence.next_sequence(), 1);
    }
    let proposal = proposal(false);
    assert_eq!(proposal.bags.len(), 2);
    assert_eq!(proposal.results.len(), 2);
    let ticket = CraftingTicket {
        operation: 1,
        actor: b.actor,
        decision: CraftingDecision::Salvage(Box::new(proposal)),
        registry_reservations: vec![],
        proficiency: None,
        inventory: bace_inventory::InventoryProposal {
            changes: vec![],
            participants: vec![],
            actor_burden: 0,
            requires_pickup_motion: false,
        },
    };
    let outcome = CraftingOutcome {
        correlation: 2,
        result: Ok(CraftingResult::Committed(Box::new(ticket))),
    };
    let batch = project_crafting_outcome(&mut sequence, b, &outcome, limits())
        .unwrap()
        .unwrap();
    assert_eq!(batch.messages.len(), 2);
    assert_eq!(sequence.next_sequence(), 3);
}
#[test]
fn no_mutation_unsuitable_reply_is_allowed_but_mislabelled_nonempty_is_rejected() {
    let b = binding();
    let mut sequence = EventSequencer::new(b, 1);
    let outcome = CraftingOutcome {
        correlation: 1,
        result: Ok(CraftingResult::SalvageEmpty(Box::new(proposal(true)))),
    };
    let batch = project_crafting_outcome(&mut sequence, b, &outcome, limits())
        .unwrap()
        .unwrap();
    assert_eq!(batch.messages.len(), 1);
    let outcome = CraftingOutcome {
        correlation: 2,
        result: Ok(CraftingResult::SalvageEmpty(Box::new(proposal(false)))),
    };
    assert!(project_crafting_outcome(&mut sequence, b, &outcome, limits()).is_err());
    assert_eq!(sequence.next_sequence(), 2);
}
