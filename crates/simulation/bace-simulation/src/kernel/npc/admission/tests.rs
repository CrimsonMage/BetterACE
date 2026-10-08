use super::*;
use bace_entity::{Actor, Combatant, CombatantProfile, EntityProperties};
use bace_geometry::{Aabb, Vec3};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::CellId;
use std::sync::Arc;
fn fixture() -> (Kernel, crate::npc::PreparedNpcScriptSource) {
    let mut world = bace_world::World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-10., -10., -1.), Vec3::new(10., 10., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn_oriented(
            world.scene(cell).unwrap(),
            Vec3::new(id as f32, 0., 0.5),
            0.5,
            Capabilities {
                speed: 1.,
                jump_impulse: 1.,
            },
            0.,
            1.,
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell,
                body,
            })
            .unwrap();
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 10,
                    melee_damage: 1,
                    melee_range: 2.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: id == 1,
                })
                .unwrap(),
            )
            .unwrap();
    }
    let kernel = Kernel::new(world, 8).unwrap();
    let script = crate::npc::PreparedNpcScriptSource {
        identity: crate::npc::NpcScriptIdentity {
            template: 100,
            program_hash: [1; 32],
            content_generation: [2; 32],
        },
        program: Arc::new(bace_emotes::NativeProgram::prepare(vec![], Default::default()).unwrap()),
        properties: EntityProperties::new(16).unwrap(),
        use_radius: 3.,
    };
    (kernel, script)
}
#[test]
fn admitted_source_is_not_targetable_until_exact_definition_bind_and_ready() {
    let (mut kernel, script) = fixture();
    let identity = script.identity;
    kernel
        .preflight_npc_scripts(std::iter::once((EntityId(2), &script)))
        .unwrap();
    kernel.admit_npc_script(EntityId(2), script).unwrap();
    assert_eq!(
        kernel
            .world
            .attack_geometry(EntityId(1), EntityId(2), 3.0)
            .unwrap(),
        (false, false)
    );
    assert!(kernel.acknowledge_npc_recovery_ready(EntityId(2)).is_err());
    let mut wrong = identity;
    wrong.program_hash = [3; 32];
    assert!(kernel.npcs.bind_admitted(EntityId(2), wrong).is_err());
    assert!(kernel.world.npc_admission_hold(EntityId(2)).is_some());
    kernel.npcs.bind_admitted(EntityId(2), identity).unwrap();
    kernel.acknowledge_npc_recovery_ready(EntityId(2)).unwrap();
    assert!(kernel.world.npc_admission_hold(EntityId(2)).is_none());
    assert_eq!(
        kernel
            .world
            .attack_geometry(EntityId(1), EntityId(2), 3.0)
            .unwrap(),
        (true, true)
    );
    assert!(kernel.rollback_npc_script_admission(EntityId(2)).is_err());
}
#[test]
fn rejected_batch_can_remove_only_fresh_dormant_script_and_its_own_hold() {
    let (mut kernel, script) = fixture();
    kernel.admit_npc_script(EntityId(2), script).unwrap();
    kernel.rollback_npc_script_admission(EntityId(2)).unwrap();
    assert!(kernel.world.npc_admission_hold(EntityId(2)).is_none());
    assert!(!kernel.npc_admission_pending(EntityId(2)));
    kernel.rollback_npc_script_admission(EntityId(2)).unwrap();
}
