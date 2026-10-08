use bace_geometry::Vec3;
use bace_magic::{ProjectileActorFrame, ProjectileShape, ProjectileSpec, ace_strike_projectiles};
#[test]
fn reviewed_ace_strike_origins_target_placement_and_velocity_vectors() {
    for row in include_str!("fixtures/strike.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let p: Vec<f32> = row.split(',').map(|s| s.parse().unwrap()).collect();
        let source = ProjectileActorFrame {
            position: Vec3::new(1., 2., 0.),
            height: 1.9,
            radius: 0.5,
            heading: p[1],
            velocity: Vec3::ZERO,
        };
        let target = ProjectileActorFrame {
            position: Vec3::new(10., 30., p[4] * 3.),
            height: 1.8,
            radius: 0.5,
            heading: 0.,
            velocity: Vec3::new(3., -1., 0.5),
        };
        let count = p[0] as u16;
        let spec = ProjectileSpec {
            template: 1,
            shape: ProjectileShape::Strike,
            count,
            radius: 0.15,
            speed: 20.,
            gravity: 0.,
            tracking: p[2] != 0.,
            perturbation: if p[3] != 0. {
                Vec3::new(0.2, 0.3, 0.4)
            } else {
                Vec3::ZERO
            },
            lifetime: 30.,
            spread_degrees: 0.,
            padding: Vec3::new(0.4, 0.5, 0.6),
            offset: Vec3::new(0.1, 0.2, -0.1),
            dimensions: [count, 1, 1],
            minimum_damage: 0,
            maximum_damage: 0,
            damage_type: 0,
            enchantment: None,
        };
        let launches = ace_strike_projectiles(
            &spec,
            source,
            target,
            &vec![Vec3::new(0.25, 0.25, 0.25); usize::from(count)],
        )
        .unwrap();
        let l = launches[p[5] as usize];
        for (actual, expected) in [
            l.position.x,
            l.position.y,
            l.position.z,
            l.velocity.x,
            l.velocity.y,
            l.velocity.z,
        ]
        .into_iter()
        .zip(&p[6..])
        {
            assert!(
                (actual - expected).abs() <= 0.00006,
                "{row}: {actual} != {expected}"
            );
        }
    }
}
