use super::*;
#[path = "definition.rs"]
mod definition;
#[path = "fixture.rs"]
mod fixture;
#[path = "profile.rs"]
mod profile;
use crate::{GeneratedNpcTemplate, PreparedNpcLoadout};
use bace_types::EntityId;
use std::sync::Arc;
fn setup() -> (Kernel, GeneratorSpawnKey, Arc<bace_physics::CollisionShape>) {
    setup_with_births(false)
}
fn setup_with_births(
    production: bool,
) -> (Kernel, GeneratorSpawnKey, Arc<bace_physics::CollisionShape>) {
    let (_, world, blueprint, geometry, _) = fixture::physical_fixture();
    let shape = geometry.shape.clone();
    let mut k = Kernel::new(world, 16).unwrap();
    if production {
        k.require_prepared_generator_births().unwrap();
    }
    k.configure_generators(
        Arc::new(bace_random::RandomRoot::new([8; 32], 1).unwrap()),
        1000,
        true,
    )
    .unwrap();
    let mut def = definition::definition();
    def.location.cell = 0x12340100;
    def.location.origin = [10., 10., 0.];
    def.profiles[0].where_create = 66;
    def.radius = 5.;
    def.profiles[0].weenie_class_id = 999;
    k.register_generator(Arc::new(def)).unwrap();
    let source = bace_content::WeenieV1 {
        schema_version: 1,
        weenie_id: 200,
        class_name: "mixed_creature".into(),
        weenie_type: 10,
        last_modified: None,
        properties: Default::default(),
    };
    let template = GeneratedNpcTemplate {
        script: None,
        combat_assets: None,
        death_motions: vec![],
        physical_motions: profile::motion_assets().0,
        locomotion_styles: profile::motion_assets().1,
        requires_equipment: false,
        resources: [bace_entity::VitalPool {
            current: 10,
            maximum: 10,
        }; 2],
        missile_timings: vec![],
        missile_timing_errors: vec![],
        blueprint,
        geometry,
        physical: Some(profile::profile()),
        loot: None,
        ace_loot: Some(crate::pve::AceCreatureLootPolicy {
            source: Arc::new(source),
            initial_items: vec![],
            initial_parents: vec![],
            initial_ids: vec![],
            treasure: None,
            templates: Arc::new(BTreeMap::new()),
            profile_id: 1,
            profile_revision: [1; 32],
            content_generation: [1; 32],
            rare: None,
            rare_profile_revision: None,
            creature_level: 1,
        }),
    };
    k.register_generated_npc_template(1, 200, template).unwrap();
    k.supply_generator_id(EntityId(0x8000_0000 | 100)).unwrap();
    k.step().unwrap();
    let request = k.take_generator_request().unwrap();
    k.supply_generator_request_ids(
        request.intent.key,
        1,
        &[EntityId(0x8000_0000 | 101), EntityId(0x8000_0000 | 102)],
    )
    .unwrap();
    (k, request.intent.key, shape)
}
fn container(actor: EntityId) -> InventoryContainer {
    InventoryContainer {
        id: actor,
        revision: 1,
        root_owner: None,
        slots: 10,
        pack_slots: 2,
        burden_limit: 10000,
        accessible: true,
        open: true,
        generation: 1,
    }
}
fn npc(actor: u32) -> Root {
    Root::Creature {
        root: EntityId(0x8000_0000 | actor),
        template: 200,
        loadout: Box::new(PreparedNpcLoadout {
            combat_assets: None,
            death_motions: vec![],
            physical_motions: profile::motion_assets().0,
            locomotion_styles: profile::motion_assets().1,
            items: vec![],
            containers: vec![container(EntityId(0x8000_0000 | actor))],
            profile: profile::profile(),
            death_items: vec![],
            death_parents: vec![],
            death_ids: vec![],
            enchantments: vec![],
        }),
    }
}
fn item(id: u32, shape: Arc<bace_physics::CollisionShape>) -> Root {
    Root::Item {
        root: EntityId(0x8000_0000 | id),
        items: vec![InventoryItem {
            id: EntityId(0x8000_0000 | id),
            structure: None,
            revision: 1,
            template: 300,
            stack_key: 1,
            place: ItemPlace::World,
            stack: 1,
            maximum_stack: 1,
            unit_burden: 0,
            unit_value: 0,
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
        }],
        containers: vec![],
        shape,
    }
}
#[test]
fn mixed_two_creatures_and_item_share_receipt_but_have_distinct_death_contexts() {
    let (mut k, key, shape) = setup();
    let receipt = k
        .admit_generated_mixed_trees(key, vec![npc(100), item(101, shape), npc(102)])
        .unwrap();
    assert_eq!(
        receipt.roots,
        vec![
            EntityId(0x8000_0000 | 100),
            EntityId(0x8000_0000 | 101),
            EntityId(0x8000_0000 | 102)
        ]
    );
    assert_eq!(receipt.entities.len(), 3);
    assert!(receipt.failed_roots.is_empty());
    assert!(k.world.combatant(EntityId(0x8000_0000 | 100)).is_some());
    assert!(k.world.combatant(EntityId(0x8000_0000 | 102)).is_some());
    assert!(k.inventory.item(EntityId(0x8000_0000 | 101)).is_some());
    assert!(!k.generators.requests.contains_key(&key));
    assert!(k.generators.machines[&key.generator.entity].owns_member(EntityId(0x8000_0000 | 100)));
}
#[test]
fn invalid_later_creature_rolls_back_bodies_combat_and_loot_without_acknowledgment() {
    let (mut k, key, shape) = setup();
    let mut invalid = k.generators.templates[&(1, 200)].clone();
    invalid.blueprint.think_interval = 0;
    k.register_generated_npc_template(1, 201, invalid).unwrap();
    let mut second = npc(101);
    let Root::Creature { template, .. } = &mut second else {
        panic!("fixture creature");
    };
    *template = 201;
    let roots = vec![npc(100), second, item(102, shape)];
    assert_eq!(
        k.admit_generated_mixed_trees(key, roots.clone()),
        Err(G::Invalid)
    );
    for id in [100, 101, 102] {
        assert!(!k.world.contains_identity(EntityId(0x8000_0000 | id)));
        assert!(k.inventory.item(EntityId(0x8000_0000 | id)).is_none());
        assert!(
            k.population
                .generated_origin(EntityId(0x8000_0000 | id))
                .is_none()
        );
    }
    assert!(k.generators.requests.contains_key(&key));
    assert!(!k.generators.machines[&key.generator.entity].owns_member(EntityId(0x8000_0000 | 100)));
    // Reusing the same exact event succeeds: no failed-attempt loot context,
    // physical actor or inventory graph was left behind.
    k.generators
        .templates
        .get_mut(&(1, 201))
        .unwrap()
        .blueprint
        .think_interval = 1;
    assert_eq!(
        k.admit_generated_mixed_trees(key, roots)
            .unwrap()
            .roots
            .len(),
        3
    );
}
#[test]
fn permanent_item_placement_failure_keeps_other_source_ordered_roots() {
    let (mut k, key, shape) = setup();
    let large = Arc::new(
        bace_physics::CollisionShape::prepare(
            vec![bace_physics::CollisionSphere {
                center: bace_geometry::Vec3::new(0., 0., 100.),
                radius: 100.,
            }],
            0.1,
            0.1,
        )
        .unwrap(),
    );
    let result = k
        .admit_generated_mixed_trees(key, vec![npc(100), item(101, large), item(102, shape)])
        .unwrap();
    assert_eq!(result.failed_roots, vec![EntityId(0x80000065)]);
    assert_eq!(
        result.roots,
        vec![EntityId(0x80000064), EntityId(0x80000066)]
    );
    assert!(!k.world.contains_identity(EntityId(0x80000065)));
    assert!(k.world.combatant(EntityId(0x80000064)).is_some());
    assert!(k.inventory.item(EntityId(0x80000066)).is_some());
}
#[test]
fn source_vendor_passivity_prevents_target_acquisition_without_changing_other_owners() {
    for line in include_str!("../../../tests/fixtures/creature_combat_ai.csv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let values: Vec<u32> = line.split('|').map(|v| v.parse().unwrap()).collect();
        let (mut k, key, shape) = setup();
        let source = Arc::make_mut(
            &mut k
                .generators
                .templates
                .get_mut(&(1, 200))
                .unwrap()
                .ace_loot
                .as_mut()
                .unwrap()
                .source,
        );
        source.weenie_type = 12;
        source.properties.bools.push(bace_content::Property {
            id: 19,
            value: values[0] != 0,
        });
        source.properties.ints.push(bace_content::Property {
            id: 68,
            value: values[1] as i32,
        });
        k.admit_generated_mixed_trees(
            key,
            vec![npc(100), item(101, shape.clone()), item(102, shape.clone())],
        )
        .unwrap();
        let cell = bace_types::CellId(0x12340100);
        let body = k
            .world
            .prepare_geometry_body(bace_physics::GeometrySpawn {
                cell: cell.0,
                position: bace_geometry::Vec3::new(2., 2., 0.),
                shape,
                capabilities: bace_motion::Capabilities {
                    speed: 0.,
                    jump_impulse: 0.,
                },
                heading: 0.,
                maximum_turn_rate: 0.,
            })
            .unwrap();
        k.world
            .insert(bace_entity::Actor {
                id: EntityId(50),
                cell,
                body,
            })
            .unwrap();
        k.world
            .register_combatant(
                EntityId(50),
                bace_entity::Combatant::new(bace_entity::CombatantProfile {
                    maximum_health: 100,
                    melee_damage: 1,
                    melee_range: 1.,
                    attack_duration: 1.,
                    strike_offsets: vec![0.5],
                    player: true,
                })
                .unwrap(),
            )
            .unwrap();
        for _ in 0..5 {
            k.world.tick().unwrap();
        }
        k.population
            .think(&mut k.world, &mut k.combat, 100, |_| false);
        let target = k
            .population
            .magic_targets()
            .find(|(actor, _)| *actor == EntityId(0x80000064))
            .and_then(|(_, target)| target);
        assert_eq!(
            target.is_some(),
            values[2] != 0,
            "ACE SetMonsterState {line}"
        );
        assert!(k.world.contains_identity(EntityId(0x80000064)));
        assert!(k.inventory.container(EntityId(0x80000064)).is_some());
    }
}

