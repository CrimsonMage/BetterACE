use bace_geometry::Vec3;
use bace_physics::{ProjectileBody, ProjectileStep};
use serde_json::Value;
fn vector(v: &Value) -> Vec3 {
    Vec3::new(
        v[0].as_f64().unwrap() as f32,
        v[1].as_f64().unwrap() as f32,
        v[2].as_f64().unwrap() as f32,
    )
}
fn bits(v: Vec3) -> [u32; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}
fn expected(v: &Value) -> [u32; 3] {
    std::array::from_fn(|i| v[i].as_u64().unwrap() as u32)
}
#[test]
fn pinned_gdle_airborne_projectile_integrator_and_speed_ceiling() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_projectile.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "353cbab52ef7da2b7063bc3e3f008461d8531693"
    );
    for case in fixture["vectors"]["flight"].as_array().unwrap() {
        let mut body = ProjectileBody::new(
            Vec3::new(1.0, -2.0, 3.0),
            vector(&case["velocity"]),
            0.1,
            case["gravity"].as_f64().unwrap() as f32,
            600.0,
        )
        .unwrap();
        assert_eq!(bits(body.velocity()), expected(&case["initial_velocity"]));
        for (step, expected_step) in case["steps"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                body.step(case["dt"].as_f64().unwrap() as f32, None)
                    .unwrap(),
                ProjectileStep::Flying
            );
            assert_eq!(
                bits(body.position()),
                expected(&expected_step["position"]),
                "case={case} step={step}"
            );
            assert_eq!(
                bits(body.velocity()),
                expected(&expected_step["velocity"]),
                "step={step}"
            );
        }
    }
}
#[test]
fn pinned_gdle_cross_cell_and_landblock_coordinate_offsets() {
    use bace_physics::{GeometryCell, GeometryRegion};
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/gdle_projectile.json")).unwrap();
    let cell = |id| GeometryCell {
        id,
        terrain: false,
        restriction: None,
        solids: vec![],
        static_primitives: vec![],
        boundary: vec![bace_physics::CollisionPlane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            distance: 100.0,
        }],
        faces: vec![],
        portals: vec![],
    };
    for case in fixture["vectors"]["frames"].as_array().unwrap() {
        let from = case["from"].as_u64().unwrap() as u32;
        let to = case["to"].as_u64().unwrap() as u32;
        let region = GeometryRegion::prepare(vec![cell(from), cell(to)]).unwrap();
        if from >> 16 != to >> 16 {
            assert!(region.frame_offset(from, to).is_err());
        }
        let region = region
            .with_landblock_metric(
                case["square"].as_f64().unwrap() as f32,
                case["side"].as_i64().unwrap() as i32,
            )
            .unwrap();
        let offset = region.frame_offset(from, to).unwrap();
        assert_eq!(bits(offset), expected(&case["offset"]));
        assert_eq!(
            bits(offset + Vec3::new(3.0, 19.0, 7.0) - Vec3::new(190.0, 17.0, 4.0)),
            expected(&case["delta"])
        );
    }
}
