use bace_geometry::Vec3;
use bace_physics::*;
fn polygon() -> GdlePolygon {
    GdlePolygon::prepare(vec![
        Vec3::new(-2.0, -2.0, 0.0),
        Vec3::new(2.0, -2.0, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
        Vec3::new(-2.0, 2.0, 0.0),
    ])
    .unwrap()
}
fn sphere() -> CollisionSphere {
    CollisionSphere {
        center: Vec3::ZERO,
        radius: 10.0,
    }
}
fn leaf() -> BspCollisionNode {
    BspCollisionNode::Leaf {
        bounds: sphere(),
        solid: false,
        polygons: vec![0],
    }
}
#[test]
fn malformed_graphs_planes_polygons_and_query_budgets_fail_closed() {
    let plane = polygon().plane();
    for nodes in [
        vec![
            BspCollisionNode::Branch {
                bounds: sphere(),
                plane,
                positive: 0,
                negative: 1,
            },
            leaf(),
        ],
        vec![
            BspCollisionNode::Branch {
                bounds: sphere(),
                plane,
                positive: 1,
                negative: 9,
            },
            leaf(),
        ],
        vec![
            BspCollisionNode::Branch {
                bounds: sphere(),
                plane,
                positive: 1,
                negative: 1,
            },
            leaf(),
        ],
        vec![leaf(), leaf()],
        vec![BspCollisionNode::Leaf {
            bounds: sphere(),
            solid: false,
            polygons: vec![1],
        }],
    ] {
        assert!(GdleBspCell::prepare(nodes, vec![polygon()], BspLimits::default()).is_err());
    }
    assert!(GdlePolygon::prepare(vec![Vec3::ZERO; 3]).is_err());
    assert!(GdlePolygon::prepare(vec![Vec3::new(f32::NAN, 0.0, 0.0); 3]).is_err());
    let cell = GdleBspCell::prepare(vec![leaf()], vec![polygon()], BspLimits::default()).unwrap();
    let query = CollisionSphere {
        center: Vec3::new(0.0, 0.0, 0.1),
        radius: 0.5,
    };
    assert_eq!(
        cell.intersects_solid(query, true, &mut BspQueryBudget::new(0, 100, 100)),
        Err(BspQueryError::Limit)
    );
    assert_eq!(
        cell.sphere_contact(
            query,
            Vec3::new(0.0, 0.0, -1.0),
            &mut BspQueryBudget::new(100, 0, 100)
        ),
        Err(BspQueryError::Limit)
    );
    assert!(
        sphere_path_steps(
            CollisionSphere {
                radius: 0.0,
                ..query
            },
            Vec3::ZERO,
            100
        )
        .is_err()
    );
    assert_eq!(
        cell.sample_path(
            CollisionSphere {
                center: Vec3::new(0.0, 0.0, 2.0),
                radius: 0.5
            },
            Vec3::new(0.0, 0.0, -2.0),
            &mut BspQueryBudget::new(100, 100, 2)
        ),
        Err(BspQueryError::Limit)
    );
}
#[test]
fn sampled_path_uses_authored_polygon_and_preserves_away_contact() {
    let cell = GdleBspCell::prepare(vec![leaf()], vec![polygon()], BspLimits::default()).unwrap();
    let contact = cell
        .sample_path(
            CollisionSphere {
                center: Vec3::new(0.0, 0.0, 2.0),
                radius: 0.5,
            },
            Vec3::new(0.0, 0.0, -2.0),
            &mut BspQueryBudget::default(),
        )
        .unwrap()
        .unwrap();
    assert_eq!((contact.step, contact.total_steps), (4, 8));
    assert_eq!(contact.center, Vec3::ZERO);
    assert!(contact.contact.unwrap().approaching);
    let away = cell
        .sphere_contact(
            CollisionSphere {
                center: Vec3::new(0.0, 0.0, 0.1),
                radius: 0.5,
            },
            Vec3::new(0.0, 0.0, 1.0),
            &mut BspQueryBudget::default(),
        )
        .unwrap()
        .unwrap();
    assert!(!away.approaching);
    assert!(
        cell.sample_path(
            CollisionSphere {
                center: Vec3::new(3.0, 0.0, 2.0),
                radius: 0.5
            },
            Vec3::new(3.0, 0.0, -2.0),
            &mut BspQueryBudget::default()
        )
        .unwrap()
        .is_none()
    );
}
