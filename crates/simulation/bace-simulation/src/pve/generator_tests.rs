use super::*;
use bace_physics::{
    CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon, GeometryCell,
    GeometryRegion,
};
use std::sync::Arc;
fn fixture() -> (
    Population,
    World,
    NpcBlueprint,
    PreparedNpcGeometry,
    GeneratedNpcOrigin,
) {
    let mut world = World::default();
    let floor = CollisionFace {
        polygon: GdlePolygon::prepare(vec![
            Vec3::new(0., 0., 0.),
            Vec3::new(24., 0., 0.),
            Vec3::new(24., 24., 0.),
            Vec3::new(0., 24., 0.),
        ])
        .unwrap(),
        two_sided: false,
        object: None,
    };
    world
        .install_geometry(Arc::new(
            GeometryRegion::prepare(vec![GeometryCell {
                id: 0x12340001,
                terrain: false,
                restriction: None,
                solids: vec![],
                static_primitives: vec![],
                boundary: vec![
                    CollisionPlane {
                        normal: Vec3::new(1., 0., 0.),
                        distance: 0.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(-1., 0., 0.),
                        distance: 24.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(0., 1., 0.),
                        distance: 0.,
                    },
                    CollisionPlane {
                        normal: Vec3::new(0., -1., 0.),
                        distance: 24.,
                    },
                ],
                faces: vec![floor],
                portals: vec![],
            }])
            .unwrap(),
        ))
        .unwrap();
    let blueprint = NpcBlueprint {
        cell: CellId(0x12340001),
        position: Vec3::new(10., 10., 0.),
        radius: 0.5,
        capabilities: Capabilities {
            speed: 1.,
            jump_impulse: 0.,
        },
        combat: CombatantProfile {
            maximum_health: 20,
            melee_damage: 2,
            melee_range: 1.,
            attack_duration: 0.5,
            strike_offsets: vec![0.25],
            player: false,
        },
        visual_range: 18.,
        think_interval: 1,
        corpse_template: 10,
        xp_override: None,
        loot: vec![],
        death_animation_ticks: 1,
        respawn_ticks: 1,
        corpse_decay_ticks: 10,
    };
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
    let geometry = PreparedNpcGeometry {
        shape: Arc::new(
            CollisionShape::prepare(
                vec![CollisionSphere {
                    center: Vec3::new(0., 0., 0.5),
                    radius: 0.5,
                }],
                0.1,
                0.1,
            )
            .unwrap(),
        ),
        locomotion: Arc::new(bace_motion::AnimatedLocomotion {
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
        }),
        heading: 0.,
        maximum_turn_rate: 0.,
        run_rate: 1.,
    };
    let origin = GeneratedNpcOrigin {
        generator: EntityId(5),
        incarnation: 1,
        content_revision: 1,
        profile: 0,
        child_incarnation: 1,
    };
    (Population::new(8), world, blueprint, geometry, origin)
}
#[test]
fn generated_creature_uses_authentic_geometry_and_death_has_one_respawn_owner() {
    let (mut population, mut world, blueprint, geometry, origin) = fixture();
    let actor = EntityId(0x80000001);
    assert!(world.scene(blueprint.cell).is_err());
    population
        .spawn_generated(actor, blueprint, physical(geometry), origin, &mut world, 0)
        .unwrap();
    assert!(world.body(actor).unwrap().collision_shape().is_some());
    world.tick().unwrap();
    world
        .apply_vital_batch(
            &[bace_entity::VitalMutation {
                actor,
                vital: bace_entity::EntityVital::Health,
                before: 20,
                after: 0,
            }],
            None,
        )
        .unwrap();
    let mut characters = Characters::new(8);
    population.feed_random(&[0.25]).unwrap();
    assert!(population.ingest(
        CombatEvent::Damage {
            death_blow: None,
            attacker: Some(EntityId(9)),
            target: actor,
            target_incarnation: world.combatant(actor).unwrap().incarnation(),
            amount: 20,
            current: 0,
            maximum: 20,
            killed: true,
            revision: 1
        },
        &world,
        &mut characters,
        1,
        (false, false),
        |_| false
    ));
    population.maintain(&mut world, 2);
    let proposal = population.take_proposal().unwrap();
    let accepted = proposal.position.as_ref().unwrap();
    assert_eq!(accepted.obj_cell_id, 0x12340001);
    assert_eq!(
        (
            accepted.position_x,
            accepted.position_y,
            accepted.position_z
        ),
        (10., 10., 0.)
    );
    assert!(!proposal.no_corpse);
    assert!(!proposal.olthoi_killer);
    population
        .committed(
            proposal.operation,
            EntityId(0x80000002),
            &[],
            &[],
            &mut world,
            2,
        )
        .unwrap();
    assert_eq!(population.respawns.pending(), 0);
    assert!(population.respawn_blueprints.is_empty());
    let notice = population.generated_death().unwrap();
    assert_eq!(notice, GeneratedNpcDeath { actor, origin });
    population.feed_id(EntityId(0x80000003), &world).unwrap();
    for tick in 3..20 {
        population.maintain(&mut world, tick);
        world.tick().unwrap();
    }
    assert!(world.combatant(EntityId(0x80000003)).is_none());
    assert_eq!(population.generated_death(), Some(notice));
    assert!(population.acknowledge_generated_death(notice));
    assert!(!population.acknowledge_generated_death(notice));
}
#[test]
fn missing_authentic_cell_never_falls_back_to_a_synthetic_scene() {
    let (mut population, mut world, mut blueprint, geometry, origin) = fixture();
    blueprint.cell = CellId(2);
    world
        .register_scene(
            CellId(2),
            bace_physics::SyntheticScene::new(
                0.,
                bace_geometry::Aabb::new(Vec3::new(0., 0., -1.), Vec3::new(24., 24., 24.)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        population.spawn_generated(
            EntityId(0x80000001),
            blueprint,
            physical(geometry),
            origin,
            &mut world,
            0
        ),
        Err(PveError::MissingGeometry)
    );
    assert!(!world.contains_identity(EntityId(0x80000001)));
    assert!(population.npcs.is_empty());
}

fn ace_policy() -> AceCreatureLootPolicy {
    use bace_content::{CreateListEntry, Property, SparseProperties, WeenieV1};
    let item = Arc::new(WeenieV1 {
        schema_version: 1,
        weenie_id: 20,
        class_name: "trophy".into(),
        weenie_type: 51,
        last_modified: None,
        properties: SparseProperties {
            ints: vec![
                Property { id: 11, value: 100 },
                Property { id: 12, value: 1 },
                Property { id: 13, value: 2 },
                Property { id: 15, value: 3 },
            ],
            strings: vec![Property {
                id: 1,
                value: "Source Trophy".into(),
            }],
            floats: vec![Property {
                id: 12,
                value: 0.75,
            }],
            ..Default::default()
        },
    });
    let source = Arc::new(WeenieV1 {
        schema_version: 1,
        weenie_id: 10,
        class_name: "monster".into(),
        weenie_type: 10,
        last_modified: None,
        properties: SparseProperties {
            create_list: vec![CreateListEntry {
                destination_type: 8,
                weenie_class_id: 20,
                stack_size: 4,
                palette: 7,
                shade: 1.0,
                ..Default::default()
            }],
            ..Default::default()
        },
    });
    AceCreatureLootPolicy {
        source,
        treasure: None,
        templates: Arc::new(BTreeMap::from([(20, item)])),
        profile_id: 10,
        profile_revision: [3; 32],
        content_generation: [4; 32],
        rare: None,
        rare_profile_revision: None,
        creature_level: 1,
        initial_items: vec![],
        initial_parents: vec![],
        initial_ids: vec![],
    }
}
#[test]
fn ace_generated_death_freezes_full_trophy_snapshot_across_retry() {
    let (mut population, mut world, blueprint, geometry, origin) = fixture();
    population
        .configure_generator_loot(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
        )
        .unwrap();
    let actor = EntityId(0x80000001);
    population
        .spawn_generated(actor, blueprint, physical(geometry), origin, &mut world, 0)
        .unwrap();
    population.ace_loot(actor, ace_policy(), [8; 16]).unwrap();
    world
        .apply_vital_batch(
            &[bace_entity::VitalMutation {
                actor,
                vital: bace_entity::EntityVital::Health,
                before: 20,
                after: 0,
            }],
            None,
        )
        .unwrap();
    let mut characters = Characters::new(8);
    assert!(population.ingest(
        CombatEvent::Damage {
            death_blow: None,
            attacker: Some(EntityId(9)),
            target: actor,
            target_incarnation: world.combatant(actor).unwrap().incarnation(),
            amount: 20,
            current: 0,
            maximum: 20,
            killed: true,
            revision: 1
        },
        &world,
        &mut characters,
        1,
        (false, false),
        |_| false
    ));
    population.maintain(&mut world, 2);
    let proposal = population.take_proposal().unwrap();
    let loot = proposal.native.as_ref().unwrap();
    let items = loot.source_items.as_ref().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].properties.strings[0].value, "Source Trophy");
    assert_eq!(items[0].properties.floats[0].value, 0.75);
    assert!(
        items[0]
            .properties
            .ints
            .iter()
            .any(|p| p.id == 12 && p.value == 4)
    );
    assert!(
        items[0]
            .properties
            .ints
            .iter()
            .any(|p| p.id == 3 && p.value == 7)
    );
    population.retry(proposal.operation).unwrap();
    population.maintain(&mut world, 3);
    assert_eq!(population.take_proposal().unwrap(), proposal);
}
#[test]
fn static_creature_region_admission_is_atomic_and_uses_population() {
    let (_, world, blueprint, geometry, _) = fixture();
    let region = world.geometry().unwrap().clone();
    let mut kernel = crate::Kernel::new(world, 8).unwrap();
    kernel
        .configure_generators(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
            true,
        )
        .unwrap();
    let creature = crate::GeneratedNpcTemplate {
        script: None,
        combat_assets: None,
        death_motions: vec![],
        physical_motions: vec![],
        locomotion_styles: vec![],
        requires_equipment: false,
        resources: [bace_entity::VitalPool {
            current: 50,
            maximum: 50,
        }; 2],
        missile_timings: vec![],
        missile_timing_errors: vec![],
        blueprint,
        geometry: geometry.clone(),
        physical: None,
        loot: None,
        ace_loot: Some(ace_policy()),
    };
    let root = crate::PreparedGeneratorRoot {
        script: None,
        entity: EntityId(40),
        location: bace_gameplay_api::GeneratorLocation {
            cell: 0x12340001,
            origin: [10., 10., 0.],
            rotation: [0., 0., 0., 1.],
        },
        shape: geometry.shape,
        creature: Some(creature),
        loadout: None,
    };
    let mut batch = crate::PreparedGeneratorRegion {
        landblock: 0x1234,
        epoch: 1,
        revision: 1,
        geometry: region,
        roots: vec![root.clone()],
        containers: vec![],
        definitions: vec![],
        templates: vec![],
        creatures: vec![],
    };
    let mut overlapping = root;
    overlapping.entity = EntityId(41);
    batch.roots.push(overlapping);
    assert_eq!(
        kernel.admit_generator_region(batch.clone()),
        Err(crate::GeneratorServiceError::Geometry)
    );
    assert!(!kernel.world().contains_identity(EntityId(40)));
    batch.roots.pop();
    kernel.admit_generator_region(batch).unwrap();
    assert_eq!(kernel.world().combatant(EntityId(40)).unwrap().health(), 20);
    assert!(
        kernel
            .world()
            .body(EntityId(40))
            .unwrap()
            .collision_shape()
            .is_some()
    );
    assert!(kernel.has_pve_state());
}

