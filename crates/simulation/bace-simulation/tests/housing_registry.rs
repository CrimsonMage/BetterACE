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
            count: 10,
            currency_value: 1,
            trade_note: false,
        }],
        100,
    )
    .unwrap();
    k.take_housing_proposal().unwrap()
}

use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
fn registry(duration: f64) -> EnchantmentRegistry {
    EnchantmentRegistry::restore(
        16,
        20,
        vec![EnchantmentEntry {
            spell: 123,
            caster: 100,
            school: MagicSchool::Creature,
            spec: EnchantmentSpec {
                category: 23,
                power: 100,
                duration,
                layer: 7,
                stat_type: 0x2008000,
                stat_key: 7,
                value: 12.5,
                beneficial: true,
                set_id: None,
            },
            start_time: -15.0,
            is_set_spell: false,
            is_level8_aura: false,
            metadata: EnchantmentMetadata {
                enchantment_category: 12,
                degrade_modifier: 0.1,
                degrade_limit: 0.2,
                last_time_degraded: -3.0,
                ..Default::default()
            },
        }],
    )
    .unwrap()
}

#[test]
fn housing_payment_freezes_compacted_sibling_and_cannot_adopt_either_half() {
    for commit in [false, true] {
        let mut k = setup();
        let mut sibling = k.inventory_item(EntityId(10)).unwrap().clone();
        sibling.id = EntityId(11);
        sibling.place = ItemPlace::Contained {
            container: EntityId(1),
            slot: 1,
            equipped: 0,
        };
        k.register_inventory_item(sibling).unwrap();
        for id in [10, 11] {
            k.register_magic_registry(EntityId(id), registry(60.0), true)
                .unwrap();
        }
        let ticket = buy(&mut k);
        let binding = k.prepare_housing_payment(&ticket).unwrap();
        let payment = k.take_inventory_proposal().unwrap();
        assert!(
            payment
                .proposal
                .changes
                .iter()
                .any(|v| v.after.id == EntityId(11))
        );
        assert!(k.reject_inventory(payment.operation).is_err());
        assert!(k.reject_housing(ticket.operation).is_err());
        let receipt = InventoryReceipt {
            operation: payment.operation,
            revisions: payment
                .proposal
                .changes
                .iter()
                .map(|c| (c.after.id, c.after.revision))
                .collect(),
        };
        assert!(k.confirm_inventory_committed(&receipt).is_err());
        for _ in 0..300 {
            k.step().unwrap();
        }
        for id in [10, 11] {
            assert_eq!(
                k.magic_registry(EntityId(id)).unwrap().entries()[0].start_time,
                -15.0
            );
        }
        if commit {
            let house = HousingReceipt {
                operation: ticket.operation,
                house: ticket.proposal.after.house,
                revision: ticket.proposal.after.revision,
                generation: ticket.proposal.after.generation,
                owner: ticket.proposal.after.owner,
            };
            k.confirm_housing_payment(&binding, house, &receipt)
                .unwrap();
            assert!(k.magic_registry(EntityId(10)).is_none());
            assert!(k.inventory_item(EntityId(10)).is_none());
        } else {
            k.reject_housing_payment(&binding).unwrap();
            assert!(k.magic_registry(EntityId(10)).is_some());
        }
        k.step().unwrap();
        assert_eq!(
            k.magic_registry(EntityId(11)).unwrap().entries()[0].start_time,
            -25.0
        );
    }
}
