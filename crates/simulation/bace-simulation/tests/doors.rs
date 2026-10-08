use std::sync::Arc;

use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::Actor;
use bace_entity::{Combatant, CombatantProfile};
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
};
use bace_gameplay_api::{DoorChange, DoorRejection, UseDoor};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_simulation::{Command, Kernel};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;

fn kernel(commands: usize, characters: usize, outcomes: usize) -> Kernel {
    let mut world = World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.0,
                Aabb::new(Vec3::new(-10.0, -10.0, -1.0), Vec3::new(10.0, 10.0, 10.0)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0.0, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
    }
    for id in 1..=2 {
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 20,
                    melee_range: 2.0,
                    attack_duration: 0.1,
                    strike_offsets: vec![0.05],
                    player: id == 1,
                })
                .unwrap(),
            )
            .unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, commands, characters, outcomes).unwrap();
    kernel.register_character(binding(), character()).unwrap();
    kernel
}

fn character() -> CharacterProgression {
    let table = RankTable::new(&[0, 1, 10, 100]).unwrap();
    CharacterProgression::new(
        &[TraitProgress {
            target: ProgressionTarget::Attribute(AttributeId::Strength),
            experience_spent: 0,
            advancement: SkillAdvancement::Inactive,
        }],
        Arc::new(ProgressionTables {
            attributes: table.clone(),
            vitals: table.clone(),
            trained_skills: table.clone(),
            specialized_skills: table,
        }),
        100,
        0,
    )
    .unwrap()
}

fn binding() -> CharacterBinding {
    CharacterBinding {
        session: SessionId(9),
        account: AccountId(4),
        actor: EntityId(1),
    }
}

fn context(sequence: u32) -> ActionContext {
    let binding = binding();
    ActionContext {
        session: binding.session,
        account: binding.account,
        actor: binding.actor,
        sequence,
    }
}

use bace_simulation::{PreparedDoor, PreparedDoorAnimation, PreparedDoorHook};
use bace_world::DoorCollider;
fn door() -> PreparedDoor {
    PreparedDoor {
        collider: DoorCollider {
            cell: CellId(1),
            position: Vec3::new(3.1, 0.0, 0.5),
            bounds: Aabb::new(Vec3::new(3.0, -1.0, 0.0), Vec3::new(3.2, 1.0, 2.0)).unwrap(),
            solid: true,
        },
        initially_open: false,
        initially_locked: false,
        reset_interval_ticks: None,
        use_radius: 5.0,
        open: PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![PreparedDoorHook {
                offset_ticks: 1,
                ethereal: true,
            }],
        },
        close: PreparedDoorAnimation {
            duration_ticks: 2,
            hooks: vec![PreparedDoorHook {
                offset_ticks: 1,
                ethereal: false,
            }],
        },
    }
}
fn use_door(sequence: u32) -> Command {
    Command::UseDoor {
        context: context(sequence),
        request: UseDoor { door: EntityId(10) },
    }
}
fn tick(kernel: &mut Kernel) {
    kernel.step().unwrap();
    while kernel.take_door_event().is_some() {}
}
#[test]
fn authenticated_door_use_drives_collision_and_occupied_close_retries() {
    let mut kernel = kernel(16, 2, 16);
    kernel.register_door(EntityId(10), door()).unwrap();
    assert!(
        !kernel
            .world()
            .scene(CellId(1))
            .unwrap()
            .segment_clear(Vec3::new(1.0, 0.0, 0.5), Vec3::new(5.0, 0.0, 0.5))
    );
    kernel.enqueue(use_door(1)).unwrap();
    tick(&mut kernel);
    assert!(matches!(
        kernel.take_door_outcome().unwrap().result,
        Ok(DoorChange::Motion { open: true, .. })
    ));
    assert!(kernel.world().door(EntityId(10)).unwrap().solid);
    tick(&mut kernel);
    tick(&mut kernel);
    assert!(!kernel.world().door(EntityId(10)).unwrap().solid);
    kernel
        .enqueue(Command::ServerTeleport {
            actor: EntityId(1),
            cell: CellId(1),
            position: Vec3::new(3.1, 0.0, 0.5),
        })
        .unwrap();
    kernel.enqueue(use_door(2)).unwrap();
    tick(&mut kernel);
    tick(&mut kernel);
    tick(&mut kernel);
    assert!(!kernel.world().door(EntityId(10)).unwrap().solid);
    assert!(kernel.door_physics(EntityId(10)).unwrap().retry_solidity);
    kernel
        .enqueue(Command::ServerTeleport {
            actor: EntityId(1),
            cell: CellId(1),
            position: Vec3::new(5.0, 0.0, 0.5),
        })
        .unwrap();
    tick(&mut kernel);
    assert!(kernel.world().door(EntityId(10)).unwrap().solid);
    assert!(!kernel.door_physics(EntityId(10)).unwrap().retry_solidity);
}
#[test]
fn bounded_door_outputs_retain_auth_sequence_and_reject_replay() {
    let mut kernel = kernel(8, 2, 1);
    kernel.register_door(EntityId(10), door()).unwrap();
    kernel.enqueue(use_door(1)).unwrap();
    kernel.enqueue(use_door(2)).unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 1);
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 1);
    kernel.take_door_outcome().unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 1);
    kernel.take_door_event().unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.queued_commands(), 0);
    assert_eq!(
        kernel.take_door_outcome().unwrap().result,
        Ok(DoorChange::Busy)
    );
    while kernel.take_door_event().is_some() {}
    kernel.enqueue(use_door(2)).unwrap();
    tick(&mut kernel);
    assert_eq!(
        kernel.take_door_outcome().unwrap().result,
        Err(DoorRejection::StaleSequence)
    );
}
#[test]
fn actual_npc_sweep_contact_opens_unlocked_door_but_player_contact_does_not() {
    let mut kernel = kernel(16, 2, 16);
    kernel.register_door(EntityId(10), door()).unwrap();
    kernel
        .enqueue(Command::Movement {
            actor: EntityId(1),
            epoch: 0,
            sequence: 1,
            intent: bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        })
        .unwrap();
    for _ in 0..20 {
        tick(&mut kernel);
    }
    assert!(kernel.world().door(EntityId(10)).unwrap().solid);
    assert!(
        kernel
            .world()
            .body(EntityId(1))
            .unwrap()
            .accepted()
            .position()
            .x
            < 3.0
    );
    kernel
        .enqueue(Command::Movement {
            actor: EntityId(2),
            epoch: 0,
            sequence: 1,
            intent: bace_motion::MotionIntent::new(Vec3::new(1.0, 0.0, 0.0), false).unwrap(),
        })
        .unwrap();
    let mut opened = false;
    for _ in 0..10 {
        kernel.step().unwrap();
        while let Some(event) = kernel.take_door_event() {
            opened |= event.motion.is_some_and(|m| m.open && !m.autonomous);
        }
    }
    assert!(opened);
    assert!(!kernel.world().door(EntityId(10)).unwrap().solid);
}
