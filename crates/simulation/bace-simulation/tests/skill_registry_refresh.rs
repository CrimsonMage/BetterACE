//! Public-owner regressions: pending durable work cannot release its clock, and
//! live Dirty Fighting adds only to Current until the authoritative expiry tick.
use bace_character::*;
use bace_gameplay_api::*;
use bace_inventory::*;
use bace_magic::{
    EnchantmentEntry, EnchantmentMetadata, EnchantmentRegistry, EnchantmentSpec, MagicSchool,
};
use bace_simulation::*;
use bace_types::{AccountId, EntityId};
use std::sync::Arc;
fn context(sequence: u32) -> ActionContext {
    ActionContext {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(1),
        sequence,
    }
}
fn setup() -> Kernel {
    let mut k = synthetic_scenario(1, 0).unwrap();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let ids = [6, 21, 44, 48];
    let states: Vec<_> = ids
        .into_iter()
        .map(|skill| TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(skill),
                advancement: SkillAdvancement::Trained,
                experience_spent: 0,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        })
        .collect();
    let costs: Vec<_> = ids
        .into_iter()
        .map(|skill| SkillCosts {
            skill,
            trained_cost: 4,
            specialized_cost: 6,
        })
        .collect();
    let character = CharacterProgression::with_state(
        &states,
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        1000,
        0,
    )
    .unwrap()
    .with_training(Arc::new(SkillTrainingRules::new(&costs).unwrap()), 20, &[])
    .unwrap();
    k.register_character(
        CharacterBinding {
            actor: EntityId(1),
            account: AccountId(1),
            session: SessionId(1),
        },
        character,
    )
    .unwrap();
    let input = SkillValueInputs {
        formula: VitalFormula {
            enabled: true,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [100; 6],
        current_attributes: [100; 6],
        bonuses: SkillBonuses::default(),
        multiplier: 1.0,
        vitae: 1.0,
        additive: 0,
    };
    k.register_character_skill_inputs(
        EntityId(1),
        PreparedCharacterSkillInputs {
            attack_skill: 44,
            inputs: ids.into_iter().map(|id| (id, input)).collect(),
            shield: None,
            attribute_modifiers: [PreparedAttributeModifier::default(); 6],
        },
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
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    })
    .unwrap();
    k
}
fn registry(duration: f64) -> EnchantmentRegistry {
    EnchantmentRegistry::restore(
        8,
        0,
        [(5938, 684, 0x18010), (5940, 686, 0x28010)]
            .into_iter()
            .map(|(spell, category, flags)| EnchantmentEntry {
                spell,
                caster: 2,
                school: MagicSchool::Creature,
                spec: EnchantmentSpec {
                    category,
                    power: 100,
                    duration,
                    layer: 1,
                    stat_type: flags,
                    stat_key: 0,
                    value: -20.0,
                    beneficial: false,
                    set_id: None,
                },
                start_time: 0.0,
                is_set_spell: false,
                is_level8_aura: false,
                metadata: EnchantmentMetadata::default(),
            })
            .collect(),
    )
    .unwrap()
}
fn current(k: &Kernel, skill: u32) -> u32 {
    k.character_skill_projection(EntityId(1), skill)
        .unwrap()
        .1
        .current
}
#[test]
fn general_skill_buffs_compose_once_with_dirty_fighting_and_expire() {
    let mut k = setup();
    let mut entries = registry(5.0).entries().to_vec();
    let mut buff = entries[0].clone();
    buff.spell = 1;
    buff.spec.category = 1;
    buff.spec.stat_type = 0x9010; // SingleStat | Additive | Skill
    buff.spec.stat_key = 21;
    buff.spec.value = 20.0;
    buff.spec.beneficial = true;
    entries.push(buff.clone());
    buff.spell = 2;
    buff.spec.category = 2;
    buff.spec.stat_type = 0x5010; // SingleStat | Multiplicative | Skill
    buff.spec.value = 1.5;
    entries.push(buff);
    k.register_magic_registry(
        EntityId(1),
        EnchantmentRegistry::restore(8, 0, entries).unwrap(),
        true,
    )
    .unwrap();
    k.step().unwrap();
    assert_eq!(current(&k, 21), 170);
    assert_eq!(current(&k, 44), 80);
    assert_eq!(
        k.character_skill_projection(EntityId(1), 21)
            .unwrap()
            .1
            .base,
        100
    );
    for _ in 0..10 {
        k.refresh_character_skills(EntityId(1)).unwrap();
        k.refresh_changed_magic_skills().unwrap();
    }
    assert_eq!(current(&k, 21), 170);
    ticks(&mut k, 149);
    assert_eq!(current(&k, 21), 100);
    assert_eq!(current(&k, 44), 100);
}
fn ticks(k: &mut Kernel, count: usize) {
    for _ in 0..count {
        k.step().unwrap();
    }
}
#[test]
fn live_debuff_refreshes_current_once_and_expiry_restores_it_on_same_completed_tick() {
    let mut k = setup();
    assert_eq!(current(&k, 44), 100);
    // Adding the owner after skill admission is a live change, not a prepared
    // input which already baked the modifier into its initial projection.
    k.register_magic_registry(EntityId(1), registry(5.0), true)
        .unwrap();
    k.step().unwrap();
    for skill in [6, 44, 48] {
        let value = k.character_skill_projection(EntityId(1), skill).unwrap().1;
        assert_eq!(value.base, 100);
        assert_eq!(value.current, 80);
    }
    assert_eq!(current(&k, 21), 100);
    k.refresh_character_skills(EntityId(1)).unwrap();
    k.refresh_changed_magic_skills().unwrap();
    assert_eq!(current(&k, 44), 80);
    ticks(&mut k, 148);
    assert_eq!(current(&k, 44), 80);
    k.step().unwrap(); // 150 / 30 Hz = first five-second heartbeat.
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
    for skill in [6, 21, 44, 48] {
        assert_eq!(current(&k, skill), 100);
    }
    k.step().unwrap();
    assert_eq!(current(&k, 44), 100);
}
#[test]
fn public_release_cannot_unfreeze_character_valuable_operation() {
    let mut k = setup();
    k.register_magic_registry(EntityId(1), registry(5.0), true)
        .unwrap();
    let ticket = k
        .propose_skill(context(1), SkillIntent::Specialize(6))
        .unwrap();
    assert_eq!(
        k.reserve_magic_registry(EntityId(1), false),
        Err(CastRejection::Busy)
    );
    ticks(&mut k, 300);
    assert_eq!(
        k.magic_registry(EntityId(1)).unwrap().entries()[0].start_time,
        0.0
    );
    assert_eq!(
        k.character(EntityId(1)).unwrap().revision(),
        ticket.expected_revision
    );
    k.reject_skill(ticket).unwrap();
    k.reserve_magic_registry(EntityId(1), false).unwrap();
    k.step().unwrap();
    assert!(k.magic_registry(EntityId(1)).unwrap().entries().is_empty());
    assert_eq!(
        k.character(EntityId(1))
            .unwrap()
            .projection(ProgressionTarget::Skill(6))
            .unwrap()
            .advancement,
        SkillAdvancement::Trained
    );
}
#[test]
fn public_release_cannot_unfreeze_inventory_actor_or_consumed_item() {
    let mut k = setup();
    for id in [1, 10] {
        k.register_magic_registry(EntityId(id), registry(5.0), true)
            .unwrap();
    }
    let operation = k.propose_item_take(EntityId(1), EntityId(10), 1).unwrap();
    for id in [1, 10] {
        assert_eq!(
            k.reserve_magic_registry(EntityId(id), false),
            Err(CastRejection::Busy)
        );
    }
    ticks(&mut k, 300);
    for id in [1, 10] {
        assert_eq!(
            k.magic_registry(EntityId(id)).unwrap().entries()[0].start_time,
            0.0
        );
    }
    assert_eq!(k.inventory_item(EntityId(10)).unwrap().revision, 1);
    k.reject_inventory(operation).unwrap();
    k.step().unwrap();
    for id in [1, 10] {
        assert!(k.magic_registry(EntityId(id)).unwrap().entries().is_empty());
    }
    assert!(k.inventory_item(EntityId(10)).is_some());
}