fn physical(geometry: PreparedNpcGeometry) -> PreparedNpcPhysical {
    PreparedNpcPhysical {
        geometry,
        resources: [bace_entity::VitalPool {
            current: 50,
            maximum: 50,
        }; 2],
    }
}

#[test]
fn death_refresh_uses_current_owned_stacks_and_parent_tree() {
    use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
    let (mut population, mut world, blueprint, geometry, origin) = fixture();
    let actor = EntityId(0x80000001);
    population
        .configure_generator_loot(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
        )
        .unwrap();
    population
        .spawn_generated(actor, blueprint, physical(geometry), origin, &mut world, 0)
        .unwrap();
    let mut policy = ace_policy();
    let source = (**policy.templates.get(&20).unwrap()).clone();
    policy.initial_ids = vec![EntityId(101), EntityId(102), EntityId(103)];
    policy.initial_items = vec![source.clone(), source.clone(), source];
    policy.initial_parents = vec![None, Some(0), None];
    population.ace_loot(actor, policy, [8; 16]).unwrap();
    let mut inventory = crate::inventory::Inventory::new(8);
    for id in [actor, EntityId(101)] {
        inventory
            .register_container(InventoryContainer {
                id,
                revision: 1,
                root_owner: None,
                slots: 10,
                pack_slots: 10,
                burden_limit: 10000,
                accessible: true,
                open: true,
                generation: 1,
            })
            .unwrap();
    }
    for (id, parent, stack, bag) in [(101, actor, 1, true), (102, EntityId(101), 7, false)] {
        inventory
            .register_item(InventoryItem {
                id: EntityId(id),
                revision: 1,
                template: 20,
                stack_key: 0,
                place: ItemPlace::Contained {
                    container: parent,
                    slot: 0,
                    equipped: 0,
                },
                stack,
                maximum_stack: 100,
                structure: Some(3),
                unit_burden: 2,
                unit_value: 3,
                pack_slot: bag,
                is_container: bag,
                attuned: false,
                trade_reserved: false,
                active_pet: false,
                unique: false,
                quest_allowed: true,
                valid_wield: 0,
                incompatible_wield: 0,
                wield_requirements_met: true,
            })
            .unwrap();
    }
    population.refresh_owned_loot(actor, &inventory).unwrap();
    let accepted = &population.native.aces.get(&actor).unwrap().policy;
    assert_eq!(accepted.initial_ids, vec![EntityId(101), EntityId(102)]);
    assert_eq!(accepted.initial_parents, vec![None, Some(0)]);
    let props = &accepted.initial_items[1].properties;
    for (id, value) in [(12, 7), (92, 3), (5, 14), (19, 21)] {
        assert!(props.ints.iter().any(|p| p.id == id && p.value == value));
    }
    assert_eq!(props.strings[0].value, "Source Trophy");
    // A later refresh rebuilds from immutable source and cannot multiply totals again.
    let frozen = accepted.initial_items.clone();
    population.refresh_owned_loot(actor, &inventory).unwrap();
    assert_eq!(
        population
            .native
            .aces
            .get(&actor)
            .unwrap()
            .policy
            .initial_items,
        frozen
    );
}

