use bace_wire::*;
fn limits() -> PlayerDescriptionLimits {
    PlayerDescriptionLimits {
        max_table_entries: 4,
        max_string_bytes: 16,
        max_gameplay_options_bytes: 8,
        max_message_bytes: 1024,
    }
}
#[test]
fn description_rejects_unsupported_registry_duplicates_and_all_nested_limits() {
    let mut p = PlayerDescription {
        has_enchantments: true,
        ..Default::default()
    };
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::InvalidEncoding));
    p.has_enchantments = false;
    p.integers = vec![(1, 1), (1, 2)];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::InvalidEncoding));
    p.integers = vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::LimitExceeded));
    p.integers.clear();
    p.strings = vec![(1, "01234567890123456".into())];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::LimitExceeded));
    p.strings.clear();
    p.known_spells = vec![1, 1];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::InvalidEncoding));
    p.known_spells.clear();
    p.spell_bars[7] = vec![1; 5];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::LimitExceeded));
    p.spell_bars[7].clear();
    p.gameplay_options = vec![0; 9];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::LimitExceeded));
    p.gameplay_options.clear();
    p.inventory = vec![
        ContainerEntry {
            object_id: 1,
            container_type: 0
        };
        5
    ];
    assert_eq!(p.encode(1, 0, limits()), Err(WireError::LimitExceeded));
    p.inventory.clear();
    assert_eq!(
        p.encode(
            1,
            0,
            PlayerDescriptionLimits {
                max_message_bytes: 20,
                ..limits()
            }
        ),
        Err(WireError::LimitExceeded)
    );
}
#[test]
fn lifecycle_bounds_strings_and_never_grants_ownership() {
    let enter = [
        0x57, 0xf6, 0, 0, 1, 0, 0, 0, 4, 0, b'a', b'b', b'c', b'd', 0, 0,
    ];
    assert_eq!(
        CharacterLifecycleRequest::decode(&enter, 32, 3),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        CharacterLifecycleRequest::decode(&enter, 15, 4),
        Err(WireError::LimitExceeded)
    );
    assert!(matches!(
        CharacterLifecycleRequest::decode(&enter, 32, 4)
            .unwrap()
            .action,
        CharacterLifecycleAction::EnterWorld {
            character_id: 1,
            ..
        }
    ));
    for (opcode, action) in [
        (0xf7c8u32, CharacterLifecycleAction::EnterWorldRequest),
        (0xf653, CharacterLifecycleAction::LogOff),
    ] {
        let mut message = opcode.to_le_bytes().to_vec();
        message.push(0xa5);
        let decoded = CharacterLifecycleRequest::decode(&message, 32, 4).unwrap();
        assert_eq!(decoded.action, action);
        assert_eq!(decoded.trailing_bytes, 1);
    }
    assert_eq!(
        CharacterTitle {
            current: 7,
            titles: vec![7, 8]
        }
        .encode(1, 0, 1, 128),
        Err(WireError::LimitExceeded)
    );
}
