use bace_magic::MagicSchool;
use bace_runtime::enchantment_saves::*;
use bace_storage_codec::*;

fn definition() -> EnchantmentDefinition {
    EnchantmentDefinition {
        school: MagicSchool::Creature,
        is_set_spell: true,
        is_level8_aura: false,
        category: 23,
        power: 99,
        degrade_modifier: 0.1,
        degrade_limit: 0.2,
        stat_type: 0x8000,
        stat_key: 7,
        beneficial: true,
    }
}
fn row() -> FrozenEnchantmentV1 {
    FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 12,
        spell_id: 101,
        layer_id: 7,
        has_spell_set_id: false,
        spell_category: 99,
        power_level: 1,
        start_time: -15.0,
        duration: 60.0,
        caster_object_id: 0x50000001,
        degrade_modifier: 9.0,
        degrade_limit: 8.0,
        last_time_degraded: -7.0,
        stat_mod_type: 0,
        stat_mod_key: 88,
        stat_mod_value: 12.5,
        spell_set_id: 42,
    }
}
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 5,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "fixture".into(),
            weenie_type: 1,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn item() -> ItemSaveV5 {
    ItemSaveV5 {
        previous: ItemSaveV4 {
            previous: bace_storage_codec::ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: entity(0x80000001),
                    placement: ItemPlacementV2::Removed,
                },
                enchantments: vec![row()],
            },
            construction: None,
        },
        source_destination: Some(2),
    }
}
fn fixture(name: &str) -> Vec<u8> {
    let hex = include_str!("../../../network/bace-replication/tests/fixtures/enchantments.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            line.split_once(',')
                .filter(|(key, _)| *key == name)
                .map(|(_, hex)| hex)
        })
        .unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn frozen_rows_preserve_nonwire_metadata_and_absent_set_flag() {
    let saved = row();
    let runtime = restore_enchantment(&saved, Some(definition())).unwrap();
    assert_eq!(freeze_enchantment(&runtime).unwrap(), saved);
    assert_eq!(runtime.spec.set_id, None);
    let projection = prepare_enchantment_projection(&runtime, Some(definition())).unwrap();
    assert_eq!(projection.category, 23);
    assert_eq!(projection.power, 99);
    assert_eq!(projection.stat_key, 7);
    assert_eq!(projection.degrade_modifier, 0.1);
    assert_eq!(projection.last_time_degraded, 0.0);
    assert_eq!(projection.spell_set_id, 42);
}

#[test]
fn stored_mixed_registry_projects_identically_to_original_ace_constructors() {
    let additive = row();
    let mut mult = row();
    mult.spell_id = 100;
    mult.layer_id = 3;
    mult.stat_mod_value = 0.75;
    let mut cooldown = row();
    cooldown.spell_id = 0x8001;
    cooldown.layer_id = 2;
    cooldown.spell_category = 0x8000;
    cooldown.degrade_modifier = 0.3;
    cooldown.degrade_limit = 0.4;
    cooldown.last_time_degraded = -2.0;
    cooldown.stat_mod_type = 0x1000000;
    cooldown.stat_mod_key = 22;
    cooldown.stat_mod_value = 35.0;
    let mut vitae = row();
    vitae.spell_id = 666;
    vitae.layer_id = 1;
    vitae.duration = -1.0;
    vitae.stat_mod_value = 0.95;
    let saved = [additive, cooldown, vitae, mult];
    let mut projected = Vec::new();
    for entry in &saved {
        let mut def = definition();
        if entry.spell_id == 100 {
            def.stat_type = 0x4000;
        }
        let live = restore_enchantment(entry, Some(def)).unwrap();
        assert_eq!(freeze_enchantment(&live).unwrap(), *entry);
        projected.push(prepare_enchantment_projection(&live, Some(def)).unwrap());
    }
    assert_eq!(
        bace_replication::project_enchantments(&projected, 16)
            .unwrap()
            .encode(16, 4096)
            .unwrap(),
        fixture("mixed")
    );
}

fn advance(saved: &mut impl SavedEnchantments) {
    let mut registry = restore_saved_enchantments(saved, 16, |_| Some(definition())).unwrap();
    let mut removed = Vec::with_capacity(16);
    registry.heartbeat(5.0, &mut removed).unwrap();
    assert!(snapshot_enchantments(saved, &registry, 5).is_err());
    snapshot_enchantments(saved, &registry, 6).unwrap();
    assert_eq!(saved.enchantments()[0].start_time, -20.0);
    assert_eq!(saved.revision(), 6);
}

#[test]
fn every_persistent_aggregate_saves_and_loads_the_same_registry() {
    let mut item = item();
    advance(&mut item);
    assert_eq!(ItemSaveV5::decode(&item.encode().unwrap()).unwrap(), item);
    assert_eq!(item.source_destination, Some(2));
    let mut player = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: entity(0x50000001),
            account_id: 1,
            name: "Fixture Player".into(),
            metadata: Default::default(),
            quests: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    player.previous.enchantments = vec![row()];
    advance(&mut player);
    assert_eq!(
        PlayerSaveV6::decode(&player.encode().unwrap()).unwrap(),
        player
    );
    let mut corpse = CorpseSaveV3 {
        previous: CorpseSaveV2 {
            corpse: CorpseSaveV1 {
                entity: entity(0x80000002),
                owner: Some(0x50000001),
                death_operation: "death-fixture".into(),
                expires_at: 100,
            },
            placement: ItemPlacementV2::Removed,
        },
        enchantments: vec![row()],
    };
    advance(&mut corpse);
    assert_eq!(
        CorpseSaveV3::decode(&corpse.encode().unwrap()).unwrap(),
        corpse
    );
    let mut house = HouseSaveV3::migrate_v2(
        HouseSaveV2::migrate_v1(HouseSaveV1 {
            entity: entity(0x80000003),
            house_id: 1,
            owner_id: 0x50000001,
            purchased_at: 0,
            rent_period_start: 0,
            rent_due_at: 100,
            access: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    house.enchantments = vec![row()];
    advance(&mut house);
    assert_eq!(
        HouseSaveV3::decode(&house.encode().unwrap()).unwrap(),
        house
    );
}

#[test]
fn missing_spell_and_capacity_failure_leave_persisted_input_intact() {
    let saved = item();
    let before = saved.clone();
    assert!(matches!(
        restore_saved_enchantments(&saved, 16, |_| None),
        Err(EnchantmentSaveError::MissingSpell(101))
    ));
    assert!(restore_saved_enchantments(&saved, 0, |_| Some(definition())).is_err());
    assert_eq!(saved, before);
    let mut cooldown = row();
    cooldown.spell_id = 0x8001;
    cooldown.spell_category = 0x8000;
    assert_eq!(
        freeze_enchantment(&restore_enchantment(&cooldown, None).unwrap()).unwrap(),
        cooldown
    );
}
