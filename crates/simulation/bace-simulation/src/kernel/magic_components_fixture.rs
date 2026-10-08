//! Local synthetic fixture for component handoff and registry heartbeat tests.
use crate::{Kernel, MagicCaster, PreparedCastGesture, PreparedMagicSpell};
use bace_character::{CharacterProgression, ProgressionTables, RankTable, TraitProgress};
use bace_entity::{Actor, Combatant, CombatantProfile, VitalPool};
use bace_gameplay_api::{
    ActionContext, AttributeId, CharacterBinding, ProgressionTarget, SessionId, SkillAdvancement,
};
use bace_geometry::{Aabb, Vec3};
use bace_magic::{CastGesture, MagicSchool, PreparedSpell, SpellEffect};
use bace_motion::Capabilities;
use bace_physics::{Body, SyntheticScene};
use bace_types::{AccountId, CellId, EntityId};
use bace_world::World;
use std::{collections::BTreeSet, sync::Arc};
pub(super) fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(7),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
pub(super) fn component_kernel(capacity: usize) -> Kernel {
    let mut world = World::default();
    world
        .register_scene(
            CellId(1),
            SyntheticScene::new(
                0.0,
                Aabb::new(
                    Vec3::new(-100.0, -100.0, -1.0),
                    Vec3::new(100.0, 100.0, 100.0),
                )
                .unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for id in 1..=2 {
        let body = Body::spawn_oriented(
            world.scene(CellId(1)).unwrap(),
            Vec3::new(0.0, if id == 1 { 0.0 } else { 5.0 }, 0.5),
            0.5,
            Capabilities {
                speed: 5.0,
                jump_impulse: 5.0,
            },
            0.0,
            std::f32::consts::PI,
        )
        .unwrap();
        world
            .insert(Actor {
                id: EntityId(id),
                cell: CellId(1),
                body,
            })
            .unwrap();
        let mut combatant = Combatant::new(CombatantProfile {
            maximum_health: 100,
            melee_damage: 1,
            melee_range: 2.0,
            attack_duration: 1.0,
            strike_offsets: vec![0.5],
            player: id == 1,
        })
        .unwrap()
        .with_resources(
            Some(VitalPool {
                current: 100,
                maximum: 100,
            }),
            Some(VitalPool {
                current: 100,
                maximum: 100,
            }),
        )
        .unwrap();
        combatant.damage(50).unwrap();
        combatant.set_mode(8);
        world.register_combatant(EntityId(id), combatant).unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, 32, 2, capacity).unwrap();
    kernel.enable_synthetic_cast_timing();
    let table = RankTable::new(&[0, 10, 100]).unwrap();
    let character = CharacterProgression::new(
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
    .unwrap();
    kernel
        .register_character(
            CharacterBinding {
                session: SessionId(7),
                account: AccountId(1),
                actor: EntityId(1),
            },
            character,
        )
        .unwrap();
    kernel
        .configure_magic_random(bace_random::RandomRoot::new([7; 32], 1).unwrap())
        .unwrap();
    kernel
}
pub(super) fn spell(id: u32, effect: SpellEffect) -> PreparedMagicSpell {
    PreparedMagicSpell {
        spell: PreparedSpell {
            id,
            school: MagicSchool::Life,
            power: 0,
            base_mana: 10,
            range_constant: 30.0,
            range_per_skill: 0.0,
            harmful: false,
            resistable: false,
            effect,
        },
        gestures: vec![PreparedCastGesture {
            gesture: CastGesture {
                motion: 0x13000132,
                minimum_seconds: 0.1,
            },
            duration_seconds: 0.1,
            motion_chain: None,
        }],
        components: vec![],
        component_loss: 0.0,
        component_modifiers: vec![],
        fast_resistable_pk_spell: false,
    }
}

pub(super) fn register_component_caster(kernel: &mut Kernel) {
    kernel
        .register_magic_caster(
            EntityId(1),
            MagicCaster {
                player: true,
                known_spells: BTreeSet::from([100]),
                school_skills: [1000; 5],
                magic_defense: 0,
                mana_conversion: 0,
                components_required: true,
                safe_components: false,
            },
        )
        .unwrap();
}
