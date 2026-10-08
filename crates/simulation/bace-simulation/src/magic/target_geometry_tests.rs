//! Synthetic connected-cell authority regression; not a retail motion claim.
use super::*;
use bace_physics::{
    CellPortalGeometry, CollisionFace, CollisionPlane, CollisionShape, CollisionSphere,
    GdlePolygon, GeometryCell, GeometryRegion, GeometrySpawn,
};
fn region(wall: bool) -> Arc<GeometryRegion> {
    let face = |x: f32| CollisionFace {
        polygon: GdlePolygon::prepare(vec![
            Vec3::new(x, 0., 0.),
            Vec3::new(x, 10., 0.),
            Vec3::new(x, 10., 10.),
            Vec3::new(x, 0., 10.),
        ])
        .unwrap(),
        two_sided: true,
        object: None,
    };
    let mut left = GeometryCell {
        id: 1,
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
                distance: 5.,
            },
            CollisionPlane {
                normal: Vec3::new(0., 1., 0.),
                distance: 0.,
            },
            CollisionPlane {
                normal: Vec3::new(0., -1., 0.),
                distance: 10.,
            },
        ],
        faces: vec![],
        portals: vec![],
    };
    let mut right = left.clone();
    right.id = 2;
    right.boundary[0].distance = -5.;
    right.boundary[1].distance = 10.;
    if wall {
        right.faces.push(face(5.5));
    }
    left.portals.push(CellPortalGeometry {
        destination: 2,
        polygon: face(5.).polygon,
        translation: Vec3::ZERO,
    });
    Arc::new(GeometryRegion::prepare(vec![left, right]).unwrap())
}
#[test]
fn direct_spell_range_uses_cylinders_and_checks_real_connected_cell_geometry() {
    for blocked in [false, true] {
        let geometry = region(blocked);
        let shape = Arc::new(
            CollisionShape::prepare(
                vec![CollisionSphere {
                    center: Vec3::new(0., 0., 0.5),
                    radius: 0.5,
                }],
                0.,
                0.,
            )
            .unwrap()
            .with_nominal_dimensions(0.5, 1.)
            .unwrap(),
        );
        let mut world = World::default();
        world.install_geometry(geometry.clone()).unwrap();
        for (id, x) in [(1, 4.), (2, 6.)] {
            let body = bace_physics::Body::spawn_geometry(
                &geometry,
                GeometrySpawn {
                    cell: id,
                    position: Vec3::new(x, 2., 0.),
                    shape: shape.clone(),
                    capabilities: bace_motion::Capabilities {
                        speed: 5.,
                        jump_impulse: 5.,
                    },
                    heading: 0.,
                    maximum_turn_rate: 3.,
                },
            )
            .unwrap();
            world
                .insert(bace_entity::Actor {
                    id: EntityId(id),
                    cell: CellId(id),
                    body,
                })
                .unwrap();
            world
                .register_combatant(
                    EntityId(id),
                    bace_entity::Combatant::new(bace_entity::CombatantProfile {
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
        }
        let spell = PreparedSpell {
            id: 1,
            school: bace_magic::MagicSchool::Life,
            power: 0,
            base_mana: 0,
            range_constant: 1.,
            range_per_skill: 0.,
            harmful: false,
            resistable: false,
            effect: SpellEffect::Boost {
                vital: Vital::Health,
                minimum: 1,
                maximum: 1,
            },
        };
        let observation = observe(&world, EntityId(1), Some(EntityId(2)), &spell, 100).unwrap();
        assert!(observation.target_in_range);
        assert_eq!(observation.geometry_clear, !blocked);
        let mut short = spell;
        short.range_constant = f32::from_bits(1.0f32.to_bits() - 1);
        assert!(
            !observe(&world, EntityId(1), Some(EntityId(2)), &short, 100)
                .unwrap()
                .target_in_range
        );
    }
}
