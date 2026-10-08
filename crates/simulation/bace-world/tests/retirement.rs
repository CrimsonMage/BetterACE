use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalMutation, VitalPool};
use bace_geometry::{Aabb, Vec3};
use bace_motion::{
    Capabilities, ExecutionClip, MotionDomain, MotionPhysics, MotionToken, PreparedMotionChain,
    RootFrame,
};
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::World;
use std::sync::Arc;
fn world(airborne: bool, player: bool) -> World {
    let scene = SyntheticScene::new(
        0.0,
        Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
        vec![],
    )
    .unwrap();
    let body = Body::spawn(
        &scene,
        Vec3::new(0.0, 0.0, if airborne { 4.0 } else { 0.5 }),
        0.5,
        Capabilities {
            speed: 3.0,
            jump_impulse: 1.0,
        },
    )
    .unwrap();
    let mut world = World::default();
    world.register_scene(CellId(1), scene).unwrap();
    world
        .insert(Actor {
            id: EntityId(2),
            cell: CellId(1),
            body,
        })
        .unwrap();
    world
        .register_combatant(
            EntityId(2),
            Combatant::new(CombatantProfile {
                maximum_health: 10,
                melee_damage: 1,
                melee_range: 1.0,
                attack_duration: 1.0,
                strike_offsets: vec![0.5],
                player,
            })
            .unwrap()
            .with_resources(
                Some(VitalPool {
                    current: 10,
                    maximum: 10,
                }),
                Some(VitalPool {
                    current: 10,
                    maximum: 10,
                }),
            )
            .unwrap(),
        )
        .unwrap();
    world
}
#[test]
fn airborne_hold_freezes_accepted_state_and_only_exact_rollback_resumes() {
    let mut world = world(true, false);
    world.tick().unwrap();
    let before = world.body(EntityId(2)).unwrap().accepted();
    assert!(!before.grounded());
    let hold = world.hold_retirement(EntityId(2), 7).unwrap();
    assert_eq!(world.hold_retirement(EntityId(2), 7).unwrap(), hold);
    for _ in 0..90 {
        world.tick().unwrap();
        assert_eq!(world.body(EntityId(2)).unwrap().accepted(), before);
    }
    assert!(world.body_mut(EntityId(2)).is_err());
    assert!(
        world
            .teleport(EntityId(2), CellId(1), Vec3::new(2.0, 0.0, 0.5))
            .is_err()
    );
    assert!(world.remove(EntityId(2)).is_none());
    assert!(
        world
            .apply_vital_batch(
                &[VitalMutation {
                    actor: EntityId(2),
                    vital: EntityVital::Health,
                    before: 10,
                    after: 0
                }],
                None
            )
            .is_err()
    );
    let mut wrong = hold;
    wrong.operation += 1;
    assert!(world.release_retirement(wrong).is_err());
    assert!(world.remove_retired(wrong).is_err());
    assert_eq!(world.body(EntityId(2)).unwrap().accepted(), before);
    world.release_retirement(hold).unwrap();
    world.tick().unwrap();
    assert!(world.body(EntityId(2)).unwrap().accepted().position().z < before.position().z);
    let hold = world.hold_retirement(EntityId(2), 8).unwrap();
    world.remove_retired(hold).unwrap();
    assert!(world.body(EntityId(2)).is_err());
    assert!(!world.has_vital_reservations());
    assert!(!world.retirement_held(EntityId(2)));
}
#[test]
fn unfinished_source_motion_must_quiesce_before_archive_hold() {
    let mut world = world(false, false);
    let clip = |id, count| ExecutionClip {
        animation: id,
        frame_count: count,
        low: 0,
        high: -1,
        framerate: 30.0,
        frames: vec![
            RootFrame {
                translation: Vec3::new(0.05, 0.0, 0.0),
                heading: 0.0
            };
            count as usize
        ]
        .into(),
        hooks: vec![].into(),
    };
    let chain = Arc::new(
        PreparedMotionChain::prepare(
            vec![clip(3, 4), clip(4, 2)],
            1,
            1,
            MotionPhysics {
                velocity: Vec3::ZERO,
                omega: Vec3::ZERO,
            },
            0x13000132,
            1.0,
        )
        .unwrap(),
    );
    world
        .begin_motion(
            EntityId(2),
            MotionToken {
                domain: MotionDomain::Interaction,
                owner: 1,
                sequence: 1,
            },
            chain,
        )
        .unwrap();
    assert!(world.hold_retirement(EntityId(2), 9).is_err());
    for _ in 0..6 {
        world.tick().unwrap();
    }
    assert!(
        world.hold_retirement(EntityId(2), 9).is_err(),
        "undelivered callback still owns the motion"
    );
    while world.take_motion_event().is_some() {}
    let hold = world.hold_retirement(EntityId(2), 9).unwrap();
    let state = world.body(EntityId(2)).unwrap().accepted();
    let token = world.source_motion_token(EntityId(2));
    for _ in 0..30 {
        world.tick().unwrap();
    }
    assert_eq!(world.body(EntityId(2)).unwrap().accepted(), state);
    assert_eq!(world.source_motion_token(EntityId(2)), token);
    assert!(world.take_motion_event().is_none());
    world.release_retirement(hold).unwrap();
}
#[test]
fn player_body_cannot_enter_npc_retirement() {
    assert!(world(false, true).hold_retirement(EntityId(2), 1).is_err());
}
