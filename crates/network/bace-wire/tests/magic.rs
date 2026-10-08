use bace_wire::opcode::GameActionType;
use bace_wire::{
    Enchantment, EnchantmentRegistry, MagicAction, MagicEvent, MagicRequest, WireError,
};
fn enchantment() -> Enchantment {
    Enchantment {
        spell_id: 1,
        layer: 1,
        category: 1,
        has_spell_set_id: 0,
        power: 10,
        start_time: 0.0,
        duration: 30.0,
        caster_id: 7,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        stat_type: 0x8001,
        stat_key: 1,
        stat_value: 10.0,
        spell_set_id: None,
    }
}
#[test]
fn cast_payload_widths_budgets_suffixes_and_truncation() {
    let payload = [1, 0, 0, 0, 255, 255, 255, 255, 0xa5];
    let request = MagicRequest::decode(GameActionType::CastTargetedSpell, &payload, 16).unwrap();
    assert_eq!(
        request.action,
        MagicAction::Targeted {
            target_id: 1,
            spell_id: u32::MAX
        }
    );
    assert_eq!(request.trailing_bytes, 1);
    for n in 0..8 {
        assert!(
            MagicRequest::decode(GameActionType::CastTargetedSpell, &payload[..n], 16).is_err()
        );
    }
    assert_eq!(
        MagicRequest::decode(GameActionType::CastTargetedSpell, &payload, 8),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        MagicRequest::decode(GameActionType::CastUntargetedSpell, &payload[..4], 16)
            .unwrap()
            .action,
        MagicAction::Untargeted { spell_id: 1 }
    );
}
#[test]
fn registry_limits_optional_consistency_and_bulk_limits_are_explicit() {
    let mut e = enchantment();
    e.has_spell_set_id = 1;
    assert_eq!(e.encode(), Err(WireError::InvalidEncoding));
    let e = enchantment();
    let registry = EnchantmentRegistry {
        additive: vec![e; 2],
        ..Default::default()
    };
    assert_eq!(registry.encode(1, 4096), Err(WireError::LimitExceeded));
    assert_eq!(registry.encode(2, 8), Err(WireError::LimitExceeded));
    assert_eq!(
        MagicEvent::UpdateMultiple(&[e; 2]).encode(7, 1, 1, 4096),
        Err(WireError::LimitExceeded)
    );
    assert_eq!(
        MagicEvent::Purge.encode(7, 1, 2, 8),
        Err(WireError::LimitExceeded)
    );
}