#[test]
fn no_corpse_olthoi_killer_copies_pose_without_rolling_treasure() {
    let (mut population, mut world, blueprint, geometry, origin) = fixture();
    population
        .configure_generator_loot(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
        )
        .unwrap();
    let actor = EntityId(0x80000041);
    population
        .spawn_generated(actor, blueprint, physical(geometry), origin, &mut world, 0)
        .unwrap();
    population.ace_loot(actor, ace_policy(), [9; 16]).unwrap();
    let mut properties = bace_entity::EntityProperties::new(8).unwrap();
    let change = properties
        .propose(
            bace_entity::PropertyFamily::Bool,
            29,
            Some(bace_entity::PropertyValue::Bool(true)),
        )
        .unwrap();
    properties.adopt(change).unwrap();
    world.register_properties(actor, properties).unwrap();
    world
        .combatant_mut(actor)
        .unwrap()
        .damage_from(EntityId(9), 20)
        .unwrap();
    let mut characters = Characters::new(8);
    assert!(population.ingest(
        CombatEvent::Damage {
            death_blow: None,
            attacker: Some(EntityId(9)),
            target: actor,
            target_incarnation: world.combatant(actor).unwrap().incarnation(),
            amount: 20,
            current: 0,
            maximum: 20,
            killed: true,
            revision: world.combatant(actor).unwrap().revision(),
        },
        &world,
        &mut characters,
        1,
        (false, false),
        |id| id == EntityId(9),
    ));
    population.maintain(&mut world, 2);
    let proposal = population.take_proposal().unwrap();
    assert!(proposal.no_corpse);
    assert!(proposal.olthoi_killer);
    assert_eq!(proposal.owner, Some(EntityId(9)));
    let position = proposal.position.unwrap();
    assert_eq!(position.obj_cell_id, 0x12340001);
    assert_eq!(
        (
            position.position_x,
            position.position_y,
            position.position_z
        ),
        (10., 10., 0.)
    );
    let native = proposal.native.unwrap();
    assert!(native.generated.is_empty());
    assert_eq!(native.source_items, Some(vec![]));
    assert!(native.rare.is_none());
}

