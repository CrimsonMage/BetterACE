use bace_content::{Property, WeenieV1};
use bace_runtime::{
    equipment_effects::overlay_equipment_item_sources, game_inventory::FrozenInventoryItem,
};
use bace_simulation::{
    EquipmentEffectsPatch, EquipmentItemPropertyChange, ItemExperienceRegistryChange,
};
use bace_storage_codec::{EntitySaveV1, ItemPlacementV2};
use bace_types::EntityId;

fn item(id: u32) -> FrozenInventoryItem {
    let mut state = WeenieV1 {
        schema_version: 1,
        weenie_id: 100,
        class_name: "equipment_effect_test".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    state.properties.ints.push(Property { id: 107, value: 2 });
    state.properties.strings.push(Property {
        id: 999,
        value: "preserved".into(),
    });
    FrozenInventoryItem {
        corpse: None,
        construction: None,
        source_destination: None,
        entity: EntitySaveV1 {
            object_id: id,
            template_revision: 7,
            mutation_revision: 5,
            state,
        },
        enchantments: vec![],
        persisted_version: 3,
        placement: Some(ItemPlacementV2::Contained {
            container: 1,
            slot: 0,
            pack_slot: false,
            equipped: 1,
        }),
    }
}
fn patch() -> EquipmentEffectsPatch {
    EquipmentEffectsPatch {
        mana: Vec::new(),
        actor: EntityId(1),
        before_revision: 8,
        registries: vec![],
        item_experience: vec![],
        properties: vec![EquipmentItemPropertyChange {
            item: EntityId(2),
            before_revision: 5,
            after_revision: 6,
            mana_before: Some(2),
            mana_after: Some(1),
            affecting_before: None,
            affecting_after: Some(true),
        }],
        minimum_vital_maxima: None,
        activation_messages: vec![],
    }
}
#[test]
fn equipment_source_overlay_preserves_unrelated_state_and_does_not_advance_revision_twice() {
    let mut items = vec![item(2)];
    overlay_equipment_item_sources(&mut items, &patch()).unwrap();
    let saved = &items[0];
    assert_eq!(saved.entity.mutation_revision, 5);
    assert_eq!(saved.entity.template_revision, 7);
    assert_eq!(saved.entity.state.properties.ints[0].value, 1);
    assert_eq!(
        saved.entity.state.properties.bools[0],
        Property {
            id: 56,
            value: true
        }
    );
    assert_eq!(saved.entity.state.properties.strings[0].value, "preserved");
}
#[test]
fn stale_later_property_and_missing_registry_do_not_partially_overlay_earlier_items() {
    let original = vec![item(2), item(3)];
    let mut candidate = original.clone();
    let mut change = patch();
    let mut stale = change.properties[0].clone();
    stale.item = EntityId(3);
    stale.mana_before = Some(99);
    change.properties.push(stale);
    assert!(overlay_equipment_item_sources(&mut candidate, &change).is_err());
    assert_unchanged(&candidate, &original);
    change.properties.pop();
    change.registries.push(ItemExperienceRegistryChange {
        actor: EntityId(4),
        capacity: 4,
        before_revision: 0,
        before: vec![],
        after_revision: 1,
        after: vec![],
        events: vec![],
    });
    assert!(overlay_equipment_item_sources(&mut candidate, &change).is_err());
    assert_unchanged(&candidate, &original);
}
#[test]
fn duplicate_property_change_is_rejected_before_mutation() {
    let mut items = vec![item(2)];
    let original = items.clone();
    let mut change = patch();
    change.properties.push(change.properties[0].clone());
    assert!(overlay_equipment_item_sources(&mut items, &change).is_err());
    assert_unchanged(&items, &original);
}

fn assert_unchanged(actual: &[FrozenInventoryItem], expected: &[FrozenInventoryItem]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.entity, expected.entity);
        assert_eq!(actual.enchantments, expected.enchantments);
        assert_eq!(actual.placement, expected.placement);
        assert_eq!(actual.persisted_version, expected.persisted_version);
    }
}

#[test]
fn cold_reconstruction_resets_only_source_transient_mana_fields() {
    let mut item = item(2).entity.state;
    item.properties.ints.push(Property {
        id: 108,
        value: 100,
    });
    item.properties.floats.push(Property { id: 5, value: -0.2 });
    item.properties.bools.push(Property {
        id: 56,
        value: true,
    });
    let spells = bace_dat::SpellTable {
        spells: Default::default(),
        sets: Default::default(),
    };
    let actor = WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "player".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let restored = bace_runtime::equipment_mana::prepare_equipment_mana(
        EntityId(1),
        &actor,
        &[(EntityId(2), &item)],
        &spells,
    )
    .unwrap();
    assert!(restored.fresh);
    assert_eq!(restored.heartbeat, 5.);
    assert_eq!(restored.items[0].current, Some(2));
    assert_eq!(restored.items[0].affecting, Some(true));
    assert_eq!(restored.items[0].accumulator, 0.);
    assert!(!restored.items[0].warned);
    assert_eq!(restored.items[0].removal_remaining, None);
}

#[test]
fn zero_mana_restart_cleanup_selects_only_matching_known_item_caster() {
    let mut player = bace_magic::EnchantmentRegistry::new(8).unwrap();
    let entry = |spell, caster| bace_magic::EnchantmentEntry {
        spell,
        caster,
        school: bace_magic::MagicSchool::Creature,
        spec: bace_magic::EnchantmentSpec {
            category: spell as u16,
            power: 100,
            duration: 60.,
            layer: 1,
            stat_type: 4 | 0x8000,
            stat_key: 1,
            value: 1.,
            beneficial: true,
            set_id: None,
        },
        start_time: 0.,
        is_set_spell: false,
        is_level8_aura: false,
        metadata: Default::default(),
    };
    player.add(entry(1, 2), 0., true).unwrap();
    player.add(entry(2, 3), 0., true).unwrap();
    let mut state = bace_simulation::EquipmentManaRecovery {
        fresh: true,
        heartbeat: 5.,
        rating: 0,
        heartbeat_remaining: 5.,
        items: vec![bace_simulation::EquipmentManaItem {
            item: EntityId(2),
            name: "zero".into(),
            current: Some(0),
            maximum: Some(10),
            rate: Some(-0.1),
            affecting: None,
            removals: vec![(EntityId(1), 1)],
            accumulator: 0.,
            warned: false,
            removal_remaining: None,
        }],
    };
    assert_eq!(
        bace_runtime::equipment_mana::prepare_equipment_mana_cleanup(
            EntityId(1),
            &mut state,
            &player,
            &[]
        )
        .unwrap(),
        1
    );
    assert_eq!(state.items[0].removal_remaining, Some(0.));
    assert_eq!(
        player.entries().len(),
        2,
        "cold preparation does not adopt mutations"
    );
    state.items[0].removal_remaining = None;
    state.items[0].current = Some(1);
    assert_eq!(
        bace_runtime::equipment_mana::prepare_equipment_mana_cleanup(
            EntityId(1),
            &mut state,
            &player,
            &[]
        )
        .unwrap(),
        0
    );
    state.items[0].current = Some(0);
    state.items[0].removals = vec![(EntityId(1), 2)];
    assert_eq!(
        bace_runtime::equipment_mana::prepare_equipment_mana_cleanup(
            EntityId(1),
            &mut state,
            &player,
            &[]
        )
        .unwrap(),
        0,
        "another caster remains untouched"
    );
}
