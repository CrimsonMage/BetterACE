//! GDLE 353cbab ObjCell.cpp::find_cell_list uses the primary sphere center;
//! EnvCell.cpp::find_visible_child_cell retains authored then visible-cell order.
use bace_geometry::Vec3;
use bace_physics::*;
fn cell(id: u32, low: f32, high: f32) -> GeometryCell {
    GeometryCell {
        id,
        terrain: false,
        restriction: None,
        solids: vec![],
        static_primitives: vec![],
        boundary: vec![
            CollisionPlane {
                normal: Vec3::new(0., 0., 1.),
                distance: 0.,
            },
            CollisionPlane {
                normal: Vec3::new(0., 1., 0.),
                distance: -low,
            },
            CollisionPlane {
                normal: Vec3::new(0., -1., 0.),
                distance: high,
            },
        ],
        faces: vec![CollisionFace {
            polygon: GdlePolygon::prepare(vec![
                Vec3::new(-10., low, 0.),
                Vec3::new(10., low, 0.),
                Vec3::new(10., high, 0.),
                Vec3::new(-10., high, 0.),
            ])
            .unwrap(),
            two_sided: false,
            object: None,
        }],
        portals: vec![],
    }
}
fn portal() -> CollisionShape {
    // Actual portal setup values: origin floor-0.063 touches with the complete
    // sphere; classifying the model origin would incorrectly reject this.
    CollisionShape::prepare(
        vec![CollisionSphere {
            center: Vec3::new(0., 0., 1.42),
            radius: 1.357,
        }],
        0.,
        0.,
    )
    .unwrap()
}
#[test]
fn primary_sphere_cell_membership_retains_full_shape_collision() {
    let region = GeometryRegion::prepare(vec![cell(0x86020100, 0., 10.)]).unwrap();
    let shape = portal();
    let position = Vec3::new(0., 5., -0.063);
    assert_eq!(
        region.placement_cell(0x86020100, position, &shape, &[]),
        Ok(0x86020100)
    );
    region
        .validate_placement(0x86020100, position, &shape, &[], 0)
        .unwrap();
    let below = Vec3::new(0., 5., -0.2);
    assert_eq!(
        region.placement_cell(0x86020100, below, &shape, &[]),
        Ok(0x86020100)
    );
    assert_eq!(
        region.validate_placement(0x86020100, below, &shape, &[], 0),
        Err(GeometryError::Invalid)
    );
}
#[test]
fn authored_hint_uses_only_admitted_visible_cells_without_moving_position() {
    let region =
        GeometryRegion::prepare(vec![cell(0x86020100, 0., 4.), cell(0x86020101, 4., 10.)]).unwrap();
    let shape = portal();
    let position = Vec3::new(0., 7., -0.063);
    assert_eq!(
        region.placement_cell(0x86020100, position, &shape, &[]),
        Err(GeometryError::MissingCell)
    );
    let accepted = region
        .placement_cell(0x86020100, position, &shape, &[0x86020101])
        .unwrap();
    assert_eq!(accepted, 0x86020101);
    region
        .validate_placement(accepted, position, &shape, &[], 0)
        .unwrap();
    assert!(
        region
            .placement_cell(0x86020100, position, &shape, &[0x86020102])
            .is_err()
    );
    assert!(
        region
            .placement_cell(0x86020100, Vec3::new(f32::NAN, 0., 0.), &shape, &[])
            .is_err()
    );
}
