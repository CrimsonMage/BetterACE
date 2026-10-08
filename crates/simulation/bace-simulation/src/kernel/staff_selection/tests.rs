use super::*;
use bace_character::{CharacterProgression, ProgressionTables, RankTable};
use bace_gameplay_api::{
    SessionId,
    staff::{StaffPrivileges, StaffRegistration},
};
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_types::AccountId;
use std::sync::Arc;
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(7),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
fn fixture(current: u32) -> Kernel {
    let mut k = crate::synthetic_scenario(1, 2).unwrap();
    let rank = RankTable::new(&[0, 10]).unwrap();
    let c = context(1);
    let binding = CharacterBinding {
        actor: c.actor,
        session: c.session,
        account: c.account,
    };
    k.register_character(
        binding,
        CharacterProgression::new(
            &[],
            Arc::new(ProgressionTables {
                attributes: rank.clone(),
                vitals: rank.clone(),
                trained_skills: rank.clone(),
                specialized_skills: rank,
            }),
            0,
            0,
        )
        .unwrap(),
    )
    .unwrap();
    k.register_staff(StaffRegistration {
        binding,
        privileges: StaffPrivileges::default(),
    })
    .unwrap();
    let mut combatant = bace_entity::Combatant::new(bace_entity::CombatantProfile {
        maximum_health: 10,
        melee_damage: 1,
        melee_range: 1.,
        attack_duration: 1.,
        strike_offsets: vec![0.5],
        player: false,
    })
    .unwrap();
    combatant.damage(10 - current).unwrap();
    k.world.register_combatant(EntityId(2), combatant).unwrap();
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
        id: EntityId(3),
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
    k
}
#[test]
fn original_query_selection_and_response_vectors() {
    let rows: Vec<_> = include_str!("../../../tests/fixtures/target_query.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect();
    let mut checked = 0;
    for group in rows.chunks_exact(7) {
        let first: Vec<_> = group[0].split('|').collect();
        let maximum: i32 = first[1].parse().unwrap();
        // Authoritative Combatant admission forbids a zero maximum. The raw wire
        // fixture separately preserves source IEEE-754 zero-divisor payloads.
        if maximum == 0 {
            continue;
        }
        let current: u32 = first[0].parse().unwrap();
        let has_mana = first[2] == "True";
        let mut k = fixture(current);
        for (index, row) in group.iter().enumerate() {
            let f: Vec<_> = row.split('|').collect();
            let target = EntityId(f[4].parse().unwrap());
            let kind = if f[3] == "0" {
                TargetQueryKind::Health
            } else {
                TargetQueryKind::ItemMana
            };
            let mana = (kind == TargetQueryKind::ItemMana && target == EntityId(3)).then_some(
                PreparedItemManaQuery {
                    revision: 1,
                    current: has_mana.then_some(current as i32),
                    maximum: has_mana.then_some(maximum),
                },
            );
            k.query_staff_target(context(index as u32 + 1), kind, target, mana)
                .unwrap();
            let selected = k.staff.selections[&EntityId(1)];
            let value = |v: Option<EntityId>| v.map_or_else(|| "-".into(), |v| v.0.to_string());
            assert_eq!(value(selected.health), f[5]);
            assert_eq!(value(selected.mana), f[6]);
            let StaffEvent::TargetQuery(event) = k.take_staff_event().unwrap() else {
                panic!("query event")
            };
            let actual = match event.response {
                None => String::new(),
                Some(TargetQueryResponse::Health { target, fraction }) => {
                    format!("H:{}:{:08X}", target.0, fraction.to_bits())
                }
                Some(TargetQueryResponse::ItemMana {
                    target,
                    fraction,
                    success,
                }) => format!("M:{}:{:08X}:{}", target.0, fraction.to_bits(), success),
            };
            assert_eq!(actual, f[7], "{row}");
            checked += 1;
        }
    }
    assert_eq!(checked, 42);
}
#[test]
fn selected_target_updates_preserve_intermediate_pools_and_retire_on_clear() {
    let mut k = fixture(10);
    k.query_staff_target(context(1), TargetQueryKind::Health, EntityId(2), None)
        .unwrap();
    k.take_staff_event();
    k.world.damage_from(EntityId(2), EntityId(1), 2).unwrap();
    k.world.damage_from(EntityId(2), EntityId(1), 3).unwrap();
    k.query_staff_target(context(2), TargetQueryKind::Health, EntityId(0), None)
        .unwrap();
    k.take_staff_event();
    k.drain_health_observations();
    for row in include_str!("../../../tests/fixtures/health_updates.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let StaffEvent::TargetQuery(e) = k.take_staff_event().unwrap() else {
            panic!()
        };
        let Some(TargetQueryResponse::Health { target, fraction }) = e.response else {
            panic!()
        };
        assert_eq!(format!("U|H:{}:{:08X}", target.0, fraction.to_bits()), row);
    }
    k.world.damage_from(EntityId(2), EntityId(1), 1).unwrap();
    assert!(k.world.pending_health_observation().is_none());
}

#[test]
fn removed_target_keeps_frozen_notification_and_identity_without_blocking_body_retirement() {
    let mut k = fixture(10);
    k.query_staff_target(context(1), TargetQueryKind::Health, EntityId(2), None)
        .unwrap();
    k.take_staff_event();
    k.world.damage_from(EntityId(2), EntityId(1), 3).unwrap();
    assert!(!k.world.health_observation_pending_for(EntityId(2)));
    assert!(k.world.health_observation_pending_for(EntityId(1)));
    assert!(k.world.remove(EntityId(2)).is_some());
    assert!(k.world.contains_identity(EntityId(2)));
    k.drain_health_observations();
    assert!(!k.world.contains_identity(EntityId(2)));
    let StaffEvent::TargetQuery(e) = k.take_staff_event().unwrap() else {
        panic!()
    };
    assert_eq!(
        e.response,
        Some(TargetQueryResponse::Health {
            target: EntityId(2),
            fraction: 0.7
        })
    );
}
