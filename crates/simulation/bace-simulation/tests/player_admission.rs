//! Authored primitive geometry fixture tests atomic ownership, not stock-client qualification.
#[path = "magic_dirty_fixture/mod.rs"]
mod dirty_fixture;
use bace_character::*;
use bace_entity::*;
use bace_gameplay_api::{weapon_combat::*, *};
use bace_geometry::Vec3;
use bace_physics::*;
use bace_simulation::*;
use bace_types::*;
use std::sync::Arc;
#[path = "player_admission/locomotion.rs"]
mod locomotion;
#[path = "player_admission/magic_locomotion.rs"]
mod magic_locomotion;
fn fixture() -> (Kernel, PreparedPlayerAdmission) {
    let cell = CellId(0x100);
    let geometry = Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: cell.0,
            restriction: None,
            terrain: false,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![
                CollisionPlane {
                    normal: Vec3::new(1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(-1., 0., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., 1., 0.),
                    distance: 20.,
                },
                CollisionPlane {
                    normal: Vec3::new(0., -1., 0.),
                    distance: 20.,
                },
            ],
            faces: vec![CollisionFace {
                polygon: GdlePolygon::prepare(vec![
                    Vec3::new(-20., -20., 0.),
                    Vec3::new(20., -20., 0.),
                    Vec3::new(20., 20., 0.),
                    Vec3::new(-20., 20., 0.),
                ])
                .unwrap(),
                two_sided: true,
                object: None,
            }],
            portals: vec![],
        }])
        .unwrap(),
    );
    let shape = Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., 0.5),
                radius: 0.5,
            }],
            0.,
            0.2,
        )
        .unwrap(),
    );
    let body = Body::spawn_geometry(
        &geometry,
        GeometrySpawn {
            cell: cell.0,
            position: Vec3::new(0., 0., 0.),
            shape,
            capabilities: bace_motion::Capabilities {
                speed: 5.,
                jump_impulse: 5.,
            },
            heading: 0.,
            maximum_turn_rate: 3.,
        },
    )
    .unwrap();
    let mut world = bace_world::World::default();
    world.install_geometry(geometry).unwrap();
    let kernel = Kernel::with_gameplay_limits(world, 32, 8, 32).unwrap();
    let binding = CharacterBinding {
        session: SessionId(7),
        account: AccountId(2),
        actor: EntityId(1),
    };
    let skills = [6, 15, 16, 31, 32, 33, 34, 43, 44];
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    let mut traits: Vec<_> = skills
        .into_iter()
        .map(|id| TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Skill(id),
                experience_spent: 0,
                advancement: SkillAdvancement::Trained,
            },
            details: TraitDetails::Skill {
                initial_level: 0,
                resistance_at_last_check: 0,
                last_used_time: 0.0,
            },
        })
        .collect();
    for id in 1..=6 {
        traits.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Attribute(AttributeId::try_from(id).unwrap()),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Attribute {
                starting_value: 100,
            },
        });
    }
    for id in [1, 3, 5] {
        traits.push(TraitState {
            progress: TraitProgress {
                target: ProgressionTarget::Vital(VitalId::try_from(id).unwrap()),
                experience_spent: 0,
                advancement: SkillAdvancement::Inactive,
            },
            details: TraitDetails::Vital {
                starting_value: 100,
                current: 50,
            },
        });
    }
    let progression = CharacterProgression::with_state(
        &traits,
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        0,
        4,
    )
    .unwrap();
    let pool = VitalPool {
        current: 50,
        maximum: 100,
    };
    let mut combatant = Combatant::new(CombatantProfile {
        maximum_health: 100,
        melee_damage: 1,
        melee_range: 2.,
        attack_duration: 1.,
        strike_offsets: vec![0.5],
        player: true,
    })
    .unwrap()
    .with_resources(Some(pool), Some(pool))
    .unwrap();
    combatant.damage(50).unwrap();
    let snapshot = PlayerWorldSnapshot {
        cell,
        position: body.accepted().position(),
        heading: body.accepted().heading_radians(),
        vitals: [Some(pool); 3],
    };
    let presence = bace_social::SocialPresence {
        identity: social::SocialIdentity {
            character: binding.actor,
            account: binding.account,
            name: "Alice".into(),
        },
        access: 0,
        online: true,
        appear_offline: false,
        afk: false,
        gagged: false,
        olthoi: false,
        no_olthoi_talk: false,
        ignore_fellowship_requests: false,
        auto_accept_fellowship: false,
        share_fellowship_loot: false,
        society: 0,
        listen_allegiance: true,
        listen_general: true,
        listen_trade: true,
        listen_lfg: true,
        listen_roleplay: true,
        listen_society: true,
    };
    let input = SkillValueInputs {
        formula: VitalFormula {
            enabled: true,
            divisor: 1,
            attribute1: 1,
            attribute2: 0,
        },
        usable_untrained: false,
        base_attributes: [100; 6],
        current_attributes: [100; 6],
        bonuses: SkillBonuses::default(),
        multiplier: 1.,
        vitae: 1.,
        additive: 0,
    };
    let state = OwnedPlayerState {
        gag: None,
        equipment_mana: None,
        chat_age: None,
        physical_recovery: 0.0,
        death: Some(PlayerDeathState::default()),
        social: Some(bace_social::SocialPreferences::default()),
        portal_links: Some(bace_interactions::PortalLinks::new(0, &[], &[]).unwrap()),
        world: Some(snapshot),
        recovery: Some(bace_magic::CastRecovery::default()),
        character: OwnedCharacterState {
            progression,
            rares: None,
            ui: Some(OwnedUiState {
                state: CharacterUi::default(),
                known_spells: vec![],
                component_templates: vec![],
                entered: false,
            }),
            native_services: Some(CharacterServiceState {
                level: 1,
                total_experience: 0,
                total_skill_credits: None,
                titles: vec![],
                enlightenment: 0,
                sanctuary: None,
            }),
            contracts: Some(bace_quests::ContractRegistry::restore(vec![]).unwrap()),
        },
        enchantments: Some(bace_magic::EnchantmentRegistry::new(32).unwrap()),
        item_experience: vec![],
        item_enchantments: vec![],
    };
    (
        kernel,
        PreparedPlayerAdmission {
            chat_eligibility: Default::default(),
            locomotion: fixture_locomotion(),
            locomotion_styles: vec![fixture_locomotion()],
            death_motions: fixture_death_motions(),
            server_magic: dirty_fixture::batch(),
            binding,
            actor: Actor {
                id: binding.actor,
                cell,
                body,
            },
            state,
            properties: EntityProperties::new(64).unwrap(),
            combatant,
            caster: MagicCaster {
                player: true,
                known_spells: Default::default(),
                school_skills: [100; 5],
                magic_defense: 100,
                mana_conversion: 100,
                components_required: true,
                safe_components: false,
            },
            physical: Arc::new(prepared(true)),
            magic_damage: bace_magic::MagicDamageProfile::neutral(true),
            physical_motions: fixture_physical_motions(),
            physical_source: fixture_physical_source(),
            vital_inputs: PreparedVitalInputs {
                formulas: [VitalFormula {
                    enabled: false,
                    divisor: 1,
                    attribute1: 1,
                    attribute2: 0,
                }; 3],
                equipped_health: vec![],
            },
            skills: PreparedCharacterSkillInputs {
                attack_skill: 44,
                inputs: skills.map(|id| (id, input)).to_vec(),
                shield: None,
                attribute_modifiers: [PreparedAttributeModifier::default(); 6],
            },
            items: vec![],
            item_spell_targets: vec![],
            containers: vec![bace_inventory::InventoryContainer {
                id: binding.actor,
                revision: 4,
                root_owner: Some(binding.actor),
                slots: 24,
                pack_slots: 7,
                burden_limit: 10000,
                accessible: true,
                open: false,
                generation: 1,
            }],
            presence,
            staff: staff::StaffRegistration {
                binding,
                privileges: staff::StaffPrivileges::default(),
            },
            portal_access: bace_interactions::PortalAccess {
                level: 1,
                pk_status: 2,
                pk_recent: false,
                olthoi: false,
                vitae: false,
                account_15_days: true,
                entitlement: 0,
                quest_allowed: true,
                teleporting: false,
                recently_teleported: false,
                ignore_restrictions: false,
                enforce_maximum_level: true,
            },
        },
    )
}
#[test]
fn late_social_rejection_returns_every_owner_and_retry_admits_once() {
    let (mut k, mut p) = fixture();
    p.state
        .social
        .as_mut()
        .unwrap()
        .friends
        .insert(EntityId(99));
    let (error, mut retained) = k.admit_player(p).expect_err("missing cold friend identity");
    assert_eq!(error, PlayerAdmissionError::Social);
    assert!(!k.world().contains_identity(EntityId(1)));
    assert!(k.character(EntityId(1)).is_none());
    assert!(k.social_presence(EntityId(1)).is_none());
    retained.state.social.as_mut().unwrap().friends.clear();
    assert!(k.admit_player(*retained).is_ok());
    assert!(k.world().contains_identity(EntityId(1)));
    assert_eq!(k.character(EntityId(1)).unwrap().revision(), 4);
    assert!(k.social_presence(EntityId(1)).unwrap().online);
    assert!(k.player_death_state(EntityId(1)).is_some());
    assert!(k.magic_recovery(EntityId(1)).is_ok());
    assert!(k.take_social_event().is_some());
}
#[test]
fn typed_admission_retains_rejected_owner_and_backpressures_the_next_request() {
    let (mut kernel, mut prepared) = fixture();
    prepared
        .state
        .social
        .as_mut()
        .unwrap()
        .friends
        .insert(EntityId(99));
    kernel
        .enqueue(Command::AdmitPlayer(PlayerAdmissionRequest {
            correlation: 41,
            prepared: Box::new(prepared),
        }))
        .unwrap();
    kernel.step().unwrap();
    assert!(!kernel.world().contains_identity(EntityId(1)));
    let outcome = kernel.take_player_admission_outcome().unwrap();
    assert_eq!(outcome.correlation, 41);
    let (error, mut retained) = outcome.result.err().unwrap();
    assert_eq!(error, PlayerAdmissionError::Social);
    retained.state.social.as_mut().unwrap().friends.clear();
    kernel
        .enqueue(Command::AdmitPlayer(PlayerAdmissionRequest {
            correlation: 42,
            prepared: retained,
        }))
        .unwrap();
    kernel.step().unwrap();
    assert!(
        kernel
            .take_player_admission_outcome()
            .unwrap()
            .result
            .is_ok()
    );
    assert!(kernel.world().contains_identity(EntityId(1)));
}
#[test]
fn undelivered_admission_outcome_holds_the_next_preparation() {
    let (mut kernel, mut first) = fixture();
    let (_, second) = fixture();
    first
        .state
        .social
        .as_mut()
        .unwrap()
        .friends
        .insert(EntityId(99));
    kernel
        .enqueue(Command::AdmitPlayer(PlayerAdmissionRequest {
            correlation: 51,
            prepared: Box::new(first),
        }))
        .unwrap();
    kernel
        .enqueue(Command::AdmitPlayer(PlayerAdmissionRequest {
            correlation: 52,
            prepared: Box::new(second),
        }))
        .unwrap();
    kernel.step().unwrap();
    kernel.step().unwrap();
    assert!(!kernel.world().contains_identity(EntityId(1)));
    let first = kernel.take_player_admission_outcome().unwrap();
    assert_eq!(first.correlation, 51);
    assert!(first.result.is_err());
    kernel.step().unwrap();
    let second = kernel.take_player_admission_outcome().unwrap();
    assert_eq!(second.correlation, 52);
    assert!(second.result.is_ok());
}
fn quality(raw: f64) -> PhysicalQuality {
    PhysicalQuality {
        raw,
        increasing: 1.,
        decreasing: 1.,
        additive_increasing: 0.,
        additive_decreasing: 0.,
    }
}
fn prepared(player: bool) -> PhysicalCombatProfile {
    let _weapon = PhysicalWeapon {
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
        equipment: vec![],
        revision: 1,
        content_hash: [7; 32],
        player,
        creature_type: 1,
        pk: PkStatus::Npk,
        attackable: true,
        immune: false,
        lifestone_protected: false,
        style: 0x80000040,
        main: None,
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

fn fixture_locomotion() -> Arc<bace_motion::AnimatedLocomotion> {
    let physics = bace_motion::MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let cycle = || {
        bace_motion::RootCycle::prepare(vec![bace_motion::RootSegment {
            low: 0,
            high: 0,
            frame_count: 1,
            framerate: 30.,
            frames: vec![bace_motion::RootFrame {
                translation: Vec3::ZERO,
                heading: 0.,
            }],
        }])
        .unwrap()
    };
    Arc::new(bace_motion::AnimatedLocomotion {
        profile: bace_motion::LocomotionProfile {
            style: 0x8000003d,
            ready: physics,
            walk: physics,
            run: physics,
            sidestep: physics,
            turn: physics,
        },
        ready: cycle(),
        walk: cycle(),
        run: cycle(),
    })
}

fn fixture_physical_motions() -> Vec<(u32, f32, Arc<bace_motion::PreparedMotionChain>)> {
    use bace_motion::{
        ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
        SourceMotionTransition,
    };
    let motion = 0x10000063;
    let speed = bace_combat::physical::attack_speed(100, 0).unwrap();
    let ready = SourceMotionState {
        style: 0x80000040,
        substate: 0x41000003,
        speed: 1.0,
    };
    let clip = ExecutionClip {
        animation: 0x03000001,
        frame_count: 2,
        low: 0,
        high: 1,
        framerate: 30.0,
        frames: vec![RootFrame::default(); 2].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = Arc::new(
        PreparedMotionChain::prepare(vec![clip.clone()], 0, 0, physics, ready.substate, 1.0)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: true,
            })
            .unwrap(),
    );
    let action =
        PreparedMotionChain::prepare(vec![clip.clone(), clip], 1, 1, physics, motion, speed)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before: ready,
                after: ready,
                continues_cycle: false,
            })
            .unwrap()
            .with_stop_chain(stop)
            .unwrap();
    vec![(motion, speed, Arc::new(action))]
}
fn fixture_physical_source() -> Arc<bace_combat::preparation::PreparedPhysicalRefreshSource> {
    let mut weenie = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 1,
        class_name: "admission-physical".into(),
        weenie_type: 1,
        last_modified: None,
        properties: Default::default(),
    };
    weenie.properties.body_parts.push(bace_content::Property {
        id: 0,
        value: bace_content::BodyPart {
            d_type: 4,
            d_val: 3,
            d_var: 0.2,
            llf: 1.,
            llb: 1.,
            lrf: 1.,
            lrb: 1.,
            mlf: 1.,
            mlb: 1.,
            mrf: 1.,
            mrb: 1.,
            hlf: 1.,
            hlb: 1.,
            hrf: 1.,
            hrb: 1.,
            ..Default::default()
        },
    });
    Arc::new(bace_combat::preparation::PreparedPhysicalRefreshSource {
        actor: 1,
        weenie: Arc::new(weenie),
        equipment: vec![],
        qualities: vec![],
        profile: Arc::new(prepared(true)),
    })
}
#[test]
fn expired_vital_buff_refreshes_world_maximum_and_clamps_current_without_healing() {
    let (mut kernel, mut input) = fixture();
    input.state.enchantments = Some(
        bace_magic::EnchantmentRegistry::restore(
            32,
            0,
            vec![bace_magic::EnchantmentEntry {
                spell: 1,
                caster: 1,
                school: bace_magic::MagicSchool::Creature,
                spec: bace_magic::EnchantmentSpec {
                    category: 1,
                    power: 100,
                    duration: 5.0,
                    layer: 1,
                    stat_type: 0x5002,
                    stat_key: 1,
                    value: 1.5,
                    beneficial: true,
                    set_id: None,
                },
                start_time: 0.0,
                is_set_spell: false,
                is_level8_aura: false,
                metadata: Default::default(),
            }],
        )
        .unwrap(),
    );
    input
        .combatant
        .replace_vital_maxima([150, 100, 100])
        .unwrap();
    input
        .combatant
        .apply_vital(bace_entity::EntityVital::Health, 50, 140, None)
        .unwrap();
    input.state.world.as_mut().unwrap().vitals[0] = Some(VitalPool {
        current: 140,
        maximum: 150,
    });
    let actor = input.binding.actor;
    assert!(kernel.admit_player(input).is_ok());
    assert_eq!(
        kernel
            .world()
            .vital(actor, bace_entity::EntityVital::Health)
            .unwrap(),
        VitalPool {
            current: 140,
            maximum: 150
        }
    );
    for _ in 0..150 {
        kernel.step().unwrap();
    }
    assert_eq!(
        kernel
            .world()
            .vital(actor, bace_entity::EntityVital::Health)
            .unwrap(),
        VitalPool {
            current: 100,
            maximum: 100
        }
    );
    assert!(kernel.player_world_dirty_since(actor).is_some());
}

