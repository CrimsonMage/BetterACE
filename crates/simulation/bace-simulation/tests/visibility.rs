use bace_gameplay_api::{
    CharacterBinding, SessionId,
    visibility::{VisibilityRejection, VisibilityRequest},
};
use bace_geometry::{Aabb, Vec3};
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, CellId, EntityId};
use std::sync::Arc;
fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(1),
    }
}
fn kernel() -> Kernel {
    let mut world = bace_world::World::default();
    let cell = CellId(0x10100001);
    world
        .register_scene(
            cell,
            bace_physics::SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = bace_physics::Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0., 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
        )
        .unwrap();
        world
            .insert(bace_entity::Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    let rank = bace_character::RankTable::new(&[0, 10, 100]).unwrap();
    let character = bace_character::CharacterProgression::new(
        &[],
        Arc::new(bace_character::ProgressionTables {
            attributes: rank.clone(),
            vitals: rank.clone(),
            trained_skills: rank.clone(),
            specialized_skills: rank,
        }),
        0,
        0,
    )
    .unwrap();
    let mut k = Kernel::with_gameplay_limits(world, 16, 4, 16).unwrap();
    k.register_character(binding(), character).unwrap();
    k
}
#[test]
fn immutable_visibility_query_checks_binding_and_returns_reusable_storage() {
    let mut k = kernel();
    let mut wrong = binding();
    wrong.session = SessionId(10);
    let buffer = Vec::with_capacity(16);
    let storage = buffer.as_ptr();
    let (error, buffer) = k
        .visibility_snapshot_with_buffer(wrong, 16, buffer)
        .unwrap_err();
    assert_eq!(error, VisibilityRejection::NotBound);
    assert_eq!(storage, buffer.as_ptr());
    let snapshot = k
        .visibility_snapshot_with_buffer(binding(), 16, buffer)
        .unwrap();
    assert_eq!(snapshot.candidates.len(), 1);
    assert_eq!(snapshot.candidates[0].entity, EntityId(2));
    assert_eq!(snapshot.candidates[0].distance_squared, 1.0);
    assert_eq!(snapshot.tick, 0);
    k.step().unwrap();
    let next = k
        .visibility_snapshot_with_buffer(binding(), 16, snapshot.candidates)
        .unwrap();
    assert_eq!(next.tick, 1);
    assert_eq!(next.candidates.as_ptr(), storage);
}
#[test]
fn stalled_visibility_consumer_retains_exact_correlation_while_physics_ticks() {
    let mut k = kernel();
    for correlation in [1, 2] {
        k.enqueue(Command::Visibility(Box::new(VisibilityRequest {
            correlation,
            binding: binding(),
            limit: 16,
            candidates: Vec::with_capacity(16),
        })))
        .unwrap();
    }
    k.step().unwrap();
    assert_eq!(k.peek_visibility_outcome().unwrap().correlation, 1);
    for _ in 0..3 {
        k.step().unwrap();
        assert_eq!(k.peek_visibility_outcome().unwrap().correlation, 1);
    }
    assert_eq!(k.ticks(), 4);
    let first = Arc::try_unwrap(k.take_visibility_outcome().unwrap()).unwrap();
    assert!(first.result.is_ok());
    k.step().unwrap();
    assert_eq!(k.take_visibility_outcome().unwrap().correlation, 2);
    assert!(!k.has_visibility_work());
}
