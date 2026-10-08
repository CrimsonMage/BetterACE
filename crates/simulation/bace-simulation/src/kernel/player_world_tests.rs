use crate::kernel::magic_components_fixture as common;
use crate::{CharacterRegistrationError, Kernel};
use bace_entity::EntityVital;
use bace_gameplay_api::{CharacterBinding, SessionId};
use bace_geometry::Vec3;
use bace_types::{AccountId, CellId, EntityId};
fn legacy_kernel() -> Kernel {
    let mut kernel = common::component_kernel(16);
    common::register_component_caster(&mut kernel);
    // Remove the unrelated casting supplement so the narrow transfer test can
    // only pass by preserving the accepted world state being exercised.
    kernel.magic.take_actor_recovery(EntityId(1), 0.0).unwrap();
    kernel.recovery_revisions.remove(&EntityId(1));
    kernel
}
fn binding() -> CharacterBinding {
    CharacterBinding {
        actor: EntityId(1),
        account: AccountId(1),
        session: SessionId(7),
    }
}
#[test]
fn between_tick_vital_change_requires_complete_transfer_without_losing_the_owner() {
    let mut k = legacy_kernel();
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
    assert_eq!(k.player_world_dirty_since(EntityId(1)), None);
    assert!(matches!(
        k.take_character(binding()),
        Err(CharacterRegistrationError::CompleteStateRequired)
    ));
    assert!(k.has_characters());
    let state = k.take_player_state(binding()).unwrap();
    assert_eq!(state.world.unwrap().vitals[2].unwrap().current, 90);
    assert_eq!(state.character.progression.revision(), 1);
}
#[test]
fn between_tick_teleport_cannot_escape_through_character_only_transfer() {
    let mut k = legacy_kernel();
    k.world
        .teleport(EntityId(1), CellId(1), Vec3::new(2.0, 0.0, 0.5))
        .unwrap();
    assert_eq!(k.player_world_dirty_since(EntityId(1)), None);
    assert!(matches!(
        k.take_character_with_rare(binding()),
        Err(CharacterRegistrationError::CompleteStateRequired)
    ));
    assert_eq!(
        k.take_player_state(binding())
            .unwrap()
            .world
            .unwrap()
            .position
            .x,
        2.0
    );
}
