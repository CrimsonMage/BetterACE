use bace_geometry::{Aabb, Vec3};

fn box_fixture() -> Aabb {
    Aabb::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)).unwrap()
}

fn axis_vector(axis: usize, value: f32) -> Vec3 {
    match axis {
        0 => Vec3::new(value, 0.0, 0.0),
        1 => Vec3::new(0.0, value, 0.0),
        _ => Vec3::new(0.0, 0.0, value),
    }
}

#[test]
fn each_tangent_face_allows_separating_and_parallel_motion_but_blocks_entry() {
    let obstacle = box_fixture();
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let start = axis_vector(axis, sign * 1.5);
            assert_eq!(obstacle.sweep(start, axis_vector(axis, sign), 0.5), None);
            assert_eq!(
                obstacle.sweep(start, axis_vector((axis + 1) % 3, 1.0), 0.5),
                None
            );
            assert_eq!(obstacle.sweep(start, Vec3::ZERO, 0.5), None);
            assert_eq!(
                obstacle.sweep(start, axis_vector(axis, -sign), 0.5),
                Some(0.0)
            );
            // A small nonzero inward displacement is not a stationary axis.
            assert_eq!(
                obstacle.sweep(start, axis_vector(axis, -sign * f32::EPSILON / 2.0), 0.5),
                Some(0.0)
            );
        }
    }
}

#[test]
fn sweep_distinguishes_grazing_crossing_and_existing_overlap() {
    let obstacle = box_fixture();
    assert_eq!(
        obstacle.sweep(Vec3::new(-2.0, 1.0, 0.0), Vec3::new(4.0, 0.0, 0.0), 0.0),
        None
    );
    // The line only touches (-1, 1, 0), with no interior interval.
    assert_eq!(
        obstacle.sweep(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), 0.0),
        None
    );
    assert_eq!(
        obstacle.sweep(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0), 0.0),
        Some(0.25)
    );
    assert_eq!(
        obstacle.sweep(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), 0.0),
        Some(0.0)
    );
    assert_eq!(
        obstacle.sweep(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), 0.0),
        Some(1.0)
    );
}
