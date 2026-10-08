use bace_storage_codec::*;
fn v1() -> PlayerSaveV1 {
    PlayerSaveV1 {
        entity: EntitySaveV1 {
            object_id: 0x50000001,
            template_revision: 9,
            mutation_revision: 7,
            state: bace_content::WeenieV1 {
                schema_version: 1,
                weenie_id: 1,
                class_name: "test".into(),
                weenie_type: 10,
                last_modified: None,
                properties: Default::default(),
            },
        },
        account_id: 1,
        name: "Player".into(),
        metadata: Default::default(),
        quests: vec![QuestSaveV1 {
            name: "quest".into(),
            completions: 3,
            last_completed: 100,
        }],
    }
}
#[test]
fn explicit_player_v1_migration_preserves_original_bytes_and_does_not_invent_rares() {
    let before = v1().encode().unwrap();
    let migrated = PlayerSaveV2::decode_or_migrate(&before).unwrap();
    assert_eq!(migrated.player.encode().unwrap(), before);
    assert!(migrated.rares.is_none());
    assert!(migrated.enchantments.is_empty());
    let bytes = migrated.encode().unwrap();
    assert_eq!(
        inspect(&bytes, Default::default()).unwrap().schema_version,
        2
    );
    assert!(PlayerSaveV1::decode(&bytes).is_err());
    assert_eq!(PlayerSaveV2::decode(&bytes).unwrap(), migrated);
}
#[test]
fn rare_identity_and_enchantment_registry_survive_reconnect_without_wall_clock_changes() {
    let mut player = PlayerSaveV2::migrate_v1(v1()).unwrap();
    player.rares = Some(RareStateV1 {
        character: 0x50000001,
        random_identity: [4; 16],
        key_version: 1,
        attempt_ordinal: 77,
        timer_ordinal: 5,
        next_realtime_at: Some(9000),
        last_effective_time: 1200,
    });
    player.enchantments.push(FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 0,
        spell_id: 10,
        layer_id: 1,
        has_spell_set_id: false,
        spell_category: 3,
        power_level: 100,
        start_time: -10.0,
        duration: 30.0,
        caster_object_id: 0x50000001,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        stat_mod_type: 1,
        stat_mod_key: 1,
        stat_mod_value: 20.0,
        spell_set_id: 0,
    });
    assert_eq!(
        PlayerSaveV2::decode(&player.encode().unwrap()).unwrap(),
        player
    );
    player.rares.as_mut().unwrap().character = 0x50000002;
    assert!(player.encode().is_err());
    player.rares.as_mut().unwrap().character = 0x50000001;
    player.enchantments.push(player.enchantments[0].clone());
    assert!(player.encode().is_err());
}
