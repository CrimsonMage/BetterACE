//! Synthetic single-owner lifecycle integration; durable receipts are supplied by
//! the test. Real transaction/payment/retention behavior is covered by PG tests.
use bace_gameplay_api::{ActionContext, CharacterBinding, HousingRequest, SessionId};
use bace_housing::*;
use bace_simulation::{HousingReceipt, HousingRegistration};
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
fn actor() -> HousingActor {
    HousingActor {
        actor: EntityId(1),
        account: 1,
        level: 100,
        monarch: false,
        allegiance_rank: 0,
        account_age_seconds: 0,
        previous_purchase: 0,
        owns_house: true,
        in_range: true,
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
fn permissions_and_offline_due_effects_wait_for_exact_committed_receipts() {
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
    let house = EntityId(100);
    let state = HousingState {
        house,
        revision: 1,
        owner: Some(EntityId(1)),
        allegiance_monarch: None,
        generation: 1,
        purchased_at: 1,
        period_start: 1,
        rent_due: 2,
        interval_seconds: 30 * 86400,
        maintenance_free: false,
        open: false,
        storage_open: false,
        hooks_visible: true,
        guests: vec![],
        rent: vec![HousePayment {
            template: 273,
            required: 10,
            paid: 0,
        }],
    };
    let view = state.open_view(EntityId(1), true).unwrap();
    kernel
        .register_housing(HousingRegistration {
            slumlord: EntityId(101),
            state: state.clone(),
            rules: PurchaseRules {
                minimum_level: 0,
                requires_monarch: false,
                minimum_rank: 0,
                account_age_seconds: 0,
                cooldown_seconds: 0,
                apartment: false,
                buy: vec![],
            },
            owner_account: Some(1),
        })
        .unwrap();
    let operation = kernel
        .propose_housing(context(1), HousingRequest::SetOpen(true), actor(), &[], 1)
        .unwrap();
    assert!(!kernel.housing_state(house).unwrap().open);
    let ticket = kernel.take_housing_proposal().unwrap();
    assert_eq!(ticket.operation, operation);
    let mut receipt = HousingReceipt {
        operation,
        house,
        revision: ticket.proposal.after.revision,
        generation: ticket.proposal.after.generation,
        owner: ticket.proposal.after.owner,
    };
    receipt.revision += 1;
    assert!(kernel.confirm_housing_committed(receipt).is_err());
    assert_eq!(kernel.housing_state(house).unwrap(), &state);
    receipt.revision -= 1;
    kernel.confirm_housing_committed(receipt).unwrap();
    assert!(kernel.housing_state(house).unwrap().open);
    assert!(
        kernel
            .housing_state(house)
            .unwrap()
            .validate_view(view)
            .is_err()
    );
    assert_eq!(kernel.next_housing_rent(3), Some(house));
    let operation = kernel
        .propose_due_housing_rent(house, 3, true, true)
        .unwrap()
        .unwrap();
    let ticket = kernel.take_housing_proposal().unwrap();
    assert!(kernel.housing_state(house).unwrap().owner.is_some());
    kernel.retry_housing(operation).unwrap();
    assert_eq!(kernel.take_housing_proposal().unwrap(), ticket);
    kernel
        .confirm_housing_committed(HousingReceipt {
            operation,
            house,
            revision: ticket.proposal.after.revision,
            generation: ticket.proposal.after.generation,
            owner: None,
        })
        .unwrap();
    assert!(kernel.housing_state(house).unwrap().owner.is_none());
    assert!(!kernel.housing_permits(house, actor(), true));
    assert!(kernel.next_housing_rent(100).is_none());
}
