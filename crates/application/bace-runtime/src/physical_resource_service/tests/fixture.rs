//! Synthetic movement isolates the real ammunition transaction and owner protocol.
use super::*;
use bace_entity::{Actor, Combatant, CombatantProfile, VitalPool};
use bace_gameplay_api::weapon_combat::*;
use bace_geometry::{Aabb, Vec3};
use bace_physics::{Body, SyntheticScene};
use bace_types::CellId;
pub(super) const ACTOR: u32 = 0x50000001;
pub(super) const TARGET: u32 = 0x70000001;
pub(super) const LAUNCHER: u32 = 0x80000001;
pub(super) const AMMO: u32 = 0x80000002;
pub(super) const PROJECTILE: u32 = 0x80000003;
pub(super) fn kernel(
    binding: CharacterBinding,
) -> (bace_simulation::Kernel, PhysicalLaunchProposal) {
    let mut world = bace_world::World::default();
    let cell = CellId(1);
    world
        .register_scene(
            cell,
            SyntheticScene::new(
                0.,
                Aabb::new(Vec3::new(-20., -20., -1.), Vec3::new(20., 20., 10.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    for (id, x, player) in [(ACTOR, 1., true), (TARGET, 2., false)] {
        let body = Body::spawn(
            world.scene(cell).unwrap(),
            Vec3::new(x, 0., 0.5),
            0.5,
            bace_motion::Capabilities {
                speed: 0.,
                jump_impulse: 0.,
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
        world
            .register_combatant(
                EntityId(id),
                Combatant::new(CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 20,
                    melee_range: 2.,
                    attack_duration: 0.5,
                    strike_offsets: vec![0.05],
                    player,
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
                .unwrap(),
            )
            .unwrap();
    }
    let mut kernel = bace_simulation::Kernel::with_gameplay_limits(world, 32, 4, 32).unwrap();
    let rank = bace_character::RankTable::new(&[0, 1, 10, 100]).unwrap();
    let mut traits = vec![bace_character::TraitState {
        progress: bace_character::TraitProgress {
            target: bace_gameplay_api::ProgressionTarget::Attribute(
                bace_gameplay_api::AttributeId::Strength,
            ),
            experience_spent: 0,
            advancement: bace_gameplay_api::SkillAdvancement::Inactive,
        },
        details: bace_gameplay_api::TraitDetails::Attribute {
            starting_value: 100,
        },
    }];
    traits.extend([1, 3, 5].map(|id| bace_character::TraitState {
        progress: bace_character::TraitProgress {
            target: bace_gameplay_api::ProgressionTarget::Vital(
                bace_gameplay_api::VitalId::try_from(id).unwrap(),
            ),
            experience_spent: 0,
            advancement: bace_gameplay_api::SkillAdvancement::Inactive,
        },
        details: bace_gameplay_api::TraitDetails::Vital {
            starting_value: 100,
            current: 100,
        },
    }));
    let character = bace_character::CharacterProgression::with_state(
        &traits,
        Arc::new(bace_character::ProgressionTables {
            attributes: rank.clone(),
            vitals: rank.clone(),
            trained_skills: rank.clone(),
            specialized_skills: rank,
        }),
        100,
        1,
    )
    .unwrap()
    .with_training(
        Arc::new(bace_character::SkillTrainingRules::new(&[]).unwrap()),
        0,
        &[],
    )
    .unwrap();
    kernel.register_character(binding, character).unwrap();
    kernel.restore_physical_recovery(binding.actor, 0.).unwrap();
    kernel
        .register_character_ui(
            binding,
            bace_simulation::OwnedUiState {
                state: bace_gameplay_api::CharacterUi::default(),
                known_spells: vec![],
                component_templates: vec![],
                entered: true,
            },
        )
        .unwrap();
    kernel
        .configure_combat_random_shared(
            Arc::new(bace_random::RandomRoot::new([9; 32], 1).unwrap()),
            1,
        )
        .unwrap();
    kernel
        .register_inventory_container(bace_inventory::InventoryContainer {
            id: binding.actor,
            revision: 1,
            root_owner: Some(binding.actor),
            slots: 16,
            pack_slots: 4,
            burden_limit: 10000,
            accessible: true,
            open: false,
            generation: 1,
        })
        .unwrap();
    for (slot, id, location, stack) in [(0, LAUNCHER, 0x400000, 1), (1, AMMO, 0x800000, 2)] {
        kernel
            .register_inventory_item(bace_inventory::InventoryItem {
                structure: None,
                id: EntityId(id),
                revision: 2,
                template: 100,
                stack_key: 1,
                place: bace_inventory::ItemPlace::Contained {
                    container: binding.actor,
                    slot,
                    equipped: location,
                },
                stack,
                maximum_stack: 100,
                unit_burden: 1,
                unit_value: 1,
                pack_slot: false,
                is_container: false,
                attuned: false,
                trade_reserved: false,
                active_pet: false,
                unique: false,
                quest_allowed: true,
                valid_wield: location,
                incompatible_wield: 0,
                wield_requirements_met: true,
            })
            .unwrap();
    }
    let mut profile = prepared(true);
    let mut launcher = profile.main.take().unwrap();
    launcher.entity = LAUNCHER;
    launcher.revision = 2;
    let mut ammo = launcher.clone();
    ammo.entity = AMMO;
    ammo.damage = 5.;
    profile.equipment = vec![
        PhysicalEquipmentStamp {
            entity: LAUNCHER,
            revision: 2,
            location: 0x400000,
        },
        PhysicalEquipmentStamp {
            entity: AMMO,
            revision: 2,
            location: 0x800000,
        },
    ];
    profile.launcher = Some(launcher);
    profile.ammunition = Some(ammo);
    profile.missile = Some(PhysicalMissileSpec {
        ammunition_count: 2,
        speed: 20.,
        radius: 0.05,
        gravity: false,
        tracking: false,
        attack_motion: 0x10000062,
        launch_seconds: 0.01,
        duration_seconds: 2.,
        damage_modifier: 1.,
    });
    kernel
        .register_physical_combat(binding.actor, Arc::new(profile))
        .unwrap();
    let mut target = prepared(false);
    target.equipment.clear();
    target.main = None;
    target.body_attacks = vec![PhysicalBodyAttack {
        part: 0,
        damage: 1.,
        variance: 0.,
        damage_type: 4,
    }];
    kernel
        .register_physical_combat(EntityId(TARGET), Arc::new(target))
        .unwrap();
    kernel
        .supply_physical_projectile_id(EntityId(PROJECTILE))
        .unwrap();
    for (sequence, request) in [
        (1, bace_gameplay_api::CombatRequest::ChangeMode(4)),
        (
            2,
            bace_gameplay_api::CombatRequest::TargetedMissile {
                target: EntityId(TARGET),
                height: 3,
                accuracy: 0.5,
            },
        ),
    ] {
        kernel
            .enqueue(Command::Combat {
                context: bace_gameplay_api::ActionContext {
                    actor: binding.actor,
                    account: binding.account,
                    session: binding.session,
                    sequence,
                },
                request,
            })
            .unwrap();
    }
    kernel.step().unwrap();
    let launch = kernel.take_physical_launch().unwrap();
    (kernel, launch)
}
fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.0,
        decreasing: 1.0,
        additive_increasing: 0.0,
        additive_decreasing: 0.0,
    }
}
fn prepared(player: bool) -> PhysicalCombatProfile {
    let weapon = PhysicalWeapon {
        entity: if player { 100 } else { 200 },
        revision: 1,
        style: 2,
        attack_type: 6,
        damage_type: 3,
        damage: 20.0,
        variance: 0.0,
        skill: 44,
        attack_time: 0,
        encumbrance: 0,
        offense: 1.0,
        imbues: 0,
        biting: 0.0,
        crushing: 0.0,
        slayer_type: 0,
        slayer_bonus: 0.0,
        cleave_targets: 1,
        ignore_magic_armor: false,
        ignore_magic_resistance: false,
        armor_cleaving: false,
        resistance_cleaving: None,
        proc_spell: None,
        proc_chance: 0.0,
    };
    let armor = PhysicalBodyDefense {
        part: 0,
        hit_weights: [1.0; 12],
        armor: [quality(0.0); 8],
    };
    PhysicalCombatProfile {
        equipment: vec![PhysicalEquipmentStamp {
            entity: if player { 100 } else { 200 },
            revision: 1,
            location: 0x100000,
        }],
        revision: 1,
        content_hash: [7; 32],
        player,
        creature_type: 1,
        pk: PkStatus::Npk,
        attackable: true,
        immune: false,
        lifestone_protected: false,
        style: 0x80000040,
        main: Some(weapon),
        offhand: None,
        launcher: None,
        ammunition: None,
        gloves: None,
        boots: None,
        body_attacks: vec![],
        maneuvers: vec![PhysicalManeuver {
            style: 0x80000040,
            attack_type: 4,
            height: 2,
            minimum_skill: 0,
            motion: 0x10000063,
            duration: 0.2,
            hooks: vec![
                PhysicalAttackHook {
                    seconds: 0.05,
                    part: 0,
                },
                PhysicalAttackHook {
                    seconds: 0.1,
                    part: 0,
                },
            ],
        }],
        skills: vec![
            (
                6,
                PhysicalSkill {
                    advancement: 2,
                    current: 100,
                },
            ),
            (
                44,
                PhysicalSkill {
                    advancement: 3,
                    current: 300,
                },
            ),
        ],
        strength: 100,
        coordination: 100,
        quickness: 100,
        base_strength: 0,
        base_endurance: 0,
        melee_defense_modifier: 1.0,
        missile_defense_modifier: 1.0,
        armor: vec![armor],
        resistances: [PhysicalResistance {
            quality: quality(1.0),
            augmentation: 0,
        }; 8],
        shield_encumbrance: 0,
        shield_placement: true,
        armor_layers: vec![],
        ignore_shield: 0.0,
        shield: None,
        shield_skill: PhysicalSkill {
            advancement: 0,
            current: 0,
        },
        ratings: PhysicalRatings::default(),
        critical_defense: false,
        range: 3.0,
        height: 1.0,
        missile: None,
    }
}