#[path = "constructed_tests.rs"]
mod constructed_tests;

#[test]
fn production_birth_pvs_failure_rolls_back_then_exact_retry_retains_evidence() {
    let (mut kernel, key, shape) = setup_with_births(true);
    let roots = vec![npc(100), item(101, shape), npc(102)];
    assert_eq!(
        kernel.admit_generated_mixed_trees(key, roots.clone()),
        Err(G::Geometry)
    );
    for id in [100, 101, 102] {
        let entity = EntityId(0x8000_0000 | id);
        assert!(!kernel.world.contains_identity(entity));
        assert!(kernel.inventory.item(entity).is_none());
        assert!(kernel.population.generated_origin(entity).is_none());
    }
    assert!(kernel.generators.requests.contains_key(&key));
    assert!(kernel.take_generator_event().is_none());
    kernel
        .world
        .install_cell_visibility(vec![bace_world::PreparedCellVisibility {
            cell: bace_types::CellId(0x12340100),
            seen_outside: false,
            visible_cells: Arc::from([]),
        }])
        .unwrap();
    let receipt = kernel.admit_generated_mixed_trees(key, roots).unwrap();
    let mut births = Vec::new();
    while let Some(event) = kernel.take_generator_event() {
        if let crate::GeneratorWorldEvent::Spawned { entity, birth, .. } = event {
            let birth = birth.expect("production birth evidence");
            assert_eq!(birth.view.entity, entity);
            assert_eq!(birth.tick, kernel.tick);
            assert_eq!(birth.view.cell, 0x12340100);
            assert!(birth.observers.is_empty());
            births.push(entity);
        }
    }
    assert_eq!(births, receipt.roots);
}
