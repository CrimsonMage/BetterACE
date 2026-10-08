use super::*;
use crate::pve::{GeneratedNpcOrigin, NpcBlueprint, Population, PreparedNpcGeometry};
use bace_entity::CombatantProfile;
use bace_geometry::Vec3;
use bace_motion::Capabilities;
use bace_physics::{
    CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon, GeometryCell,
    GeometryRegion,
};
use bace_types::CellId;
use bace_world::World;
use std::sync::Arc;
pub(super) fn physical_fixture() -> (
    Population,
    World,
    NpcBlueprint,
    PreparedNpcGeometry,
    GeneratedNpcOrigin,
) {
    let mut world = World::default();
    let floor = CollisionFace {
        polygon: GdlePolygon::prepare(vec![
            Vec3::new(0., 0., 0.),
            Vec3::new(24., 0., 0.),
            Vec3::new(24., 24., 0.),
            Vec3::new(0., 24., 0.),
        ])
        .unwrap(),
        two_sided: false,
        object: None,
    };
    world
        .install_geometry(Arc::new(
            GeometryRegion::prepare(vec![GeometryCell {
                id: 0x12340100,
                terrain: false,
                restriction: None,
                solids: vec![],
                static_primitives: vec![],
                boundary: vec![
                    CollisionPlane {
                        normal: Vec3::new(1., 0., 0.),
                        distance: 0.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(-1., 0., 0.),
                        distance: 24.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(0., 1., 0.),
                        distance: 0.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(0., -1., 0.),
                        distance: 24.,
                    },
                ],
                faces: vec![floor],
                portals: vec![],
            }])
            .unwrap(),
        ))
        .unwrap();
    let blueprint = NpcBlueprint {
        cell: CellId(0x12340100),
        position: Vec3::new(10., 10., 0.),
        radius: 0.5,
        capabilities: Capabilities {
            speed: 1.,
            jump_impulse: 0.,
        },
        combat: CombatantProfile {
            maximum_health: 20,
            melee_damage: 2,
            melee_range: 1.,
            attack_duration: 0.5,
            strike_offsets: vec![0.25],
            player: false,
        },
        visual_range: 18.,
        think_interval: 1,
        corpse_template: 10,
        xp_override: None,
        loot: vec![],
        death_animation_ticks: 1,
        respawn_ticks: 1,
        corpse_decay_ticks: 10,
    };
    let physics = bace_motion::MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let cycle = || {
        bace_motion::RootCycle::prepare(vec![bace_motion::RootSegment {
            low: 0,
            high: 0,
            frame_count: 1,
            framerate: 30.,
            frames: vec![bace_motion::RootFrame {
                translation: Vec3::ZERO,
                heading: 0.,
            }],
        }])
        .unwrap()
    };
    let geometry = PreparedNpcGeometry {
        shape: Arc::new(
            CollisionShape::prepare(
                vec![CollisionSphere {
                    center: Vec3::new(0., 0., 0.5),
                    radius: 0.5,
                }],
                0.1,
                0.1,
            )
            .unwrap(),
        ),
        locomotion: Arc::new(bace_motion::AnimatedLocomotion {
            profile: bace_motion::LocomotionProfile {
                style: 0x8000003d,
                ready: physics,
                walk: physics,
                run: physics,
                sidestep: physics,
                turn: physics,
            },
            ready: cycle(),
            walk: cycle(),
            run: cycle(),
        }),
        heading: 0.,
        maximum_turn_rate: 0.,
        run_rate: 1.,
    };
    let origin = GeneratedNpcOrigin {
        generator: EntityId(5),
        incarnation: 1,
        content_revision: 1,
        profile: 0,
        child_incarnation: 1,
    };
    (Population::new(8), world, blueprint, geometry, origin)
}
