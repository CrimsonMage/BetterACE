//! Save race regressions use an explicitly synthetic scene, not a DAT parity claim.
#[allow(dead_code, unused_imports)]
mod magic_common;
use bace_magic::Vital;
use bace_motion::MotionIntent;
use bace_simulation::PlayerWorldSaveMarker;
use magic_common::*;

fn marker(k: &Kernel) -> PlayerWorldSaveMarker {
    PlayerWorldSaveMarker {
        actor: EntityId(1),
        revision: k.character(EntityId(1)).unwrap().revision(),
        snapshot: k.player_world_snapshot(EntityId(1)).unwrap(),
    }
}
fn move_player(k: &mut Kernel, sequence: u32, direction: Vec3) {
    k.enqueue(Command::Movement {
        actor: EntityId(1),
        epoch: 0,
        sequence,
        intent: MotionIntent::new(direction, false).unwrap(),
    })
    .unwrap();
    step(k);
}
#[test]
fn admission_does_not_invent_dirty_world_state_and_movement_retains_original_age() {
    let mut k = kernel(16);
    step(&mut k);
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 0);
    assert_eq!(k.player_world_dirty_since(EntityId(1)), None);
    move_player(&mut k, 1, Vec3::new(1.0, 0.0, 0.0));
    let age = k.player_world_dirty_since(EntityId(1)).unwrap();
    let revision = k.character(EntityId(1)).unwrap().revision();
    for _ in 0..150 {
        step(&mut k);
    }
    assert_eq!(k.player_world_dirty_since(EntityId(1)), Some(age));
    assert!(k.character(EntityId(1)).unwrap().revision() > revision);
}
#[test]
fn older_completion_and_failed_save_cannot_clear_newer_accepted_position() {
    let mut k = kernel(16);
    move_player(&mut k, 1, Vec3::new(1.0, 0.0, 0.0));
    let saved = marker(&k);
    let original_age = k.player_world_dirty_since(EntityId(1)).unwrap();
    k.mark_player_world_save(saved).unwrap();
    step(&mut k);
    let current = marker(&k);
    assert_ne!(current.snapshot, saved.snapshot);
    let mut wrong = saved;
    wrong.revision += 1;
    assert!(k.acknowledge_player_world_save(wrong).is_err());
    assert_eq!(k.player_world_dirty_since(EntityId(1)), Some(original_age));
    k.fail_player_world_save(saved).unwrap();
    assert_eq!(k.player_world_dirty_since(EntityId(1)), Some(original_age));
    assert!(k.mark_player_world_save(saved).is_err());
    k.mark_player_world_save(current).unwrap();
    step(&mut k);
    k.acknowledge_player_world_save(current).unwrap();
    assert!(k.player_world_dirty_since(EntityId(1)).is_some());
    move_player(&mut k, 2, Vec3::ZERO);
    let final_marker = marker(&k);
    k.mark_player_world_save(final_marker).unwrap();
    k.acknowledge_player_world_save(final_marker).unwrap();
    assert_eq!(k.player_world_dirty_since(EntityId(1)), None);
}
#[test]
fn spell_resource_changes_and_logout_include_authoritative_vitals() {
    let mut k = kernel(16);
    k.register_magic_spell(spell(
        100,
        SpellEffect::Boost {
            vital: Vital::Health,
            minimum: 20,
            maximum: 20,
        },
    ))
    .unwrap();
    k.enqueue(Command::Cast {
        context: context(1),
        request: CastRequest::Targeted {
            target: EntityId(1),
            spell: 100,
        },
    })
    .unwrap();
    for _ in 0..8 {
        step(&mut k);
        while k.take_cast_outcome().is_some() {}
    }
    let saved = marker(&k);
    assert_eq!(saved.snapshot.vitals[0].unwrap().current, 70);
    assert_eq!(saved.snapshot.vitals[2].unwrap().current, 90);
    assert!(k.player_world_dirty_since(EntityId(1)).is_some());
    let owned = k
        .take_player_state(CharacterBinding {
            session: SessionId(7),
            account: AccountId(1),
            actor: EntityId(1),
        })
        .unwrap();
    assert_eq!(owned.world, Some(saved.snapshot));
    assert!(owned.recovery.is_some());
    assert_eq!(k.player_world_dirty_since(EntityId(1)), None);
}
