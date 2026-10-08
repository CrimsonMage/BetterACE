use bace_geometry::Vec3;
use bace_magic::{ProjectileActorFrame, ProjectileShape, ProjectileSpec, gdle_single_projectile};
#[test]
fn pinned_gdle_spawn_frame_perturbation_and_velocity_vectors() {
    for row in include_str!("fixtures/launch.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<f32> = row.split(',').map(|s| s.parse().unwrap()).collect();
        let source = ProjectileActorFrame {
            position: Vec3::new(1., 2., 0.),
            height: 1.9,
            radius: 0.5,
            heading: p[3],
            velocity: Vec3::ZERO,
        };
        let target = if p[2] != 0.0 {
            source
        } else {
            ProjectileActorFrame {
                position: Vec3::new(10., 30., 3.),
                height: 1.8,
                radius: 0.5,
                heading: 0.,
                velocity: Vec3::new(3., -1., 0.5),
            }
        };
        let spec = ProjectileSpec {
            template: 1,
            shape: if p[0] == 0.0 {
                ProjectileShape::Bolt
            } else {
                ProjectileShape::Arc
            },
            count: 1,
            radius: 0.15,
            speed: 20.0,
            gravity: if p[0] == 0.0 { 0.0 } else { 9.8 },
            tracking: p[1] != 0.,
            perturbation: if p[4] != 0. {
                Vec3::new(1., 1., 1.)
            } else {
                Vec3::ZERO
            },
            lifetime: 30.,
            spread_degrees: 0.,
            padding: Vec3::new(0.2, 0.3, 0.4),
            offset: if p[5] != 0. {
                Vec3::new(0.1, 0.2, -0.1)
            } else {
                Vec3::ZERO
            },
            dimensions: [1, 1, 1],
            minimum_damage: 0,
            maximum_damage: 0,
            damage_type: 0,
            enchantment: None,
        };
        let result = gdle_single_projectile(
            &spec,
            source,
            target,
            p[2] != 0.,
            Vec3::new(-0.5, 0.25, 0.75),
        )
        .unwrap();
        for (actual, expected) in [
            result.position.x,
            result.position.y,
            result.position.z,
            result.velocity.x,
            result.velocity.y,
            result.velocity.z,
        ]
        .into_iter()
        .zip(&p[6..])
        {
            assert!(
                (actual - *expected).abs() <= 0.00001,
                "{row}: {actual} != {expected}"
            );
        }
    }
}
