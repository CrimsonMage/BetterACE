use bace_wire::{UiAction, UiInput, WireError, opcode::GameActionType};

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
fn calls(action: &UiAction) -> String {
    match action {
        UiAction::SingleOption { option, enabled } => format!("option={option}:{enabled}"),
        UiAction::AddShortcut(s) => {
            format!("shortcut={}:{}:{}:{}", s.index, s.object, s.spell, s.layer)
        }
        UiAction::RemoveShortcut(index) => format!("remove={index}"),
        UiAction::AddFavorite {
            spell,
            position,
            bar,
        } => format!("favorite={spell}:{position}:{bar}"),
        UiAction::RemoveFavorite { spell, bar } => format!("unfavorite={spell}:{bar}"),
        UiAction::Filters(filters) => format!("filters={filters}"),
        UiAction::Component { template, quantity } => {
            format!("component={template}:{}", *quantity as u32)
        }
        UiAction::Options(options) => {
            let mut result = vec![format!("options1={}", options.options1)];
            if let Some(value) = options.options2 {
                result.push(format!("options2={value}"));
            }
            if let Some(value) = &options.gameplay {
                result.push(format!(
                    "gameplay={}",
                    value.iter().map(|b| format!("{b:02x}")).collect::<String>()
                ));
            }
            result.join("|")
        }
    }
}

#[test]
fn compiled_ace_handlers_establish_ui_layouts_consumption_and_signed_bits() {
    let mut count = 0;
    for line in include_str!("fixtures/ui_input.csv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let columns: Vec<_> = line.split(',').collect();
        let name = columns[0];
        let op = GameActionType(columns[1].parse().unwrap());
        let payload = bytes(columns[2]);
        let consumed: usize = columns[3].parse().unwrap();
        let result = UiInput::decode(op, &payload, 131072).unwrap();
        assert_eq!(payload.len() - result.trailing_bytes, consumed, "{name}");
        assert_eq!(calls(&result.action), columns[4], "{name}");
        // Gameplay options are an opaque suffix in ACE; any suffix length is
        // valid. Other read prefixes require their complete source field widths.
        let minimum = if name == "options_full" {
            consumed - 5
        } else {
            consumed
        };
        for length in 0..minimum {
            assert!(
                UiInput::decode(op, &payload[..length], 131072).is_err(),
                "{name} truncated at {length}"
            );
        }
        assert_eq!(
            UiInput::decode(op, &payload, payload.len() - 1),
            Err(WireError::LimitExceeded)
        );
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn full_options_retains_tables_that_pinned_ace_reads_then_discards() {
    let line = include_str!("fixtures/ui_input.csv")
        .lines()
        .find(|line| line.starts_with("options_full,"))
        .unwrap();
    let payload = bytes(line.split(',').nth(2).unwrap());
    let UiAction::Options(options) =
        UiInput::decode(GameActionType::SetCharacterOptions, &payload, 131072)
            .unwrap()
            .action
    else {
        panic!("options");
    };
    assert_eq!(options.bars.len(), 8);
    assert_eq!(options.bars[0], [123, 456]);
    for index in 1..8 {
        assert_eq!(options.bars[index], [999 + index as u32]);
    }
    let shortcuts = options.shortcuts.unwrap();
    assert_eq!(shortcuts.len(), 1);
    assert_eq!(
        (
            shortcuts[0].index,
            shortcuts[0].object,
            shortcuts[0].spell,
            shortcuts[0].layer
        ),
        (17, 0xf0100203, 100, 8)
    );
    assert_eq!(options.components, Some(vec![(5001, 40), (5002, -1)]));
    assert_eq!(options.filters, 0x11112222);
    assert_eq!(options.gameplay, Some(vec![0xde, 0xad, 0xbe, 0xef, 0]));
    // Negative desired counts are preserved as decoded bits, not authorized by
    // this codec. The owning domain rejects invalid quantities atomically.
}

#[test]
fn hostile_counts_and_unimplemented_sections_fail_before_state_mutation() {
    let op = GameActionType::SetCharacterOptions;
    for flag in [2, 0x80, 0x100, 0x80000000, 4 | 16, 4 | 1024] {
        assert_eq!(
            UiInput::decode(op, &words(&[flag, 0, 0]), 131072),
            Err(WireError::InvalidEncoding)
        );
    }
    assert_eq!(
        UiInput::decode(op, &words(&[1, 0, 19]), 131072),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        UiInput::decode(op, &words(&[0, 0, 4097]), 131072),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        UiInput::decode(op, &words(&[8, 0, 0, 4097]), 131072),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        UiInput::decode(op, &words(&[8, 0, 0, 1]), 131072),
        Err(WireError::Truncated)
    );
    let mut oversized = words(&[512, 0, 0]);
    oversized.resize(12 + 65537, 0);
    assert_eq!(
        UiInput::decode(op, &oversized, 131072),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        UiInput::decode(GameActionType(0xffff), &[], 131072),
        Err(WireError::InvalidEncoding)
    );
}

#[test]
fn absent_optional_ui_sections_preserve_distinctions_and_pinned_defaults() {
    let UiAction::Options(options) = UiInput::decode(
        GameActionType::SetCharacterOptions,
        &words(&[0, 7, 0]),
        131072,
    )
    .unwrap()
    .action
    else {
        panic!("options");
    };
    assert_eq!(options.filters, 0x3fff);
    assert_eq!(options.shortcuts, None);
    assert_eq!(options.components, None);
    assert_eq!(options.options2, None);
    assert_eq!(options.gameplay, None);
    assert_eq!(options.bars, vec![Vec::<u32>::new()]);
}
