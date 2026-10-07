//! Reproducible synthetic fixture, deliberately separate from production content.
use crate::{Command, Kernel};
use bace_entity::Actor;
use bace_geometry::{Aabb, Vec3};
use bace_motion::{Capabilities, MotionIntent};
use bace_physics::{Body, SyntheticScene};
use bace_types::{CellId, EntityId};
use bace_world::World;

pub fn synthetic_scenario(players: u32, bodies: u32) -> Result<Kernel, Box<dyn std::error::Error>> {
    if players == 0 || players > 4096 || bodies > 10000 {
        return Err("synthetic scenario entity limits exceeded".into());
    }
    let cell = CellId(0x0001_0001);
    let bounds = Aabb::new(
        Vec3::new(-1000.0, -1000.0, -1.0),
        Vec3::new(1000.0, 1000.0, 1000.0),
    )
    .ok_or("invalid fixture bounds")?;
    let obstacle = Aabb::new(Vec3::new(50.0, -500.0, 0.0), Vec3::new(51.0, 500.0, 50.0))
        .ok_or("invalid fixture wall")?;
    let scene = SyntheticScene::new(0.0, bounds, vec![obstacle])?;
    let mut world = World::default();
    world.register_scene(cell, scene)?;
    for index in 0..players + bodies {
        let position = Vec3::new(
            -500.0 + (index % 100) as f32 * 2.0,
            -500.0 + (index / 100) as f32 * 2.0,
            0.5,
        );
        let body = Body::spawn(
            world.scene(cell)?,
            position,
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )?;
        world.insert(Actor {
            id: EntityId(index + 1),
            cell,
            body,
        })?;
    }
    let mut kernel = Kernel::new(world, 8192)?;
    for index in 0..players {
        kernel.enqueue(Command::Movement {
            actor: EntityId(index + 1),
            epoch: 0,
            sequence: 0,
            intent: MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false)?,
        })?;
    }
    Ok(kernel)
}
