use bace_storage_codec::*;
fn entity(id: u32) -> EntitySaveV1 {
    EntitySaveV1 {
        object_id: id,
        template_revision: 1,
        mutation_revision: 7,
        state: bace_content::WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "fixture".into(),
            weenie_type: 10,
            last_modified: None,
            properties: Default::default(),
        },
    }
}
fn player() -> PlayerSaveV1 {
    PlayerSaveV1 {
        entity: entity(0x50000001),
        account_id: 1,
        name: "Player".into(),
        metadata: Default::default(),
        quests: vec![],
    }
}
#[test]
fn migration_preserves_frozen_bytes_and_new_ui_cannot_downgrade() {
    let old = player().encode().unwrap();
    let mut current = PlayerSaveV3::decode_or_migrate(&old).unwrap();
    assert_eq!(current.player.encode().unwrap(), old);
    assert_eq!(current.ui.spellbook_filters, 0x3fff);
    current.ui.gameplay_options = (0..65536).map(|i| i as u8).collect();
    current.ui.shortcuts.push(ShortcutSaveV1 {
        index: 17,
        object_id: 0x80000001,
        spell_id: 1,
        layer: 2,
    });
    let bytes = current.encode().unwrap();
    assert_eq!(PlayerSaveV3::decode(&bytes).unwrap(), current);
    assert!(PlayerSaveV2::decode_or_migrate(&bytes).is_err());
    let mut corrupt = bytes;
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(PlayerSaveV3::decode(&corrupt).is_err());
    current.ui.gameplay_options.push(0);
    assert!(current.encode().is_err());
}
#[test]
fn old_items_require_real_placement_and_new_fields_are_bounded() {
    let old = entity(0x80000001).encode_item().unwrap();
    assert!(ItemSaveV3::decode_or_migrate(&old, None).is_err());
    let placement = ItemPlacementV2::Contained {
        container: 0x50000001,
        slot: 0,
        pack_slot: false,
        equipped: 0,
    };
    let current = ItemSaveV3::decode_or_migrate(&old, Some(placement.clone())).unwrap();
    assert_eq!(current.placement, placement);
    assert_eq!(current.entity.encode_item().unwrap(), old);
    assert!(current.enchantments.is_empty());
    let mut p = PlayerSaveV3::migrate_v1(player()).unwrap();
    p.ui.desired_components = vec![ComponentPreferenceV1 {
        template_id: 1,
        quantity: u32::MAX,
    }];
    assert!(p.encode().is_err());
    p.ui.desired_components = vec![
        ComponentPreferenceV1 {
            template_id: 1,
            quantity: 1
        };
        2
    ];
    assert!(p.encode().is_err());
}
