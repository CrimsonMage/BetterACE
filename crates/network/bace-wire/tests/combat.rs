use bace_wire::opcode::GameActionType as Action;
use bace_wire::*;
#[test]
fn combat_actions_are_bounded_observations_not_accepted_damage() {
    let mut bytes = Vec::new();
    bytes.extend(7u32.to_le_bytes());
    bytes.extend(u32::MAX.to_le_bytes());
    bytes.extend(f32::NAN.to_le_bytes());
    let CombatAction::TargetedMelee {
        target_id,
        height,
        power,
    } = CombatRequest::decode(Action::TargetedMeleeAttack, &bytes, 12)
        .unwrap()
        .action
    else {
        panic!()
    };
    assert_eq!(target_id, 7);
    assert_eq!(height, u32::MAX);
    assert!(power.is_nan());
    assert_eq!(
        CombatRequest::decode(Action::TargetedMeleeAttack, &bytes, 11),
        Err(WireError::LimitExceeded)
    );
    assert!(CombatRequest::decode(Action::TargetedMissileAttack, &[], 128).is_err());
    assert_eq!(
        CombatRequest::decode(Action::CancelAttack, &[1, 2], 2)
            .unwrap()
            .trailing_bytes,
        2
    );
}
#[test]
fn combat_output_rejects_string_and_message_excess_without_a_partial_message() {
    let event = CombatEvent::KillerNotification("café");
    assert_eq!(event.encode(1, 0, 3, 100), Err(WireError::LimitExceeded));
    assert_eq!(event.encode(1, 0, 4, 16), Err(WireError::LimitExceeded));
    assert_eq!(
        CombatEvent::KillerNotification("🦀").encode(1, 0, 5, 100),
        Err(WireError::InvalidEncoding)
    );
    assert_eq!(
        CombatEffect::Sound {
            object_id: 1,
            sound_id: 2,
            volume: 1.0
        }
        .encode(0, 15),
        Err(WireError::LimitExceeded)
    );
}
#[test]
fn targeted_missile_matches_unchanged_official_handler_vectors() {
    for line in include_str!("fixtures/missile.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<_> = line.split(',').collect();
        let bytes: Vec<_> = p[0]
            .as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect();
        let CombatAction::TargetedMissile {
            target_id,
            height,
            accuracy,
        } = CombatRequest::decode(Action::TargetedMissileAttack, &bytes, 12)
            .unwrap()
            .action
        else {
            panic!("missile handler")
        };
        assert_eq!(target_id, p[1].parse::<u32>().unwrap());
        assert_eq!(height, p[2].parse::<u32>().unwrap());
        assert_eq!(accuracy.to_bits(), p[3].parse::<u32>().unwrap());
        for n in 0..12 {
            assert!(CombatRequest::decode(Action::TargetedMissileAttack, &bytes[..n], 12).is_err());
        }
    }
}
