use bace_wire::{CombatEvent, TargetQueryInput, encode_item_mana_query, opcode::GameActionType};
#[test]
fn original_query_event_constructor_packets() {
    let mut count = 0;
    for row in include_str!("fixtures/target_query.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<_> = row.split('|').collect();
        let target = f[1].parse().unwrap();
        let value = f32::from_bits(u32::from_str_radix(f[2], 16).unwrap());
        let bytes = if f[0] == "H" {
            CombatEvent::UpdateHealth {
                target_id: target,
                fraction: value,
            }
            .encode(0x50000001, 42, 4096, 4096)
            .unwrap()
        } else {
            encode_item_mana_query(0x50000001, 42, target, value, f[3].parse().unwrap(), 4096)
                .unwrap()
        };
        let actual: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
        assert_eq!(actual, f[4]);
        count += 1;
    }
    assert_eq!(count, 30);
}
#[test]
fn query_inputs_bound_lengths_and_preserve_zero_clear() {
    for (action, expected) in [
        (GameActionType::QueryHealth, TargetQueryInput::Health(0)),
        (GameActionType::QueryItemMana, TargetQueryInput::ItemMana(0)),
    ] {
        for length in 0..4 {
            assert!(TargetQueryInput::decode(action, &[0; 4][..length], 32).is_err());
        }
        assert_eq!(
            TargetQueryInput::decode(action, &[0; 8], 32).unwrap(),
            expected
        );
        assert!(TargetQueryInput::decode(action, &[0; 8], 4).is_err());
    }
    assert!(encode_item_mana_query(1, 2, 3, 0.5, 2, 64).is_err());
    assert!(encode_item_mana_query(1, 2, 3, 0.5, 1, 27).is_err());
}
