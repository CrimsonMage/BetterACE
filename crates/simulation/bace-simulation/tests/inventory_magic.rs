//! Registry freezing, retirement and time debt around exact inventory receipts.
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_inventory::*;
use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
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

fn ticks(k: &mut Kernel, n: usize) {
    for _ in 0..n {
        k.step().unwrap();
    }
}
fn receipt(t: &bace_simulation::InventoryTicket) -> InventoryReceipt {
    InventoryReceipt {
        operation: t.operation,
        revisions: t
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect(),
    }
}
#[test]
fn exact_participants_retain_timer_debt_and_removed_registry_retires_after_commit() {
    let mut k = kernel();
    for (id, slot) in [(10, 0), (11, 1)] {
        let mut v = item(id, slot);
        v.stack = 1;
        k.register_inventory_item(v).unwrap();
        k.register_magic_registry(EntityId(id), registry(60.0), true)
            .unwrap();
    }
    // Distinct owner is deliberately not a participant of player 1's operation.
    k.register_inventory_container(InventoryContainer {
        id: EntityId(2),
        revision: 1,
        root_owner: Some(EntityId(2)),
        slots: 10,
        pack_slots: 2,
        burden_limit: 1000,
        accessible: true,
        open: false,
        generation: 1,
    })
    .unwrap();
    let mut unrelated = item(20, 0);
    unrelated.place = ItemPlace::Contained {
        container: EntityId(2),
        slot: 0,
        equipped: 0,
    };
    k.register_inventory_item(unrelated).unwrap();
    k.register_magic_registry(EntityId(20), registry(60.0), true)
        .unwrap();
    let operation = k.propose_item_take(EntityId(1), EntityId(10), 1).unwrap();
    let ticket = k.take_inventory_proposal().unwrap();
    assert_eq!(operation, ticket.operation);
    assert!(
        ticket
            .proposal
            .changes
            .iter()
            .any(|c| c.after.id == EntityId(11))
    );
    ticks(&mut k, 300);
    assert_eq!(
        k.magic_registry(EntityId(10)).unwrap().entries()[0].start_time,
        -15.0
    );
    assert_eq!(
        k.magic_registry(EntityId(11)).unwrap().entries()[0].start_time,
        -15.0
    );
    assert_eq!(
        k.magic_registry(EntityId(20)).unwrap().entries()[0].start_time,
        -25.0
    );
    let mut bad = receipt(&ticket);
    bad.revisions[0].1 += 1;
    assert!(k.confirm_inventory_committed(&bad).is_err());
    assert!(k.inventory_item(EntityId(10)).is_some());
    assert!(k.magic_registry(EntityId(10)).is_some());
    k.confirm_inventory_committed(&receipt(&ticket)).unwrap();
    assert!(k.inventory_item(EntityId(10)).is_none());
    assert!(k.magic_registry(EntityId(10)).is_none());
    ticks(&mut k, 1);
    assert_eq!(
        k.magic_registry(EntityId(11)).unwrap().entries()[0].start_time,
        -25.0
    );
    assert_eq!(
        k.inventory_item(EntityId(11)).unwrap().place,
        ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0
        }
    );
}
#[test]
fn rollback_releases_timer_debt_without_retiring_item() {
    let mut k = kernel();
    let mut v = item(10, 0);
    v.stack = 1;
    k.register_inventory_item(v).unwrap();
    k.register_magic_registry(EntityId(10), registry(60.0), true)
        .unwrap();
    let operation = k.propose_item_take(EntityId(1), EntityId(10), 1).unwrap();
    ticks(&mut k, 300);
    k.reject_inventory(operation).unwrap();
    ticks(&mut k, 1);
    assert!(k.inventory_item(EntityId(10)).is_some());
    assert_eq!(
        k.magic_registry(EntityId(10)).unwrap().entries()[0].start_time,
        -25.0
    );
}
