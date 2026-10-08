use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalMutation};
use bace_geometry::Vec3;
use bace_motion::Capabilities;
use bace_physics::{
    Body, CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon,
    GeometryCell, GeometryRegion, GeometrySpawn,
};
use bace_types::{CellId, EntityId};
use bace_world::World;
use std::sync::Arc;

const CELL: CellId = CellId(0x100);

fn geometry() -> (Arc<GeometryRegion>, Arc<CollisionShape>) {
    let region = Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: CELL.0,
            restriction: None,
            terrain: false,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![
                CollisionPlane {
                    normal: Vec3::new(1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(-1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., 1., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., -1., 0.),
                    distance: 20.,
                },
            ],
            faces: vec![CollisionFace {
                polygon: GdlePolygon::prepare(vec![
                    Vec3::new(-100., -100., 0.),
                    Vec3::new(100., -100., 0.),
                    Vec3::new(100., 100., 0.),
                    Vec3::new(-100., 100., 0.),
                ])
                .unwrap(),
                two_sided: true,
                object: None,
            }],
            portals: vec![],
        }])
        .unwrap(),
    );
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 0.5),
                radius: 0.5,
            }],
            0.,
            0.2,
        )
        .unwrap(),
    );
    (region, shape)
}

fn actor_at(
    id: u32,
    position: Vec3,
    region: &GeometryRegion,
    shape: &Arc<CollisionShape>,
) -> Actor {
    Actor {
        id: EntityId(id),
        cell: CELL,
        body: Body::spawn_geometry(
            region,
            GeometrySpawn {
                cell: CELL.0,
                position,
                shape: shape.clone(),
                capabilities: Capabilities {
                    speed: 3.,
                    jump_impulse: 1.,
                },
                heading: 0.,
                maximum_turn_rate: 1.,
            },
        )
        .unwrap(),
    }
}

fn actor(id: u32, region: &GeometryRegion, shape: &Arc<CollisionShape>) -> Actor {
    actor_at(id, Vec3::ZERO, region, shape)
}

#[test]
fn committed_no_corpse_roots_admit_atomically_and_retain_dead_player() {
    let (region, shape) = geometry();
    let mut world = World::default();
    world.install_geometry(region.clone()).unwrap();
    world.insert(actor(1, &region, &shape)).unwrap();
    world
        .register_combatant(
            EntityId(1),
            Combatant::new(CombatantProfile {
                maximum_health: 100,
                melee_damage: 1,
                melee_range: 1.,
                attack_duration: 1.,
                strike_offsets: vec![0.5],
                player: true,
            })
            .unwrap(),
        )
        .unwrap();
    let roots = vec![actor(2, &region, &shape), actor(3, &region, &shape)];
    assert!(
        world
            .preflight_player_death_world_roots(EntityId(1), &roots)
            .is_err()
    );
    world
        .apply_vital_batch(
            &[VitalMutation {
                actor: EntityId(1),
                vital: EntityVital::Health,
                before: 100,
                after: 0,
            }],
            None,
        )
        .unwrap();
    assert!(
        world
            .preflight_player_death_world_roots(EntityId(1), &[])
            .is_ok()
    );
    let moved = vec![actor_at(4, Vec3::new(1., 0., 0.), &region, &shape)];
    assert!(
        world
            .adopt_player_death_world_roots(EntityId(1), moved)
            .is_err()
    );
    assert!(world.body(EntityId(4)).is_err());
    let rejected = vec![actor(2, &region, &shape), actor(2, &region, &shape)];
    assert!(
        world
            .adopt_player_death_world_roots(EntityId(1), rejected)
            .is_err()
    );
    assert!(world.body(EntityId(2)).is_err());
    assert!(
        world
            .adopt_player_death_world_roots(EntityId(1), roots)
            .is_ok()
    );
    assert!(world.body(EntityId(1)).is_ok());
    assert_eq!(
        world
            .vital(EntityId(1), EntityVital::Health)
            .unwrap()
            .current,
        0
    );
    assert!(world.is_anchor(EntityId(2)) && world.is_anchor(EntityId(3)));
}