#[test]
fn endurance_rank_follow_up_freezes_world_current_instead_of_stale_trait_current() {
    let (mut kernel, mut input) = fixture();
    let traits: Vec<_> = input
        .state
        .character
        .progression
        .trait_states()
        .map(|(progress, details)| TraitState {
            progress,
            details: match (progress.target, details.unwrap()) {
                (
                    ProgressionTarget::Vital(VitalId::MaxHealth),
                    TraitDetails::Vital { starting_value, .. },
                ) => TraitDetails::Vital {
                    starting_value,
                    current: 90,
                },
                (_, details) => details,
            },
        })
        .collect();
    let ranks = RankTable::new(&[0, 10, 100]).unwrap();
    input.state.character.progression = CharacterProgression::with_state(
        &traits,
        Arc::new(ProgressionTables {
            attributes: ranks.clone(),
            vitals: ranks.clone(),
            trained_skills: ranks.clone(),
            specialized_skills: ranks,
        }),
        100,
        4,
    )
    .unwrap();
    let binding = input.binding;
    assert!(kernel.admit_player(input).is_ok());
    kernel
        .enqueue(Command::RaiseProgression {
            context: ActionContext {
                actor: binding.actor,
                account: binding.account,
                session: binding.session,
                sequence: 1,
            },
            request: RaiseProgression {
                target: ProgressionTarget::Attribute(AttributeId::Endurance),
                amount: 10,
            },
        })
        .unwrap();
    kernel.step().unwrap();
    let change = kernel.take_progression_outcome().unwrap().result.unwrap();
    assert_eq!(change.revision, 5);
    assert_eq!(change.after.ranks, 1);
    let health = change.follow_up_vital.unwrap();
    assert_eq!(health.target, ProgressionTarget::Vital(VitalId::MaxHealth));
    assert_eq!(
        health.details,
        Some(TraitDetails::Vital {
            starting_value: 100,
            current: kernel
                .world()
                .vital(binding.actor, EntityVital::Health)
                .unwrap()
                .current,
        })
    );
    assert_ne!(
        health.details,
        Some(TraitDetails::Vital {
            starting_value: 100,
            current: 90,
        })
    );
}

