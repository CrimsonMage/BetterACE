use bace_gameplay_api::{CharacterUi, UiShortcut};
use bace_persistence::{DirtySaves, SaveAck, SaveSnapshot};
use bace_runtime::ui_saves::{freeze_ui, project_ui, restore_ui};
use bace_storage_codec::*;
use std::time::Duration;

fn player() -> PlayerSaveV6 {
    let mut saved = PlayerSaveV6::migrate_v2(
        PlayerSaveV2::migrate_v1(PlayerSaveV1 {
            entity: EntitySaveV1 {
                object_id: 0x50000001,
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
            },
            account_id: 1,
            name: "UI Fixture".into(),
            metadata: Default::default(),
            quests: vec![QuestSaveV1 {
                name: "quest".into(),
                completions: 2,
                last_completed: 20,
            }],
        })
        .unwrap(),
    )
    .unwrap();
    saved.previous.rares = Some(RareStateV1 {
        character: 0x50000001,
        random_identity: [7; 16],
        key_version: 1,
        attempt_ordinal: 39,
        timer_ordinal: 9,
        next_realtime_at: Some(500),
        last_effective_time: 400,
    });
    saved.previous.enchantments = vec![FrozenEnchantmentV1 {
        schema_version: 1,
        enchantment_category: 12,
        spell_id: 123,
        layer_id: 7,
        has_spell_set_id: false,
        spell_category: 23,
        power_level: 100,
        start_time: -15.0,
        duration: 60.0,
        caster_object_id: 8,
        degrade_modifier: 0.1,
        degrade_limit: 0.2,
        last_time_degraded: -3.0,
        stat_mod_type: 0x2008000,
        stat_mod_key: 7,
        stat_mod_value: 12.5,
        spell_set_id: 42,
    }];
    saved
}
fn ui() -> CharacterUi {
    let mut value = CharacterUi {
        options1: 0xaabbccdd,
        options2: 0xdeadbeef,
        filters: 0x1234,
        shortcuts: vec![UiShortcut {
            index: 17,
            object: 0xf0100203,
            spell: 100,
            layer: 8,
        }],
        components: vec![(5001, 40), (5002, 60)],
        gameplay: vec![0, 255, 0, 128],
        ..Default::default()
    };
    value.bars[0] = vec![201, 101];
    value.bars[7] = vec![123];
    value
}
fn snapshot(value: &PlayerSaveV6, version: i64) -> SaveSnapshot {
    SaveSnapshot {
        object_id: value.player.entity.object_id,
        mutation_revision: value.player.entity.mutation_revision,
        expected_version: version,
        bytes: value.encode().unwrap(),
    }
}

#[test]
fn ui_edit_saves_all_eight_bars_and_preserves_rares_enchantments_and_quests() {
    let saved = player();
    let ui = ui();
    let next = freeze_ui(&saved, &ui, 6).unwrap();
    assert_eq!(next.previous.rares, saved.previous.rares);
    assert_eq!(next.previous.enchantments, saved.previous.enchantments);
    assert_eq!(next.player.quests, saved.player.quests);
    assert_eq!(saved.player.entity.mutation_revision, 5);
    let loaded = PlayerSaveV6::decode(&next.encode().unwrap()).unwrap();
    assert_eq!(restore_ui(&loaded).unwrap(), ui);
    assert_eq!(
        loaded
            .player
            .metadata
            .spell_favorites
            .iter()
            .map(|s| (s.spell_id, s.bar, s.position))
            .collect::<Vec<_>>(),
        [(201, 1, 1), (101, 1, 2), (123, 8, 1)]
    );
    let mut description = bace_wire::PlayerDescription {
        known_spells: vec![101, 123, 201],
        enchantments: Some(Default::default()),
        ..Default::default()
    };
    project_ui(&ui, &mut description).unwrap();
    assert_eq!(description.spell_bars, ui.bars);
    assert_eq!(description.gameplay_options, ui.gameplay);
    assert_eq!(description.known_spells, [101, 123, 201]);
    assert!(description.enchantments.is_some());
    assert_eq!(description.shortcuts[0].layer, 8);
}

#[test]
fn invalid_ui_or_stale_revision_leaves_original_save_untouched() {
    let saved = player();
    let before = saved.clone();
    let mut invalid = ui();
    invalid.bars[0] = vec![123, 123];
    assert!(freeze_ui(&saved, &invalid, 6).is_err());
    assert!(freeze_ui(&saved, &ui(), 5).is_err());
    assert_eq!(saved, before);
    let mut gap = player();
    gap.player.metadata.spell_favorites = vec![SpellFavoriteV1 {
        spell_id: 123,
        bar: 1,
        position: 2,
    }];
    assert!(restore_ui(&gap).is_err());
}

#[test]
fn older_ui_save_acknowledgment_cannot_clear_a_newer_edit() {
    let original = player();
    let first = freeze_ui(&original, &ui(), 6).unwrap();
    let mut dirty = DirtySaves::new(1);
    dirty.mark_at(snapshot(&first, 1), Duration::ZERO).unwrap();
    let sent = dirty.due(Duration::from_secs(5));
    assert_eq!(sent.len(), 1);
    let mut updated = ui();
    updated.gameplay.push(99);
    let second = freeze_ui(&first, &updated, 7).unwrap();
    dirty
        .mark_at(snapshot(&second, 1), Duration::from_secs(6))
        .unwrap();
    dirty
        .acknowledge(&SaveAck {
            object_id: first.player.entity.object_id,
            mutation_revision: 6,
            persisted_version: 2,
        })
        .unwrap();
    assert!(!dirty.is_clean());
    assert_eq!(
        dirty.dirty_since(first.player.entity.object_id),
        Some(Duration::from_secs(6))
    );
    let due = dirty.due(Duration::from_secs(11));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].mutation_revision, 7);
    assert_eq!(due[0].expected_version, 2);
    let loaded = PlayerSaveV6::decode(&due[0].bytes).unwrap();
    assert_eq!(restore_ui(&loaded).unwrap(), updated);
    assert_eq!(loaded.previous.rares, original.previous.rares);
    assert_eq!(loaded.previous.enchantments, original.previous.enchantments);
    dirty.failed(&due[0]).unwrap();
    assert!(!dirty.is_clean());
    assert_eq!(dirty.drain_ready()[0].bytes, due[0].bytes);
}
