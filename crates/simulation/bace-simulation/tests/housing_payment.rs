//! Synthetic single-owner adoption; PostgreSQL atomicity is tested by runtime.
use bace_gameplay_api::{ActionContext, CharacterBinding, HousingRequest, SessionId};
use bace_housing::{HousePayment, HousingActor, HousingState, PaymentItem, PurchaseRules};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_simulation::{HousingReceipt, HousingRegistration, InventoryReceipt, Kernel};
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
fn setup() -> Kernel {
    let mut k = bace_simulation::synthetic_scenario(1, 0).unwrap();
    let table = bace_character::RankTable::new(&[0, 10]).unwrap();
    let progression = bace_character::CharacterProgression::new(
        &[],
        Arc::new(bace_character::ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        0,
        0,
    )
    .unwrap();
    k.register_character(
        CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
        },
        progression,
    )
    .unwrap();
    k.register_inventory_container(InventoryContainer {
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
    k.register_inventory_item(InventoryItem {
        structure: None,
        id: EntityId(10),
        revision: 1,
        template: 273,
        stack_key: 273,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0,
        },
        stack: 12,
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
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    k.register_housing(HousingRegistration {
        slumlord: EntityId(101),
        state: HousingState {
            house: EntityId(100),
            revision: 1,
            owner: None,
            allegiance_monarch: None,
            generation: 1,
            purchased_at: 0,
            period_start: 0,
            rent_due: 0,
            interval_seconds: 30 * 86400,
            maintenance_free: false,
            open: false,
            storage_open: false,
            hooks_visible: true,
            guests: vec![],
            rent: vec![],
        },
        rules: PurchaseRules {
            minimum_level: 1,
            requires_monarch: false,
            minimum_rank: 0,
            account_age_seconds: 0,
            cooldown_seconds: 0,
            apartment: false,
            buy: vec![HousePayment {
                template: 273,
                required: 10,
                paid: 0,
            }],
        },
        owner_account: None,
    })
    .unwrap();
    k
}
fn buy(k: &mut Kernel) -> bace_simulation::HousingTicket {
    let c = ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
        sequence: 1,
    };
    let actor = HousingActor {
        actor: EntityId(1),
        account: 1,
        level: 10,
        monarch: false,
        allegiance_rank: 0,
        account_age_seconds: 100,
        previous_purchase: 0,
        owns_house: false,
        in_range: true,
    };
    k.propose_housing(
        c,
        HousingRequest::Buy {
            slumlord: EntityId(101),
            payments: vec![EntityId(10)],
        },
        actor,
        &[PaymentItem {
            item: EntityId(10),
            template: 273,
            revision: 1,
            count: 12,
            currency_value: 1,
            trade_note: false,
        }],
        100,
    )
    .unwrap();
    k.take_housing_proposal().unwrap()
}
#[test]
fn paid_house_and_inventory_adopt_together_after_both_exact_receipts() {
    let mut k = setup();
    let ticket = buy(&mut k);
    let binding = k.prepare_housing_payment(&ticket).unwrap();
    let payment = k.take_inventory_proposal().unwrap();
    assert_eq!(payment.operation, binding.inventory_operation());
    let house = HousingReceipt {
        operation: ticket.operation,
        house: ticket.proposal.after.house,
        revision: ticket.proposal.after.revision,
        generation: ticket.proposal.after.generation,
        owner: ticket.proposal.after.owner,
    };
    let receipt = InventoryReceipt {
        operation: payment.operation,
        revisions: payment
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    };
    assert!(k.confirm_housing_committed(house).is_err());
    let mut wrong = receipt.clone();
    wrong.revisions[0].1 += 1;
    assert!(k.confirm_housing_payment(&binding, house, &wrong).is_err());
    assert_eq!(k.housing_state(EntityId(100)).unwrap().owner, None);
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 12);
    k.confirm_housing_payment(&binding, house, &receipt)
        .unwrap();
    assert_eq!(
        k.housing_state(EntityId(100)).unwrap().owner,
        Some(EntityId(1))
    );
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 2);
    assert!(
        k.confirm_housing_payment(&binding, house, &receipt)
            .is_err()
    );
}
#[test]
fn confirmed_rollback_releases_pair_without_consuming_payment() {
    let mut k = setup();
    let ticket = buy(&mut k);
    let binding = k.prepare_housing_payment(&ticket).unwrap();
    k.reject_housing_payment(&binding).unwrap();
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().stack, 12);
    assert_eq!(k.housing_state(EntityId(100)).unwrap().owner, None);
    assert!(k.take_inventory_proposal().is_none());
}