/// Explicit trusted synthetic death cursor; real admission prepares DAT links.
fn fixture_death_motions() -> Vec<bace_motion::PreparedDeathMotion> {
    use bace_motion::{
        ExecutionClip, MotionPhysics, PreparedMotionChain, RootFrame, SourceMotionState,
        SourceMotionTransition,
    };
    let before = SourceMotionState {
        style: fixture_locomotion().profile.style,
        substate: 0x41000003,
        speed: 1.0,
    };
    let after = SourceMotionState {
        substate: 0x40000011,
        ..before
    };
    let clip = |id, frames| ExecutionClip {
        animation: id,
        frame_count: frames,
        low: 0,
        high: -1,
        framerate: 30.,
        frames: vec![RootFrame::default(); frames as usize].into(),
        hooks: vec![].into(),
    };
    let physics = MotionPhysics {
        velocity: Vec3::ZERO,
        omega: Vec3::ZERO,
    };
    let stop = PreparedMotionChain::prepare(vec![clip(3, 1)], 0, 0, physics, 0x41000003, 1.)
        .unwrap()
        .with_source_transition(SourceMotionTransition {
            before,
            after: before,
            continues_cycle: false,
        })
        .unwrap();
    let dead =
        PreparedMotionChain::prepare(vec![clip(4, 4), clip(5, 1)], 1, 1, physics, 0x40000011, 1.)
            .unwrap()
            .with_source_transition(SourceMotionTransition {
                before,
                after,
                continues_cycle: false,
            })
            .unwrap();
    vec![bace_motion::PreparedDeathMotion {
        stop: Arc::new(stop),
        dead: Arc::new(dead),
    }]
}
