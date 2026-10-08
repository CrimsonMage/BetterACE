use super::*;
use crate::PreparedContainedCreature;
fn prepared() -> (Kernel, GeneratorSpawnKey, PreparedContainedCreature) {
    let (mut k, key, shape) = setup();
    let parent = key.generator.entity;
    k.register_inventory_container(container(parent)).unwrap();
    k.generators
        .requests
        .get_mut(&key)
        .unwrap()
        .intent
        .destination = GeneratorDestination::Contain { container: parent };
    let Root::Creature {
        root, mut loadout, ..
    } = npc(100)
    else {
        panic!()
    };
    let Root::Item { mut items, .. } = item(100, shape.clone()) else {
        panic!()
    };
    let mut actor = items.remove(0);
    actor.template = 200;
    actor.is_container = true;
    actor.place = ItemPlace::Contained {
        container: parent,
        slot: 0,
        equipped: 0,
    };
    for (id, equipped) in [(101, 1), (102, 0)] {
        let Root::Item { mut items, .. } = item(id, shape.clone()) else {
            panic!()
        };
        let mut child = items.remove(0);
        child.place = ItemPlace::Contained {
            container: root,
            slot: 0,
            equipped,
        };
        child.valid_wield = 1;
        loadout.items.push(child);
    }
    Arc::make_mut(&mut loadout.profile).equipment.push(
        bace_gameplay_api::weapon_combat::PhysicalEquipmentStamp {
            entity: 0x80000065,
            revision: 1,
            location: 1,
        },
    );
    loadout
        .enchantments
        .push(crate::PreparedGeneratorEnchantment {
            target: root,
            entry: bace_magic::EnchantmentEntry {
                spell: 1,
                caster: 0x80000065,
                school: bace_magic::MagicSchool::Creature,
                spec: bace_magic::EnchantmentSpec {
                    category: 1,
                    power: 100,
                    duration: 30.0,
                    layer: 0,
                    stat_type: 1,
                    stat_key: 1,
                    value: 10.0,
                    beneficial: true,
                    set_id: None,
                },
                start_time: 0.,
                is_set_spell: false,
                is_level8_aura: false,
                metadata: Default::default(),
            },
        });
    (
        k,
        key,
        PreparedContainedCreature {
            root: actor,
            loadout,
            weenie_type: 10,
        },
    )
}
#[test]
fn contained_creature_keeps_one_graph_and_delayed_registry_then_promotes_exactly() {
    let (mut k, key, prepared) = prepared();
    let actor = prepared.root.id;
    let mut incoming = vec![prepared.root.clone()];
    incoming.extend(prepared.loadout.items.iter().cloned());
    let graph = k
        .inventory
        .prepare_contained_forest(&[actor], &incoming, &prepared.loadout.containers)
        .expect("valid constructed graph");
    assert!(k.combat.proposed_equipment_current(
        &prepared.loadout.profile,
        actor,
        graph.inventory()
    ));
    bace_combat::physical::validate_physical_profile(&prepared.loadout.profile)
        .expect("valid physical profile");
    k.preflight_generated_enchantments(actor, &prepared.loadout.enchantments)
        .expect("valid enchantments");
    let receipt = k.admit_contained_creature(key, prepared).unwrap();
    let source = include_str!("../../../tests/fixtures/generator_destinations.csv");
    assert!(
        source.lines().any(|row| row == "1|1|1|1|0"),
        "compiled ACE accepts Creature into a Container that accepts its inventory insertion"
    );
    assert_eq!(receipt.roots, [actor]);
    assert_eq!(receipt.entities.len(), 3);
    assert!(!k.world.contains_identity(actor));
    assert!(k.has_constructed_creature(actor));
    assert!(k.magic.registry(actor).is_none());
    let due = k.tick + 3;
    k.step_generated_enchantments(due - 1).unwrap();
    assert!(k.magic.registry(actor).is_none());
    k.step_generated_enchantments(due).unwrap();
    let entries = k.magic.registry(actor).unwrap().entries().to_vec();
    assert_eq!(entries.len(), 1);
    let location = GeneratorLocation {
        cell: 0x12340100,
        origin: [10., 10., 0.],
        rotation: [0., 0., 0., 1.],
    };
    assert_eq!(
        k.promote_contained_creature(actor, 9, location),
        Err(G::Stale)
    );
    let mut bad = location;
    bad.cell = 0;
    assert_eq!(
        k.promote_contained_creature(actor, 1, bad),
        Err(G::Geometry)
    );
    assert!(k.has_constructed_creature(actor));
    assert!(k.inventory.item(actor).is_some());
    k.promote_contained_creature(actor, 1, location).unwrap();
    assert!(!k.has_constructed_creature(actor));
    assert!(k.world.combatant(actor).is_some());
    assert!(k.inventory.item(actor).is_none());
    assert!(k.inventory.container(actor).is_some());
    assert_eq!(k.inventory.equipped_items(actor).count(), 1);
    assert_eq!(k.magic.registry(actor).unwrap().entries(), entries);
    assert!(!k.generated_enchantments.has_state());
    assert!(k.promote_contained_creature(actor, 1, location).is_err());
}
#[test]
fn restored_constructed_creature_keeps_exact_children_and_blocks_unreceipted_promotion() {
    let (mut k, key, prepared) = prepared();
    let actor = prepared.root.id;
    let request = k.generators.requests.get(&key).unwrap().clone();
    let mut template = k
        .generators
        .templates
        .get(&(key.generator.content_revision, prepared.root.template))
        .unwrap()
        .clone();
    template.physical = Some(prepared.loadout.profile.clone());
    let entry = crate::kernel::constructed_creatures::PreparedRestoredConstructedCreature {
        actor,
        landblock: request.landblock,
        intent: request.intent,
        template,
        children: prepared.loadout.items.iter().map(|item| item.id).collect(),
    };
    let mut items = vec![prepared.root];
    items.extend(prepared.loadout.items);
    k.validate_restored_constructed_creatures(
        request.landblock,
        std::slice::from_ref(&entry),
        &items,
    )
    .unwrap();
    let graph = k
        .inventory
        .prepare_contained_forest(&[actor], &items, &prepared.loadout.containers)
        .unwrap();
    k.inventory.adopt_region_admission(graph);
    k.adopt_restored_constructed_creatures(vec![entry]);
    assert!(k.has_constructed_creature(actor));
    assert_eq!(
        k.promote_contained_creature(
            actor,
            1,
            GeneratorLocation {
                cell: 0x12340100,
                origin: [10., 10., 0.],
                rotation: [0., 0., 0., 1.],
            }
        ),
        Err(G::Busy)
    );
    assert!(k.inventory_item(actor).is_some());
    let saved: std::collections::BTreeSet<_> = items.iter().map(|item| item.id).collect();
    assert_eq!(
        k.preflight_restored_constructed_unload(request.landblock, &saved),
        Ok(())
    );
    let incomplete = std::collections::BTreeSet::from([actor]);
    assert_eq!(
        k.preflight_restored_constructed_unload(request.landblock, &incomplete),
        Err(G::Busy)
    );
    let ids: Vec<_> = items.iter().map(|item| item.id).collect();
    k.inventory.hold_region_items(&ids, 9).unwrap();
    k.inventory
        .evict_region_items(
            &items
                .iter()
                .map(|item| (item.id, item.revision))
                .collect::<Vec<_>>(),
            9,
        )
        .unwrap();
    k.retire_restored_constructed_region(request.landblock);
    assert!(!k.has_constructed_creature(actor));
}
#[test]
fn malformed_or_specialized_construction_retains_request_and_generic_transfer_is_rejected() {
    let (mut k, key, prepared) = prepared();
    let actor = prepared.root.id;
    let mut bad = prepared.clone();
    bad.weenie_type = 61;
    assert_eq!(k.admit_contained_creature(key, bad), Err(G::Invalid));
    assert!(k.generators.requests.contains_key(&key));
    assert!(k.inventory.item(actor).is_none());
    let mut bad = prepared.clone();
    bad.loadout.items[1].place = ItemPlace::World;
    assert_eq!(k.admit_contained_creature(key, bad), Err(G::Invalid));
    k.admit_contained_creature(key, prepared).unwrap();
    let item = k.inventory.item(EntityId(0x80000065)).unwrap().clone();
    let proposal = bace_inventory::InventoryProposal {
        changes: vec![bace_inventory::ItemChange {
            before: Some(item.clone()),
            after: item.clone(),
        }],
        participants: vec![(item.id, item.revision)],
        actor_burden: 0,
        requires_pickup_motion: false,
    };
    assert_eq!(
        k.inventory.reserve(actor, proposal),
        Err(bace_gameplay_api::InventoryRejection::InvalidState)
    );
    assert!(!k.inventory.reserved(actor));
    k.generator_control(key.generator, crate::GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    assert!(!k.has_constructed_creature(actor));
    assert!(k.inventory.item(actor).is_none());
    assert!(!k.generated_enchantments.has_state());
}

#[test]
fn promotion_preserves_pending_spell_deadline_without_restarting_construction() {
    let (mut k, key, prepared) = prepared();
    let actor = prepared.root.id;
    k.admit_contained_creature(key, prepared).unwrap();
    let due = k.tick + 3;
    let location = GeneratorLocation {
        cell: 0x12340100,
        origin: [10., 10., 0.],
        rotation: [0., 0., 0., 1.],
    };
    k.promote_contained_creature(actor, 1, location).unwrap();
    k.step_generated_enchantments(due - 1).unwrap();
    assert!(k.magic.registry(actor).is_none());
    k.step_generated_enchantments(due).unwrap();
    assert_eq!(k.magic.registry(actor).unwrap().entries().len(), 1);
    let revision = k.magic.registry(actor).unwrap().revision();
    k.step_generated_enchantments(due + 3).unwrap();
    assert_eq!(
        k.magic.registry(actor).unwrap().revision(),
        revision,
        "the original action is applied exactly once"
    );
}

#[test]
fn mixed_contain_forest_adopts_all_roots_in_source_order_or_none() {
    let (mut k, key, creature) = prepared();
    let actor = creature.root.id;
    let (_, _, shape) = setup();
    let Root::Item {
        items: mut plain, ..
    } = item(103, shape)
    else {
        panic!()
    };
    let mut ordinary = plain.remove(0);
    ordinary.place = ItemPlace::Contained {
        container: key.generator.entity,
        slot: 1,
        equipped: 0,
    };
    k.generators
        .requests
        .get_mut(&key)
        .unwrap()
        .entities
        .push(ordinary.id);
    let plain_id = ordinary.id;
    let mut items = vec![creature.root];
    items.extend(creature.loadout.items.iter().cloned());
    items.push(ordinary);
    let forest = crate::PreparedContainedForest {
        roots: vec![plain_id, actor],
        items,
        containers: creature.loadout.containers.clone(),
        creatures: vec![crate::PreparedConstructedCreature {
            entity: actor,
            weenie_type: 10,
            loadout: creature.loadout,
        }],
    };
    let mut bad = forest.clone();
    bad.items.last_mut().unwrap().place = ItemPlace::Contained {
        container: EntityId(999),
        slot: 0,
        equipped: 0,
    };
    assert!(k.admit_contained_forest(key, bad).is_err());
    assert!(k.inventory.item(actor).is_none());
    assert!(k.inventory.item(plain_id).is_none());
    assert!(k.generators.requests.contains_key(&key));
    let receipt = k.admit_contained_forest(key, forest).unwrap();
    assert_eq!(receipt.roots, vec![plain_id, actor]);
    assert_eq!(receipt.entities.len(), 4);
    assert!(k.inventory.item(actor).is_some());
    assert!(k.inventory.item(plain_id).is_some());
    assert!(k.has_constructed_creature(actor));
    assert!(!k.has_constructed_creature(plain_id));
    assert!(!k.world.contains_identity(actor));
    assert!(!k.world.contains_identity(plain_id));
    assert!(!k.generators.requests.contains_key(&key));
}

#[test]
fn nested_creature_retains_its_own_constructed_owner_below_container_root() {
    let (mut kernel, key, creature) = prepared();
    let actor = creature.root.id;
    let (_, _, shape) = setup();
    let Root::Item {
        items: mut plain, ..
    } = item(103, shape)
    else {
        panic!("container fixture");
    };
    let mut parent = plain.remove(0);
    parent.is_container = true;
    parent.place = ItemPlace::Contained {
        container: key.generator.entity,
        slot: 0,
        equipped: 0,
    };
    let parent_id = parent.id;
    let mut nested = creature.root;
    nested.place = ItemPlace::Contained {
        container: parent_id,
        slot: 0,
        equipped: 0,
    };
    kernel
        .generators
        .requests
        .get_mut(&key)
        .unwrap()
        .entities
        .push(parent_id);
    let mut items = vec![parent, nested];
    items.extend(creature.loadout.items.iter().cloned());
    let mut containers = vec![container(parent_id)];
    containers.extend(creature.loadout.containers.iter().copied());
    let receipt = kernel
        .admit_contained_forest(
            key,
            crate::PreparedContainedForest {
                roots: vec![parent_id],
                items,
                containers,
                creatures: vec![crate::PreparedConstructedCreature {
                    entity: actor,
                    weenie_type: 10,
                    loadout: creature.loadout,
                }],
            },
        )
        .unwrap();
    assert_eq!(receipt.roots, [parent_id]);
    assert_eq!(receipt.entities.len(), 4);
    assert!(kernel.has_constructed_creature(actor));
    assert!(!kernel.has_constructed_creature(parent_id));
    assert!(!kernel.world.contains_identity(actor));
}

#[test]
fn cow_uses_the_same_constructed_creature_lifecycle() {
    let (mut k, key, mut creature) = prepared();
    let actor = creature.root.id;
    creature.weenie_type = 15;
    k.admit_contained_creature(key, creature).unwrap();
    assert!(k.has_constructed_creature(actor));
    assert!(!k.world.contains_identity(actor));
    k.step_generated_enchantments(k.tick + 3).unwrap();
    assert_eq!(k.magic.registry(actor).unwrap().entries().len(), 1);
}
