pub(crate) use bace_character::{
    CharacterProgression, ProgressionTables, RankTable, TraitProgress,
};
pub(crate) use bace_entity::{Actor, Combatant, CombatantProfile, EntityVital, VitalPool};
pub(crate) use bace_gameplay_api::{
    ActionContext, AttributeId, CastRequest, CharacterBinding, ProgressionTarget, SessionId,
    SkillAdvancement,
};
pub(crate) use bace_geometry::{Aabb, Vec3};
pub(crate) use bace_magic::{
    CastGesture, MagicSchool, PreparedSpell, ProjectileShape, ProjectileSpec, SpellEffect,
};
pub(crate) use bace_motion::Capabilities;
pub(crate) use bace_physics::{Body, SyntheticScene};
pub(crate) use bace_simulation::{
    Command, Kernel, MagicCaster, MagicEvent, PreparedCastGesture, PreparedMagicSpell,
};
pub(crate) use bace_types::{AccountId, CellId, EntityId};
pub(crate) use bace_world::World;
pub(crate) use std::{collections::BTreeSet, sync::Arc};
pub(crate) fn context(sequence: u32) -> ActionContext {
    ActionContext {
        session: SessionId(7),
        account: AccountId(1),
        actor: EntityId(1),
        sequence,
    }
}
pub(crate) fn kernel(capacity: usize) -> Kernel {
    kernel_with_components(capacity, false)
}
pub(crate) fn kernel_with_components(capacity: usize, required: bool) -> Kernel {
    kernel_fixture(capacity, required, false)
}
pub(crate) fn kernel_fixture(capacity: usize, required: bool, geometry: bool) -> Kernel {
    kernel_fixture_mode(capacity, required, geometry, true)
}
pub(crate) fn kernel_fixture_mode(
    capacity: usize,
    required: bool,
    geometry: bool,
    synthetic_timing: bool,
) -> Kernel {
    kernel_fixture_heading(capacity, required, geometry, synthetic_timing, 0.0)
}
#[allow(dead_code, reason = "shared fixture used by ring qualification")]
pub(crate) fn kernel_fixture_heading(
    capacity: usize,
    required: bool,
    geometry: bool,
    synthetic_timing: bool,
    heading: f32,
) -> Kernel {
    kernel_fixture_schools(
        capacity,
        required,
        geometry,
        synthetic_timing,
        heading,
        None,
        8,
    )
}
#[allow(dead_code, reason = "shared optional distinct-school fixture")]
pub(crate) fn kernel_with_school_values(capacity: usize, schools: [u32; 5]) -> Kernel {
    kernel_fixture_schools(capacity, false, false, true, 0.0, Some(schools), 8)
}
#[allow(
    dead_code,
    reason = "shared fixture also qualifies noncombat recall actions"
)]
pub(crate) fn kernel_in_mode(capacity: usize, mode: u32) -> Kernel {
    kernel_fixture_schools(capacity, false, false, true, 0.0, None, mode)
}
fn kernel_fixture_schools(
    capacity: usize,
    required: bool,
    geometry: bool,
    synthetic_timing: bool,
    heading: f32,
    schools: Option<[u32; 5]>,
    mode: u32,
) -> Kernel {
    let mut world = World::default();
    if geometry {
        // Authored primitive fixture, explicitly not a proprietary DAT asset.
        pub(crate) use bace_physics::{CollisionFace, GdlePolygon, GeometryCell, GeometryRegion};
        let floor = CollisionFace {
            polygon: GdlePolygon::prepare(vec![
                Vec3::new(-100., -100., 0.),
                Vec3::new(100., -100., 0.),
                Vec3::new(100., 100., 0.),
                Vec3::new(-100., 100., 0.),
            ])
            .unwrap(),
            two_sided: true,
            object: None,
        };
        world
            .install_geometry(Arc::new(
                GeometryRegion::prepare(vec![GeometryCell {
                    id: 1,
                    restriction: None,
                    terrain: false,
                    solids: vec![],
                    static_primitives: vec![],
                    boundary: vec![
                        bace_physics::CollisionPlane {
                            normal: Vec3::new(1., 0., 0.),
                            distance: 100.,
                        },
                        bace_physics::CollisionPlane {
                            normal: Vec3::new(-1., 0., 0.),
                            distance: 100.,
                        },
                        bace_physics::CollisionPlane {
                            normal: Vec3::new(0., 1., 0.),
                            distance: 100.,
                        },
                        bace_physics::CollisionPlane {
                            normal: Vec3::new(0., -1., 0.),
                            distance: 100.,
                        },
                    ],
                    faces: vec![floor],
                    portals: vec![],
                }])
                .unwrap(),
            ))
            .unwrap();
    }
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
            heading,
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
        combatant.set_mode(mode);
        world.register_combatant(EntityId(id), combatant).unwrap();
    }
    let mut kernel = Kernel::with_gameplay_limits(world, 32, 2, capacity).unwrap();
    if synthetic_timing {
        kernel.enable_synthetic_cast_timing();
    }
    let table = RankTable::new(&[0, 10, 100]).unwrap();
    let character = if let Some(schools) = schools {
        let states = [34, 33, 31, 32, 43, 15, 16, 44]
            .into_iter()
            .enumerate()
            .map(|(i, id)| bace_character::TraitState {
                progress: TraitProgress {
                    target: ProgressionTarget::Skill(id),
                    experience_spent: 0,
                    advancement: SkillAdvancement::Trained,
                },
                details: bace_gameplay_api::TraitDetails::Skill {
                    initial_level: schools.get(i).copied().unwrap_or(100),
                    resistance_at_last_check: 0,
                    last_used_time: 0.,
                },
            })
            .collect::<Vec<_>>();
        CharacterProgression::with_state(
            &states,
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
    } else {
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
    };
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
    for actor in [EntityId(1), EntityId(2)] {
        kernel
            .register_magic_caster(
                actor,
                MagicCaster {
                    player: actor == EntityId(1),
                    known_spells: BTreeSet::from([100, 101, 102]),
                    school_skills: [1000; 5],
                    magic_defense: 0,
                    mana_conversion: 0,
                    components_required: required && actor == EntityId(1),
                    safe_components: false,
                },
            )
            .unwrap();
    }
    if schools.is_some() {
        let input = bace_character::SkillValueInputs {
            formula: bace_character::VitalFormula {
                enabled: false,
                divisor: 1,
                attribute1: 0,
                attribute2: 0,
            },
            usable_untrained: true,
            base_attributes: [0; 6],
            current_attributes: [0; 6],
            bonuses: bace_character::SkillBonuses::default(),
            multiplier: 1.,
            vitae: 1.,
            additive: 0,
        };
        kernel
            .register_character_skill_inputs(
                EntityId(1),
                bace_simulation::PreparedCharacterSkillInputs {
                    attack_skill: 44,
                    inputs: [34, 33, 31, 32, 43, 15, 16, 44]
                        .into_iter()
                        .map(|id| (id, input))
                        .collect(),
                    shield: None,
                    attribute_modifiers: [bace_simulation::PreparedAttributeModifier::default(); 6],
                },
            )
            .unwrap();
    }
    kernel
}
pub(crate) fn spell(id: u32, effect: SpellEffect) -> PreparedMagicSpell {
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
pub(crate) fn step(kernel: &mut Kernel) -> Vec<MagicEvent> {
    kernel.step().unwrap();
    let mut events = Vec::new();
    while let Some(event) = kernel.take_magic_event() {
        events.push(event);
    }
    events
}
#[allow(dead_code, reason = "shared fixture used by projectile suites")]
pub(crate) fn shot(shape: ProjectileShape, count: u16) -> ProjectileSpec {
    ProjectileSpec {
        template: 9000,
        shape,
        count,
        radius: 0.1,
        speed: 30.0,
        gravity: 0.0,
        tracking: false,
        perturbation: Vec3::ZERO,
        lifetime: 1.0,
        spread_degrees: if shape == ProjectileShape::Ring {
            360.0
        } else {
            0.0
        },
        padding: Vec3::ZERO,
        offset: Vec3::ZERO,
        dimensions: [count, 1, 1],
        minimum_damage: 20,
        maximum_damage: 20,
        damage_type: 16,
        enchantment: None,
    }
}
