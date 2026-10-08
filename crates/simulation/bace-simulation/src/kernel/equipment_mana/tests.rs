use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_gameplay_api::SessionId;
use bace_inventory::{InventoryContainer, InventoryItem};
use bace_types::AccountId;
use std::sync::Arc;
fn fixture(mana: i32, rate: f64) -> Kernel {
    let mut k = crate::synthetic_scenario(1, 0).unwrap();
    let ranks = RankTable::new(&[0, 10]).unwrap();
    k.register_character(
        binding(),
        CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: ranks.clone(),
                vitals: ranks.clone(),
                trained_skills: ranks.clone(),
                specialized_skills: ranks,
            }),
            0,
            0,
        )
        .unwrap(),
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
        id: EntityId(2),
        revision: 1,
        template: 100,
        stack_key: 1,
        place: ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 1,
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
    let mut registry = bace_magic::EnchantmentRegistry::new(8).unwrap();
    registry
        .add(
            bace_magic::EnchantmentEntry {
                spell: 1,
                caster: 2,
                school: bace_magic::MagicSchool::Creature,
                spec: bace_magic::EnchantmentSpec {
                    category: 1,
                    power: 100,
                    duration: 60.,
                    layer: 1,
                    stat_type: 4 | 0x8000,
                    stat_key: 1,
                    value: 10.,
                    beneficial: true,
                    set_id: None,
                },
                start_time: 0.,
                is_set_spell: false,
                is_level8_aura: false,
                metadata: Default::default(),
            },
            0.,
            true,
        )
        .unwrap();
    k.register_magic_registry(EntityId(1), registry, true)
        .unwrap();
    k.register_magic_registry(
        EntityId(2),
        bace_magic::EnchantmentRegistry::new(8).unwrap(),
        true,
    )
    .unwrap();
    k.register_equipment_mana(
        binding(),
        EquipmentManaRecovery {
            fresh: false,
            heartbeat: 5.,
            rating: 0,
            heartbeat_remaining: 5.,
            items: vec![crate::EquipmentManaItem {
                item: EntityId(2),
                name: "Test".into(),
                current: Some(mana),
                maximum: Some(100),
                rate: Some(rate),
                affecting: Some(true),
                removals: vec![(EntityId(1), 1)],
                accumulator: 0.,
                warned: false,
                removal_remaining: None,
            }],
        },
    )
    .unwrap();
    k
}
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
    }
}
#[test]
fn held_heartbeat_retains_fraction_and_resumes_one_source_beat_without_mutating_snapshot() {
    let mut k = fixture(10, -0.15);
    k.step_equipment_mana(5.).unwrap();
    let first = k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)].clone();
    assert_eq!(first.current, Some(10));
    assert_eq!(first.accumulator, 0.75);
    k.magic.reserve_registry(EntityId(1), true, 5.).unwrap();
    k.step_equipment_mana(15.).unwrap();
    assert_eq!(
        k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)],
        first
    );
    k.magic.reserve_registry(EntityId(1), false, 15.).unwrap();
    k.step_equipment_mana(15.).unwrap();
    k.step_equipment_mana(15.).unwrap();
    let state = &k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)];
    assert_eq!(state.current, Some(9));
    assert_eq!(state.accumulator, 0.5);
    assert_eq!(k.inventory.item(EntityId(2)).unwrap().revision, 2);
    assert_eq!(k.social.events.len(), 1, "low mana warns once");
}
#[test]
fn depletion_waits_two_seconds_and_retains_work_under_another_owner_hold() {
    let mut k = fixture(1, -1.);
    k.step_equipment_mana(5.).unwrap();
    assert!(k.equipment_mana_pending(EntityId(1)));
    assert_eq!(
        k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)].affecting,
        None
    );
    assert!(matches!(
        k.social.events.front(),
        Some(SocialEvent::EquipmentMana { depleted: true, .. })
    ));
    k.step_equipment_mana(6.99).unwrap();
    assert!(k.equipment_mana_pending(EntityId(1)));
    k.magic.reserve_registry(EntityId(1), true, 7.).unwrap();
    k.step_equipment_mana(8.).unwrap();
    assert!(k.equipment_mana_pending(EntityId(1)));
    k.magic.reserve_registry(EntityId(1), false, 8.).unwrap();
    k.step_equipment_mana(8.).unwrap();
    assert!(!k.equipment_mana_pending(EntityId(1)));
    assert!(k.magic.registry(EntityId(1)).unwrap().entries().is_empty());
    assert!(matches!(
        k.magic.take_event(),
        Some(crate::MagicEvent::EnchantmentExpired {
            actor: EntityId(1),
            ..
        })
    ));
}
#[test]
fn output_pressure_keeps_mana_revision_and_accumulator_unchanged() {
    let mut k = fixture(1, -1.);
    k.social.capacity = 0;
    k.step_equipment_mana(5.).unwrap();
    let item = &k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)];
    assert_eq!(item.current, Some(1));
    assert_eq!(item.accumulator, 0.);
    assert_eq!(k.inventory.item(EntityId(2)).unwrap().revision, 1);
    k.social.capacity = 1;
    k.step_equipment_mana(5.).unwrap();
    assert_eq!(
        k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)].current,
        Some(0)
    );
    assert_eq!(k.social.events.len(), 1);
}

#[test]
fn owner_recovery_preserves_fraction_warning_and_phase_without_reinitialization() {
    let mut k = fixture(10, -0.15);
    k.step_equipment_mana(5.).unwrap();
    k.tick = 150;
    let saved = k.equipment_mana_snapshot(EntityId(1)).unwrap();
    assert!(!saved.fresh);
    assert_eq!(saved.items[0].accumulator, 0.75);
    k.equipment_mana.players.remove(&EntityId(1));
    k.tick = 3000;
    k.register_equipment_mana(binding(), saved.clone()).unwrap();
    assert_eq!(k.equipment_mana_snapshot(EntityId(1)).unwrap(), saved);
    k.step_equipment_mana(104.99).unwrap();
    assert_eq!(
        k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)].current,
        Some(10)
    );
    k.step_equipment_mana(105.).unwrap();
    assert_eq!(
        k.equipment_mana.players[&EntityId(1)].items[&EntityId(2)].current,
        Some(9)
    );
}

#[test]
fn cold_zero_mana_cleanup_blocks_entry_until_exact_caster_removal_without_replaying_notice() {
    let mut k = fixture(0, -0.2);
    let mut state = k.equipment_mana_snapshot(EntityId(1)).unwrap();
    k.equipment_mana.players.remove(&EntityId(1));
    state.fresh = true;
    state.items[0].affecting = None;
    state.items[0].removal_remaining = Some(0.);
    k.configure_social_random(Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()))
        .unwrap();
    k.register_equipment_mana(binding(), state).unwrap();
    assert!(k.equipment_mana.players[&EntityId(1)].entry_cleanup);
    assert_eq!(
        k.read_player_snapshot(binding()).unwrap_err(),
        crate::CharacterRegistrationError::DurabilityPending
    );
    k.step_equipment_mana(0.).unwrap();
    assert!(!k.equipment_mana.players[&EntityId(1)].entry_cleanup);
    assert!(k.magic.registry(EntityId(1)).unwrap().entries().is_empty());
    assert!(
        k.social.events.is_empty(),
        "restart cleanup must not replay depletion notices"
    );
}
