use bace_geometry::Vec3;
use bace_magic::{ProjectileActorFrame, ProjectileShape, ProjectileSpec, gdle_projectiles};

#[test]
fn original_gdle_ring_spread_and_three_dimensional_group_vectors() {
    for row in include_str!("fixtures/multishot.csv")
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
        let target = if p[2] != 0. {
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
        let (dims, spread) = match p[4] as u32 {
            0 => ([8, 1, 1], 360.),
            1 => ([3, 2, 2], 0.),
            2 => ([3, 1, 1], 90.),
            _ => ([16, 1, 1], 360.),
        };
        let count = dims.iter().product();
        let spec = ProjectileSpec {
            template: 1,
            shape: ProjectileShape::Ring,
            count,
            radius: 0.15,
            speed: 20.,
            gravity: if p[0] == 0. { 0. } else { 9.8 },
            tracking: p[1] != 0.,
            perturbation: Vec3::new(0.2, 0.3, 0.4),
            lifetime: 30.,
            spread_degrees: spread,
            padding: Vec3::new(0.4, 0.5, 0.6),
            offset: Vec3::new(0.1, 0.2, -0.1),
            dimensions: dims,
            minimum_damage: 0,
            maximum_damage: 0,
            damage_type: 0,
            enchantment: None,
        };
        let launches = gdle_projectiles(
            &spec,
            source,
            target,
            p[2] != 0.,
            &vec![Vec3::new(0.25, 0.25, 0.25); usize::from(count)],
        )
        .unwrap();
        let index = p[5] as usize * usize::from(dims[1] * dims[2])
            + p[6] as usize * usize::from(dims[2])
            + p[7] as usize;
        let launch = launches[index];
        for (actual, expected) in [
            launch.position.x,
            launch.position.y,
            launch.position.z,
            launch.velocity.x,
            launch.velocity.y,
            launch.velocity.z,
        ]
        .into_iter()
        .zip(&p[8..])
        {
            assert!(
                (actual - expected).abs() <= 0.00002,
                "{row}: {actual} != {expected}"
            );
        }
    }
}
