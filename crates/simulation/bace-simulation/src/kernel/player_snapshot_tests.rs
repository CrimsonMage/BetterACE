use super::*;
use crate::kernel::magic_components_fixture as common;
use bace_entity::EntityVital;
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: bace_types::AccountId(1),
        session: bace_gameplay_api::SessionId(7),
    }
}
#[test]
fn captures_accepted_state_without_transferring_or_following_later_mutations() {
    let mut k = common::component_kernel(16);
    let before = k.read_player_snapshot(binding()).unwrap();
    assert!(k.has_characters());
    k.world
        .apply_vital_batch(
            &[bace_entity::VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Mana,
                before: 100,
                after: 90,
            }],
            None,
        )
        .unwrap();
    let after = k.read_player_snapshot(binding()).unwrap();
    assert_eq!(before.world().vitals[2].unwrap().current, 100);
    assert_eq!(after.world().vitals[2].unwrap().current, 90);
    assert!(
        after.character().progression().revision() > before.character().progression().revision()
    );
    assert!(k.has_characters());
    let mut wrong = binding();
    wrong.session = bace_gameplay_api::SessionId(8);
    assert!(matches!(
        k.read_player_snapshot(wrong),
        Err(CharacterRegistrationError::OwnershipMismatch)
    ));
}
#[test]
fn full_snapshot_outbox_retains_next_request_and_physics_ticks() {
    let mut k = common::component_kernel(16);
    for correlation in 1..=2 {
        assert!(
            k.try_enqueue(Command::PlayerSnapshot(crate::PlayerSnapshotRequest {
                correlation,
                operation: None,
                binding: binding()
            }))
            .is_ok()
        );
    }
    k.step().unwrap();
    assert_eq!(k.peek_player_snapshot_outcome().unwrap().correlation, 1);
    k.step().unwrap();
    assert_eq!(k.take_player_snapshot_outcome().unwrap().correlation, 1);
    k.step().unwrap();
    assert_eq!(k.take_player_snapshot_outcome().unwrap().correlation, 2);
    assert!(k.has_characters());
    assert!(!k.has_player_snapshot_work());
}

#[test]
fn operation_capture_requires_exact_reservation_domain_id_and_revision() {
    use crate::PlayerSnapshotOperation as O;
    let mut k = common::component_kernel(16);
    let before = k.read_player_snapshot(binding()).unwrap();
    let revision = before.character().progression().revision();
    k.characters.reserve_death(binding().actor, 9).unwrap();
    assert!(matches!(
        k.read_player_snapshot(binding()),
        Err(CharacterRegistrationError::DurabilityPending)
    ));
    for (operation, revision) in [
        (O::PlayerDeath(8), revision),
        (O::Allegiance(9), revision),
        (O::StaffSpell(9), revision),
        (O::Skill(9), revision),
        (O::PlayerDeath(9), revision + 1),
    ] {
        assert!(matches!(
            k.read_player_operation_snapshot(binding(), operation, revision),
            Err(CharacterRegistrationError::DurabilityPending)
        ));
    }
    let capture = k
        .read_player_operation_snapshot(binding(), O::PlayerDeath(9), revision)
        .unwrap();
    assert_eq!(capture.operation(), Some((O::PlayerDeath(9), revision)));
    assert_eq!(capture.world(), before.world());
    assert!(k.take_player_state(binding()).is_err());
    k.characters.release_death(binding().actor, 9).unwrap();
    assert!(
        k.read_player_operation_snapshot(binding(), O::PlayerDeath(9), revision)
            .is_err()
    );
    assert!(k.read_player_snapshot(binding()).is_ok());
}
