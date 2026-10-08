//! Raw ACE packet bits remain untrusted until single-owner combat admission.
use super::*;
use bace_session::{SessionState, decode_combat};
use bace_wire::opcode::GameActionType;
fn packet(sequence: u32, opcode: GameActionType, power: f32) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [0xf7b1, sequence, opcode.0, 2, 2, power.to_bits()] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}
#[test]
fn raw_attack_float_bits_reach_domain_but_invalid_values_never_mutate_gameplay() {
    for opcode in [
        GameActionType::TargetedMeleeAttack,
        GameActionType::TargetedMissileAttack,
    ] {
        let mut k = kernel(16, 4, 16);
        k.enqueue(combat(
            1,
            CombatRequest::ChangeMode(if opcode == GameActionType::TargetedMeleeAttack {
                2
            } else {
                4
            }),
        ))
        .unwrap();
        k.step().unwrap();
        k.take_combat_outcome();
        let before = k.world().combatant(EntityId(1)).unwrap().revision();
        for (i, power) in [
            f32::from_bits(0x3f800001),
            -f32::from_bits(1),
            f32::from_bits(0x7fc00123),
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]
        .into_iter()
        .enumerate()
        {
            let bytes = packet(i as u32 + 2, opcode, power);
            let decoded =
                decode_combat(SessionState::WorldConnected, context(0), &bytes, 128).unwrap();
            let value = match decoded.request {
                CombatRequest::TargetedMelee { power, .. } => power,
                CombatRequest::TargetedMissile { accuracy, .. } => accuracy,
                _ => panic!("wrong opcode dispatch"),
            };
            assert_eq!(value.to_bits(), power.to_bits());
            k.enqueue(Command::Combat {
                context: decoded.context,
                request: decoded.request,
            })
            .unwrap();
            k.step().unwrap();
            assert_eq!(
                k.take_combat_outcome().unwrap().result,
                Err(CombatRejection::InvalidRequest)
            );
            assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 10);
            assert_eq!(k.world().combatant(EntityId(1)).unwrap().revision(), before);
            assert!(k.take_combat_event().is_none());
            assert!(k.take_physical_launch().is_none());
        }
    }
}
#[test]
fn valid_full_power_packet_still_waits_for_authoritative_hook() {
    let mut k = kernel(16, 4, 16);
    k.enqueue(combat(1, CombatRequest::ChangeMode(2))).unwrap();
    k.step().unwrap();
    k.take_combat_outcome();
    let decoded = decode_combat(
        SessionState::WorldConnected,
        context(0),
        &packet(2, GameActionType::TargetedMeleeAttack, 1.0),
        128,
    )
    .unwrap();
    k.enqueue(Command::Combat {
        context: decoded.context,
        request: decoded.request,
    })
    .unwrap();
    k.step().unwrap();
    assert!(matches!(
        k.take_combat_outcome().unwrap().result,
        Ok(CombatChange::AttackStarted { .. })
    ));
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 10);
    k.step().unwrap();
    assert_eq!(k.world().combatant(EntityId(2)).unwrap().health(), 0);
}
