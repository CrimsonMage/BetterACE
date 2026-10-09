use super::*;
use bace_geometry::Vec3;
use bace_physics::{
    CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon, GeometryCell,
    GeometryRegion,
};
fn world_kernel() -> Kernel {
    let mut world = bace_world::World::default();
    world
        .install_geometry(Arc::new(
            GeometryRegion::prepare(vec![GeometryCell {
                id: 0x01010001,
                terrain: false,
                restriction: None,
                solids: vec![],
                static_primitives: vec![],
                portals: vec![],
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
                faces: vec![CollisionFace {
                    polygon: GdlePolygon::prepare(vec![
                        Vec3::new(0., 0., 0.),
                        Vec3::new(24., 0., 0.),
                        Vec3::new(24., 24., 0.),
                        Vec3::new(0., 24., 0.),
                    ])
                    .unwrap(),
                    two_sided: false,
                    object: None,
                }],
            }])
            .unwrap(),
        ))
        .unwrap();
    let mut k = Kernel::new(world, 32).unwrap();
    k.configure_generators(Arc::new(RandomRoot::new([7; 32], 1).unwrap()), 1000, true)
        .unwrap();
    let mut def = definition(10, 64);
    def.kind = GeneratorKind::Object;
    def.location.origin = [10., 10., 0.];
    k.register_generator(Arc::new(def)).unwrap();
    k
}
fn shape(radius: f32) -> Arc<CollisionShape> {
    Arc::new(
        CollisionShape::prepare(
            vec![CollisionSphere {
                center: Vec3::new(0., 0., radius),
                radius,
            }],
            0.1,
            0.1,
        )
        .unwrap(),
    )
}
#[test]
fn partial_world_placement_keeps_successful_root_and_its_contents_only() {
    let mut k = world_kernel();
    let req = request(&mut k, 0x80000a01);
    let more = [
        EntityId(0x80000a02),
        EntityId(0x80000a03),
        EntityId(0x80000a04),
    ];
    k.supply_generator_request_ids(req.intent.key, 1, &more)
        .unwrap();
    let root = req.entities[0];
    let failed = more[1];
    let mut bag = item(root.0, 0, true);
    bag.place = ItemPlace::World;
    let mut bad_bag = item(failed.0, 0, true);
    bad_bag.place = ItemPlace::World;
    let items = [
        bag,
        item(more[0].0, root.0, false),
        bad_bag,
        item(more[2].0, failed.0, false),
    ];
    let result = k
        .admit_generated_item_trees(
            req.intent.key,
            &items,
            &[container(root.0, None), container(failed.0, None)],
            &[root, failed],
            Some(&[shape(0.5), shape(100.)]),
        )
        .unwrap();
    // Original ACE GeneratorProfile.Spawn: failed first spawns are returned for
    // suppression but destroyed; only surviving roots and their contents exist.
    let golden = include_str!("../fixtures/generator_spawn.csv")
        .lines()
        .find(|line| line.starts_with("64|1|1|"))
        .unwrap();
    let columns: Vec<_> = golden.split('|').collect();
    let roots = [root, failed];
    let ids = |column: &str| {
        column
            .split(',')
            .filter(|v| !v.is_empty())
            .map(|v| roots[v.parse::<usize>().unwrap()])
            .collect::<Vec<_>>()
    };
    assert_eq!(result.roots, ids(columns[4]));
    assert_eq!(result.entities, [root, more[0]]);
    assert_eq!(result.failed_roots, ids(columns[5]));
    assert_eq!(ids(columns[3]), roots);
    assert!(k.world().body(root).is_ok());
    assert!(k.inventory_item(more[0]).is_some());
    for id in [failed, more[2]] {
        assert!(k.world().body(id).is_err());
        assert!(k.inventory_item(id).is_none());
    }
    assert!(
        !k.notify_generated_entity(more[0], GeneratorNotification::PickUp)
            .unwrap(),
        "contents are not additional generator roots"
    );
    assert!(
        k.notify_generated_entity(root, GeneratorNotification::PickUp)
            .unwrap()
    );
}
#[test]
fn malformed_descendant_tree_is_rejected_before_any_root_or_membership_is_adopted() {
    let mut k = world_kernel();
    let req = request(&mut k, 0x80000b01);
    let child = EntityId(0x80000b02);
    k.supply_generator_request_ids(req.intent.key, 1, &[child])
        .unwrap();
    let root = req.entities[0];
    let mut bag = item(root.0, 0, true);
    bag.place = ItemPlace::World;
    assert!(matches!(
        k.admit_generated_item_trees(
            req.intent.key,
            &[bag.clone(), item(child.0, child.0, false)],
            &[container(root.0, None)],
            &[root],
            Some(&[shape(0.5)])
        ),
        Err(GeneratorServiceError::GeneratedItemForest {
            stage: "non-container parent",
            entity,
            slot: Some(0),
            inventory: None,
        }) if entity == child
    ));
    assert!(k.world().body(root).is_err());
    assert!(k.inventory_item(root).is_none());
    let good = k
        .admit_generated_item_trees(
            req.intent.key,
            &[bag, item(child.0, root.0, false)],
            &[container(root.0, None)],
            &[root],
            Some(&[shape(0.5)]),
        )
        .unwrap();
    assert_eq!(good.entities, [root, child]);
}
#[test]
fn contained_tree_admission_keeps_child_parent_and_only_root_generator_membership() {
    let mut k = kernel();
    k.register_inventory_container(container(10, None)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    let req = request(&mut k, 0x80000c01);
    let root = req.entities[0];
    let child = EntityId(0x80000c02);
    k.supply_generator_request_ids(req.intent.key, 1, &[child])
        .unwrap();
    let accepted = k
        .admit_generated_item_trees(
            req.intent.key,
            &[item(root.0, 10, true), item(child.0, root.0, false)],
            &[container(root.0, None)],
            &[root],
            None,
        )
        .unwrap();
    assert_eq!(accepted.roots, [root]);
    assert_eq!(accepted.entities, [root, child]);
    assert!(
        matches!(k.inventory_item(child).unwrap().place, ItemPlace::Contained { container, slot: 0, .. } if container == root)
    );
    assert!(
        !k.notify_generated_entity(child, GeneratorNotification::PickUp)
            .unwrap()
    );
    assert!(
        k.notify_generated_entity(root, GeneratorNotification::PickUp)
            .unwrap()
    );
}
#[test]
fn chest_profiles_use_refreshed_contained_slots_in_source_order() {
    // The pinned ACE GeneratorProfile.Spawn_Container inserts each generated
    // potion through Container.TryAddToInventory. WCID 30989 has three
    // Contain profiles (31196/31197/31198), each with init/max create 1,
    // and 120 main slots in the pinned world release.
    assert!(
        include_str!("../fixtures/generator_destinations.csv")
            .lines()
            .any(|row| row == "1|0|1|1|0")
    );
    let mut k = kernel();
    let mut chest = container(10, None);
    chest.slots = 120;
    chest.pack_slots = 10;
    k.register_inventory_container(chest).unwrap();
    let mut def = definition(10, 8);
    def.profiles[0].weenie_class_id = 31196;
    for (id, wcid) in [(1, 31197), (2, 31198)] {
        let mut profile = def.profiles[0].clone();
        profile.id = id;
        profile.weenie_class_id = wcid;
        def.profiles.push(profile);
    }
    def.initial_count = 3;
    def.maximum_count = 3;
    k.register_generator(Arc::new(def)).unwrap();
    k.supply_generator_id(EntityId(0x8000_0c11)).unwrap();
    k.supply_generator_id(EntityId(0x8000_0c12)).unwrap();
    k.supply_generator_id(EntityId(0x8000_0c13)).unwrap();
    k.step().unwrap();
    let first = k.take_generator_request().unwrap();
    let first_item = item(first.entities[0].0, 10, false);
    k.admit_generated_item_trees(first.intent.key, &[first_item], &[], &first.entities, None)
        .unwrap();
    k.step().unwrap();
    let second = k.take_generator_request().unwrap();
    assert_ne!(first.intent.key.profile_id, second.intent.key.profile_id);
    assert_ne!(first.intent.key.occurrence, second.intent.key.occurrence);
    let refreshed = k.refresh_generator_request(second.intent.key).unwrap();
    assert_eq!(refreshed.next_slots, Some((1, 0)));
    let mut second_item = item(second.entities[0].0, 10, false);
    second_item.place = ItemPlace::Contained {
        container: EntityId(10),
        slot: 1,
        equipped: 0,
    };
    k.admit_generated_item_trees(
        second.intent.key,
        &[second_item],
        &[],
        &second.entities,
        None,
    )
    .unwrap();
    k.step().unwrap();
    let third = k.take_generator_request().unwrap();
    assert_ne!(second.intent.key.profile_id, third.intent.key.profile_id);
    let refreshed = k.refresh_generator_request(third.intent.key).unwrap();
    assert_eq!(refreshed.next_slots, Some((2, 0)));
    let mut third_item = item(third.entities[0].0, 10, false);
    third_item.place = ItemPlace::Contained {
        container: EntityId(10),
        slot: 2,
        equipped: 0,
    };
    k.admit_generated_item_trees(third.intent.key, &[third_item], &[], &third.entities, None)
        .unwrap();
    assert!(k.inventory_item(first.entities[0]).is_some());
    assert!(k.inventory_item(second.entities[0]).is_some());
    assert!(k.inventory_item(third.entities[0]).is_some());
}
#[test]
fn unused_allocator_ids_only_retire_after_every_generator_owner_has_drained() {
    let mut k = kernel();
    k.supply_generator_id(EntityId(0x80000d01)).unwrap();
    k.discard_unused_generator_ids_after_drain().unwrap();
    k.supply_generator_id(EntityId(0x80000d02)).unwrap();
    k.register_generator(Arc::new(definition(10, 8))).unwrap();
    assert_eq!(
        k.discard_unused_generator_ids_after_drain(),
        Err(GeneratorServiceError::Busy)
    );
    assert!(k.supply_generator_id(EntityId(0x80000d02)).is_err());
}

#[test]
fn world_retirement_retains_accepted_pose_and_descendant_tree_across_retries() {
    let mut k = world_kernel();
    let req = request(&mut k, 0x80000f01);
    let root = req.entities[0];
    let child = EntityId(0x80000f02);
    let mut bag = item(root.0, 0, true);
    bag.place = ItemPlace::World;
    k.admit_generated_item_trees(
        req.intent.key,
        &[bag],
        &[container(root.0, None)],
        &[root],
        Some(&[shape(0.5)]),
    )
    .unwrap();
    // A durable descendant forces a single atomic retirement of the mixed tree.
    k.register_inventory_item(item(child.0, root.0, false))
        .unwrap();
    let accepted = k.world().actor_state(root).unwrap().1.position();
    k.generator_control(req.intent.key.generator, GeneratorControl::Destroy)
        .unwrap();
    k.step().unwrap();
    let ticket = k.take_generated_retirement().unwrap();
    assert_eq!(ticket.positions.len(), 1);
    assert_eq!(
        ticket.positions[&root].origin,
        [accepted.x, accepted.y, accepted.z]
    );
    assert_eq!(ticket.positions[&root].cell, 0x01010001);
    assert_eq!(ticket.inventory.proposal.changes.len(), 2);
    k.retry_generated_retirement(ticket.inventory.operation)
        .unwrap();
    assert_eq!(k.take_generated_retirement(), Some(ticket.clone()));
    k.confirm_generated_retirement(&receipt(&ticket.inventory))
        .unwrap();
    assert!(k.world().body(root).is_err());
    assert!(k.inventory_item(child).is_none());
}
