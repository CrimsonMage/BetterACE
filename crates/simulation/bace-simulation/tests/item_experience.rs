use bace_gameplay_api::{CharacterBinding, InventoryRejection as Error, SessionId};
use bace_inventory::*;
use bace_simulation::Kernel;
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
            equipped: 1,
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
fn prepared(id: u32) -> bace_simulation::PreparedItemExperience {
    bace_simulation::PreparedItemExperience {
        item: EntityId(id),
        actor: EntityId(1),
        name: "Aetheria".into(),
        experience: Some(bace_character::ItemExperience {
            total: 9,
            base: 10,
            maximum_level: 3,
            style: bace_character::ItemExperienceStyle::Fixed,
            revision: 1,
        }),
        set: None,
        set_uses_item_levels: true,
        equipment_order: u64::from(id),
    }
}
#[test]
fn item_xp_reserves_exact_old_state_and_adopts_only_matching_receipt() {
    let mut k = kernel();
    let id = 0x80000001;
    k.register_inventory_item(item(id, 0)).unwrap();
    assert_eq!(
        k.prepare_item_experience(EntityId(1), 5),
        Err(Error::InvalidState)
    );
    k.register_item_experience(prepared(id)).unwrap();
    let reward = k.prepare_item_experience(EntityId(1), 5).unwrap();
    assert_eq!(reward.changes[0].1.after.total, 14);
    let ticket = k.reserve_item_experience(&reward, &[]).unwrap().unwrap();
    assert_eq!(
        k.item_experience(EntityId(id))
            .unwrap()
            .experience
            .unwrap()
            .total,
        9
    );
    assert_eq!(k.inventory_item(EntityId(id)).unwrap().revision, 1);
    assert!(k.take_item_experience_event().is_none());
    assert!(k.take_item_experience(EntityId(id)).is_err());
    let mut wrong = ticket.clone();
    wrong.proposal.changes[0].after.revision += 1;
    assert!(k.adopt_item_experience(&reward, Some(&wrong)).is_err());
    k.reject_item_experience(Some(&ticket)).unwrap();
    assert_eq!(k.prepare_item_experience(EntityId(1), 5).unwrap(), reward);
    let ticket = k.reserve_item_experience(&reward, &[]).unwrap().unwrap();
    k.validate_item_experience(&reward, Some(&ticket)).unwrap();
    k.adopt_item_experience(&reward, Some(&ticket)).unwrap();
    assert_eq!(
        k.item_experience(EntityId(id))
            .unwrap()
            .experience
            .unwrap()
            .total,
        14
    );
    assert_eq!(k.inventory_item(EntityId(id)).unwrap().revision, 2);
    let event = k.take_item_experience_event().unwrap();
    assert_eq!(event.total, 14);
    assert_eq!(event.level_up, Some(("Aetheria".into(), 1)));
    assert!(k.adopt_item_experience(&reward, Some(&ticket)).is_err());
    let capped = k.prepare_item_experience(EntityId(1), 100).unwrap();
    assert_eq!(capped.changes[0].1.after.total, 30);
}
#[test]
fn known_nonlevelable_equipment_does_not_block_or_invent_item_xp() {
    let mut k = kernel();
    let id = 0x80000001;
    k.register_inventory_item(item(id, 0)).unwrap();
    let mut p = prepared(id);
    p.experience = None;
    k.register_item_experience(p).unwrap();
    let reward = k.prepare_item_experience(EntityId(1), 100).unwrap();
    assert!(reward.changes.is_empty());
    assert!(k.reserve_item_experience(&reward, &[]).unwrap().is_none());
    k.adopt_item_experience(&reward, None).unwrap();
    assert_eq!(k.inventory_item(EntityId(id)).unwrap().revision, 1);
}
fn set_entry(spell: u32, caster: u32) -> bace_magic::EnchantmentEntry {
    bace_magic::EnchantmentEntry {
        spell,
        caster,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category: 1,
            power: spell,
            duration: 30.,
            layer: 0,
            stat_type: 0,
            stat_key: 0,
            value: 1.,
            beneficial: true,
            set_id: Some(1),
        },
        start_time: 0.,
        is_set_spell: true,
        is_level8_aura: false,
        metadata: bace_magic::EnchantmentMetadata {
            has_spell_set_id: true,
            spell_set_id: 1,
            ..Default::default()
        },
    }
}
#[test]
fn level_crossing_replaces_set_spells_in_same_reserved_reward() {
    let mut k = kernel();
    let id = 0x80000001;
    k.register_inventory_item(item(id, 0)).unwrap();
    let mut registry = bace_magic::EnchantmentRegistry::new(8).unwrap();
    registry.add(set_entry(100, id), 0., true).unwrap();
    k.register_magic_registry(EntityId(1), registry, true)
        .unwrap();
    let mut p = prepared(id);
    p.set = Some(bace_simulation::PreparedItemSet {
        id: 1,
        tiers: std::collections::BTreeMap::from([
            (
                0,
                vec![bace_simulation::PreparedGeneratorEnchantment {
                    target: EntityId(1),
                    entry: set_entry(100, id),
                }],
            ),
            (
                1,
                vec![bace_simulation::PreparedGeneratorEnchantment {
                    target: EntityId(1),
                    entry: set_entry(101, id),
                }],
            ),
        ]),
    });
    k.register_item_experience(p).unwrap();
    let reward = k.prepare_item_experience(EntityId(1), 1).unwrap();
    assert_eq!(reward.registries.len(), 1);
    assert_eq!(reward.registries[0].after[0].spell, 101);
    let ticket = k.reserve_item_experience(&reward, &[]).unwrap().unwrap();
    assert_eq!(
        k.magic_registry(EntityId(1)).unwrap().entries()[0].spell,
        100
    );
    k.adopt_item_experience(&reward, Some(&ticket)).unwrap();
    let entries = k.magic_registry(EntityId(1)).unwrap().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].spell, 101);
    assert_eq!(entries[0].caster, id);
    assert!(entries[0].spec.duration < 0.);
    assert!(matches!(
        k.take_magic_event(),
        Some(bace_simulation::MagicEvent::EnchantmentsRemoved { .. })
    ));
    assert!(matches!(
        k.take_magic_event(),
        Some(bace_simulation::MagicEvent::Enchantment { .. })
    ));
}
