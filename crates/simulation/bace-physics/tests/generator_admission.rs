use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::*;
fn terrain() -> GeometryRegion {
    let polygon = |v: Vec<Vec3>| CollisionFace {
        polygon: GdlePolygon::prepare(v).unwrap(),
        two_sided: false,
        object: None,
    };
    GeometryRegion::prepare(vec![GeometryCell {
        id: 0x12340001,
        terrain: true,
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
        faces: vec![
            polygon(vec![
                Vec3::new(0., 0., 0.),
                Vec3::new(24., 0., 12.),
                Vec3::new(0., 24., 0.),
            ]),
            polygon(vec![
                Vec3::new(24., 24., 12.),
                Vec3::new(0., 24., 0.),
                Vec3::new(24., 0., 12.),
            ]),
        ],
        portals: vec![],
    }])
    .unwrap()
}
#[test]
fn terrain_query_retains_plane_slope_and_building_attachment() {
    let r = terrain();
    let (z, n) = r.ground_at(0x12340001, Vec3::new(8., 7., 999.)).unwrap();
    assert_eq!(z, 4.);
    assert!((n.z - 0.8944272).abs() < 0.000001);
    assert!(!r.has_building(0x12340001).unwrap());
    assert!(r.clone().with_building_cells(&[0x12340002]).is_err());
    assert!(
        r.with_building_cells(&[0x12340001])
            .unwrap()
            .has_building(0x12340001)
            .unwrap()
    );
    assert!(
        terrain()
            .ground_at(0x12340001, Vec3::new(100., 0., 0.))
            .is_err()
    );
}
#[test]
fn geometry_body_cannot_be_relocated_or_advanced_by_synthetic_scene() {
    let shape = std::sync::Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 1.),
                radius: 0.25,
            }],
            0.,
            0.,
        )
        .unwrap(),
    );
    let mut body = Body::spawn_geometry(
        &terrain(),
        GeometrySpawn {
            cell: 0x12340001,
            position: Vec3::new(8., 7., 5.),
            shape,
            capabilities: Capabilities {
                speed: 5.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 1.,
        },
    )
    .unwrap();
    let synthetic = SyntheticScene::new(
        0.,
        Aabb::new(Vec3::new(-100., -100., -10.), Vec3::new(100., 100., 100.)).unwrap(),
        vec![],
    )
    .unwrap();
    let before = body.accepted();
    assert!(body.validate_placement(&synthetic).is_err());
    assert!(
        body.server_teleport(&synthetic, Vec3::new(0., 0., 5.))
            .is_err()
    );
    body.step(&synthetic);
    assert_eq!(body.accepted(), before);
}
