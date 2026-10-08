use bace_geometry::Vec3;
use bace_physics::*;
fn face(points: &[[f32; 3]]) -> CollisionFace {
    CollisionFace {
        polygon: GdlePolygon::prepare(points.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect())
            .unwrap(),
        two_sided: true,
        object: None,
    }
}
fn region(wall: bool) -> GeometryRegion {
    let mut faces = vec![face(&[
        [0., 0., 0.],
        [10., 0., 0.],
        [10., 10., 0.],
        [0., 10., 0.],
    ])];
    if wall {
        faces.push(face(&[
            [5., 0., 0.],
            [5., 10., 0.],
            [5., 10., 10.],
            [5., 0., 10.],
        ]));
    }
    GeometryRegion::prepare(vec![GeometryCell {
        terrain: false,
        restriction: None,
        solids: vec![],
        static_primitives: vec![],
        id: 1,
        boundary: vec![
            CollisionPlane {
                normal: Vec3::new(1., 0., 0.),
                distance: 0.,
            },
            CollisionPlane {
                normal: Vec3::new(-1., 0., 0.),
                distance: 10.,
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
        faces,
        portals: vec![],
    }])
    .unwrap()
}
fn shape() -> CollisionShape {
    CollisionShape::prepare(
        vec![CollisionSphere {
            center: Vec3::new(0., 0., 1.),
            radius: 1.,
        }],
        0.0,
        0.1,
    )
    .unwrap()
}
#[test]
fn continuous_thin_wall_contact_slides_without_tunneling() {
    let r = region(true);
    let s = shape();
    let result = r
        .move_body(GeometryStep {
            cell: 1,
            position: Vec3::new(2., 2., 0.),
            velocity: Vec3::new(100., 10., -1.),
            seconds: 0.1,
            shape: &s,
            dynamics: &[],
            ignore: 7,
            allowed_restrictions: &[],
            was_grounded: true,
            player_status: None,
        })
        .unwrap();
    assert!(result.position.x <= 4.001);
    assert!(result.position.y > 2.5);
    assert!(result.grounded);
    assert_eq!(result.velocity.x, 0.0);
}
#[test]
fn shape_and_dynamic_contact_share_authoritative_space() {
    let r = region(false);
    let s = shape();
    let dynamics = [DynamicSphere {
        cylinder_height: None,
        player_status: None,
        object: 8,
        cell: 1,
        sphere: CollisionSphere {
            center: Vec3::new(5., 2., 1.),
            radius: 1.,
        },
    }];
    let result = r
        .move_body(GeometryStep {
            cell: 1,
            position: Vec3::new(2., 2., 0.),
            velocity: Vec3::new(60., 0., -1.),
            seconds: 0.1,
            shape: &s,
            dynamics: &dynamics,
            ignore: 7,
            allowed_restrictions: &[],
            was_grounded: true,
            player_status: None,
        })
        .unwrap();
    assert!(result.position.x <= 3.001);
    assert_eq!(result.contacted_object, Some(8));
    assert!(
        r.validate_placement(1, Vec3::new(5., 2., 0.), &s, &dynamics, 7)
            .is_err()
    );
}
#[test]
fn absent_neighbor_and_invalid_inputs_cannot_produce_accepted_state() {
    let r = region(false);
    let s = shape();
    assert!(
        r.move_body(GeometryStep {
            cell: 1,
            position: Vec3::new(9., 2., 0.),
            velocity: Vec3::new(50., 0., 0.),
            seconds: 0.1,
            shape: &s,
            dynamics: &[],
            ignore: 7,
            allowed_restrictions: &[],
            was_grounded: true,
            player_status: None,
        })
        .is_err()
    );
    assert!(r.validate_placement(2, Vec3::ZERO, &s, &[], 7).is_err());
    assert!(
        r.move_body(GeometryStep {
            cell: 1,
            position: Vec3::new(2., 2., 0.),
            velocity: Vec3::new(f32::NAN, 0., 0.),
            seconds: 0.1,
            shape: &s,
            dynamics: &[],
            ignore: 7,
            allowed_restrictions: &[],
            was_grounded: true,
            player_status: None,
        })
        .is_err()
    );
}
#[test]
fn body_ground_jump_fall_and_failed_teleport_are_atomic() {
    let r = region(false);
    let mut body = Body::spawn_geometry(
        &r,
        GeometrySpawn {
            cell: 1,
            position: Vec3::new(2., 2., 0.),
            shape: std::sync::Arc::new(shape()),
            capabilities: bace_motion::Capabilities {
                speed: 4.,
                jump_impulse: 3.,
            },
            heading: 0.,
            maximum_turn_rate: 2.,
        },
    )
    .unwrap();
    body.step_geometry(&r, 1, 7, &[]).unwrap();
    assert!(body.accepted().grounded());
    assert!(
        body.submit_intent(
            0,
            1,
            bace_motion::MotionIntent::new(Vec3::ZERO, true).unwrap()
        )
        .is_err()
    );
    body.authorize_jump(0.46).unwrap();
    body.step_geometry(&r, 1, 7, &[]).unwrap();
    assert!(body.accepted().position().z > 0.0);
    let prior = body.accepted();
    assert!(body.teleport_geometry(&r, 2, Vec3::ZERO, 7, &[]).is_err());
    assert_eq!(body.accepted(), prior);
    for _ in 0..60 {
        body.step_geometry(&r, 1, 7, &[]).unwrap();
    }
    assert!(body.accepted().grounded());
    assert!(body.accepted().position().z.abs() < 0.001);
}
#[test]
fn portal_neighbor_wall_and_actor_are_swept_before_crossing() {
    let mut left = region(false).cell(1).unwrap().clone();
    left.boundary[1].distance = 5.;
    let mut right = left.clone();
    right.id = 2;
    right.boundary[0].distance = -5.;
    right.boundary[1].distance = 10.;
    let portal = GdlePolygon::prepare(vec![
        Vec3::new(5., 0., 0.),
        Vec3::new(5., 10., 0.),
        Vec3::new(5., 10., 10.),
        Vec3::new(5., 0., 10.),
    ])
    .unwrap();
    left.portals.push(CellPortalGeometry {
        destination: 2,
        polygon: portal,
        translation: Vec3::ZERO,
    });
    let r = GeometryRegion::prepare(vec![left, right]).unwrap();
    let s = CollisionShape::prepare(
        vec![CollisionSphere {
            center: Vec3::new(0., 0., 0.2),
            radius: 0.2,
        }],
        0.0,
        0.1,
    )
    .unwrap();
    let input = |dynamics| GeometryStep {
        cell: 1,
        position: Vec3::new(4., 2., 0.),
        velocity: Vec3::new(30., 0., -1.),
        seconds: 0.1,
        shape: &s,
        dynamics,
        ignore: 7,
        allowed_restrictions: &[],
        was_grounded: true,
        player_status: None,
    };
    let free = r.move_body(input(&[])).unwrap();
    assert_eq!(free.cell, 2);
    assert!(free.position.x > 6.0);
    let dynamic = [DynamicSphere {
        object: 9,
        cell: 2,
        sphere: CollisionSphere {
            center: Vec3::new(5.5, 2., 0.2),
            radius: 0.2,
        },
        cylinder_height: None,
        player_status: None,
    }];
    let blocked = r.move_body(input(&dynamic)).unwrap();
    assert!(blocked.position.x <= 5.101);
    assert_eq!(blocked.contacted_object, Some(9));
}
#[test]
fn npk_passes_players_but_pk_pk_lite_and_free_remain_physical() {
    assert!(!players_collide(Some(2), Some(2)));
    assert!(!players_collide(Some(2), Some(4)));
    assert!(!players_collide(Some(4), Some(0x40)));
    assert!(players_collide(Some(4), Some(4)));
    assert!(players_collide(Some(0x40), Some(0x40)));
    assert!(players_collide(Some(0x20), Some(2)));
    assert!(players_collide(None, Some(2)));
    let r = region(false);
    let s = shape();
    let others = [DynamicSphere {
        object: 8,
        cell: 1,
        sphere: CollisionSphere {
            center: Vec3::new(5., 2., 1.),
            radius: 1.,
        },
        cylinder_height: None,
        player_status: Some(2),
    }];
    let next = r
        .move_body(GeometryStep {
            cell: 1,
            position: Vec3::new(2., 2., 0.),
            velocity: Vec3::new(30., 0., -1.),
            seconds: 0.1,
            shape: &s,
            dynamics: &others,
            ignore: 7,
            allowed_restrictions: &[],
            was_grounded: true,
            player_status: Some(2),
        })
        .unwrap();
    assert!(next.position.x > 4.9);
}