#[test]
fn prepared_no_corpse_drop_waits_for_exact_receipt_then_replaces_victim() {
    let (mut population, mut world, blueprint, geometry, origin) = fixture();
    population
        .configure_generator_loot(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
        )
        .unwrap();
    let victim = EntityId(0x8000_0041);
    let drop_id = EntityId(0x8000_0042);
    population
        .spawn_generated(
            victim,
            blueprint,
            physical(geometry.clone()),
            origin,
            &mut world,
            0,
        )
        .unwrap();
    population.ace_loot(victim, ace_policy(), [9; 16]).unwrap();
    let mut properties = bace_entity::EntityProperties::new(8).unwrap();
    let change = properties
        .propose(
            bace_entity::PropertyFamily::Bool,
            29,
            Some(bace_entity::PropertyValue::Bool(true)),
        )
        .unwrap();
    properties.adopt(change).unwrap();
    world.register_properties(victim, properties).unwrap();
    world
        .combatant_mut(victim)
        .unwrap()
        .damage_from(EntityId(9), 20)
        .unwrap();
    let mut characters = Characters::new(8);
    assert!(population.ingest(
        CombatEvent::Damage {
            death_blow: None,
            attacker: Some(EntityId(9)),
            target: victim,
            target_incarnation: world.combatant(victim).unwrap().incarnation(),
            amount: 20,
            current: 0,
            maximum: 20,
            killed: true,
            revision: world.combatant(victim).unwrap().revision(),
        },
        &world,
        &mut characters,
        1,
        (false, false),
        |_| false,
    ));
    population.maintain(&mut world, 2);
    let proposal = population.take_proposal().unwrap();
    assert!(proposal.no_corpse);
    assert_eq!(proposal.drops.len(), 1);
    let pose = proposal.position.as_ref().unwrap();
    let body = Body::spawn_geometry(
        world.geometry().unwrap(),
        bace_physics::GeometrySpawn {
            cell: pose.obj_cell_id,
            position: Vec3::new(pose.position_x, pose.position_y, pose.position_z),
            shape: geometry.shape.clone(),
            capabilities: Capabilities {
                speed: 0.,
                jump_impulse: 0.,
            },
            heading: 0.,
            maximum_turn_rate: 0.,
        },
    )
    .unwrap();
    let mut forest = crate::PreparedWorldRegionItems::default();
    forest.items.push(bace_inventory::InventoryItem {
        id: drop_id,
        revision: 1,
        template: 20,
        stack_key: 0,
        place: bace_inventory::ItemPlace::World,
        stack: 4,
        maximum_stack: 100,
        structure: None,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: false,
        is_container: false,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: true,
    });
    forest.roots.push(crate::PreparedWorldRegionRoot {
        entity: drop_id,
        location: bace_gameplay_api::GeneratorLocation {
            cell: pose.obj_cell_id,
            origin: [pose.position_x, pose.position_y, pose.position_z],
            rotation: [
                pose.rotation_x,
                pose.rotation_y,
                pose.rotation_z,
                pose.rotation_w,
            ],
        },
        shape: geometry.shape,
        corpse: None,
    });
    population
        .stage_death(
            proposal.operation,
            PreparedPveDeath {
                forest,
                roots: vec![(
                    Actor {
                        id: drop_id,
                        cell: CellId(pose.obj_cell_id),
                        body,
                    },
                    None,
                )],
            },
        )
        .unwrap();
    assert!(matches!(
        population.committed_prepared(
            proposal.operation,
            Some(drop_id),
            &[drop_id],
            &[],
            &mut world,
            2,
        ),
        Err(PveError::InvalidReceipt)
    ));
    assert!(world.contains_identity(victim));
    let adopted = population
        .committed_prepared(proposal.operation, None, &[drop_id], &[], &mut world, 2)
        .unwrap();
    assert_eq!(adopted.roots[0].entity, drop_id);
    assert!(!world.contains_identity(victim));
    assert!(world.contains_identity(drop_id));
    assert_eq!(
        population.events.pop_front(),
        Some(PveEvent::NoCorpseWorldDropsCreated {
            operation: proposal.operation,
            victim,
            roots: vec![drop_id],
        })
    );
}
