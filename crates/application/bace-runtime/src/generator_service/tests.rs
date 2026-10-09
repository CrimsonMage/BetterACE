//! Real owner-thread delivery with a bounded synthetic repository. Contained
//! objects need no geometry; this deliberately does not qualify client placement.
use super::*;
mod definition;
mod fixture;
use crate::saves::{SaveWorkerConfig, spawn_save_worker};
use crate::simulation::{SimulationConfig, SimulationWorker};
use fixture::*;
use std::time::Duration;
#[tokio::test]
async fn simultaneous_chest_occurrences_refresh_distinct_contained_slots() {
    let (_dir, mut service, mut regions) = service();
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel_with_count(service.config.random.clone(), 2),
        SimulationConfig::default(),
        4,
    )
    .unwrap();
    let saves = spawn_save_worker(NoSave, SaveWorkerConfig::default()).unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "two chest births stalled"
        );
        service
            .poll(&worker, &mut regions, &saves.handle)
            .await
            .unwrap();
        while let Ok(outcome) = worker.generator_outcomes().try_recv() {
            assert!(service.accept_generator_outcome(outcome).is_ok());
        }
        assert!(service.failure().is_none(), "{:?}", service.failure());
        assert_eq!(service.blocked().count(), 0);
        if regions.batches.len() == 2 && service.jobs.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let roots: Vec<_> = regions
        .batches
        .iter()
        .map(|batch| EntityId(batch.ids().next().expect("generated root")))
        .collect();
    let exit = worker.shutdown_recover().unwrap();
    let slots: Vec<_> = roots
        .iter()
        .map(|id| match exit.kernel.inventory_item(*id).unwrap().place {
            bace_inventory::ItemPlace::Contained {
                container, slot, ..
            } => {
                assert_eq!(container, EntityId(1));
                slot
            }
            other => panic!("unexpected generated place: {other:?}"),
        })
        .collect();
    assert_eq!(slots, vec![0, 1]);
}
#[tokio::test]
async fn cold_tree_and_ids_survive_source_pressure_until_exact_owner_receipt() {
    let (_dir, mut service, mut regions) = service();
    regions.reject = true;
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel(service.config.random.clone()),
        SimulationConfig::default(),
        1,
    )
    .unwrap();
    let saves = spawn_save_worker(NoSave, SaveWorkerConfig::default()).unwrap();
    let handle = saves.handle.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut retained = None;
    let mut allocations_at_hold = None;
    let mut held = 0;
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "service stalled: {:?} {:?}",
            service.failure(),
            service.blocked().collect::<Vec<_>>()
        );
        service.poll(&worker, &mut regions, &handle).await.unwrap();
        while let Ok(outcome) = worker.generator_outcomes().try_recv() {
            assert!(service.accept_generator_outcome(outcome).is_ok());
        }
        while let Ok(event) = worker.generator_events().try_recv() {
            service
                .observe_generator_event(&event, &mut regions)
                .unwrap();
        }
        assert!(service.failure().is_none(), "{:?}", service.failure());
        assert_eq!(service.blocked().count(), 0);
        if regions.attempts > 0 && regions.reject {
            let work = service.jobs.values().next().unwrap();
            let raw = work.raw.as_ref().unwrap();
            if let Some(previous) = &retained {
                assert!(Arc::ptr_eq(previous, raw));
                assert_eq!(
                    allocations_at_hold.as_ref().unwrap(),
                    &*service.store.0.lock().unwrap()
                );
            } else {
                retained = Some(raw.clone());
                allocations_at_hold = Some(service.store.0.lock().unwrap().clone());
                service.quiesce();
            }
            held += 1;
            if held == 8 {
                regions.reject = false;
            }
        }
        if !regions.batches.is_empty() && service.jobs.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let ids: Vec<_> = regions.batches[0].ids().map(EntityId).collect();
    assert_eq!(ids.len(), 2);
    let exit = worker.shutdown_recover().unwrap();
    let root = exit.kernel.inventory_item(ids[0]).unwrap();
    let child = exit.kernel.inventory_item(ids[1]).unwrap();
    assert_eq!(
        root.place,
        bace_inventory::ItemPlace::Contained {
            container: EntityId(1),
            slot: 0,
            equipped: 0
        }
    );
    assert_eq!(
        child.place,
        bace_inventory::ItemPlace::Contained {
            container: ids[0],
            slot: 0,
            equipped: 0
        }
    );
    assert!(exit.kernel.inventory_container(ids[0]).is_some());
    let source = regions.transient_source(ids[1]).unwrap();
    assert!(
        source
            .item
            .entity
            .state
            .properties
            .instance_ids
            .iter()
            .any(|p| p.id == 6 && p.value == ids[0].0)
    );
    assert_eq!(
        service
            .store
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|n| **n == 1)
            .count(),
        1,
        "one child allocation only"
    );
    // Active generator membership cannot be discarded to manufacture a clean shutdown.
    let Err(GeneratorShutdownError::Pending(service)) = service.shutdown() else {
        panic!("active owner recovery must retain service");
    };
    service.cold.shutdown().unwrap();
    saves.handle.close();
    saves.task.await.unwrap();
}

#[test]
fn tree_receipt_requires_complete_root_partition_and_all_descendants() {
    let (_dir, service, regions) = service();
    let mut def = definition::definition();
    def.profiles[0].where_create = 8;
    let key = bace_gameplay_api::GeneratorSpawnKey {
        generator: def.identity,
        profile_id: 0,
        occurrence: 1,
    };
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key,
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 1,
            random_identity: [5; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(100), EntityId(101)],
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let raw = materialization::materialize(
        &regions.region,
        &request,
        &service.config.treasure_assets,
        &service.config.random,
        false,
    )
    .unwrap();
    let materialization::Materialized::Items(items) = raw else {
        panic!("items");
    };
    let ready = materialization::bind_items(&request, &items, |_| {
        panic!("contained objects never require physical shapes")
    })
    .unwrap();
    let mut receipt = bace_simulation::GeneratorItemAdmission {
        key,
        roots: vec![EntityId(100)],
        entities: request.entities.clone(),
        failed_roots: vec![],
    };
    assert!(delivery::valid_tree_receipt(&ready.action, &receipt));
    receipt.entities.pop();
    assert!(!delivery::valid_tree_receipt(&ready.action, &receipt));
    receipt.entities.clear();
    receipt.roots.clear();
    assert!(!delivery::valid_tree_receipt(&ready.action, &receipt));
    receipt.failed_roots.push(EntityId(100));
    assert!(delivery::valid_tree_receipt(&ready.action, &receipt));
    receipt.failed_roots.push(EntityId(100));
    assert!(!delivery::valid_tree_receipt(&ready.action, &receipt));
    service.cold.shutdown().unwrap();
}

#[test]
fn rejected_world_npc_root_without_item_source_cannot_leave_orphan_publication() {
    let def = definition::definition();
    let key = bace_gameplay_api::GeneratorSpawnKey {
        generator: def.identity,
        profile_id: 0,
        occurrence: 1,
    };
    let rejected = EntityId(100);
    let accepted = EntityId(200);
    let rejected_gear = EntityId(101);
    let accepted_gear = EntityId(201);
    // NPC roots are prepared as public descriptions, while only their gear
    // enters the transient RegionItemSource batch. The authoritative admission
    // explicitly rejected one root and admitted the other root with its gear.
    let receipt = bace_simulation::GeneratorItemAdmission {
        key,
        roots: vec![accepted],
        entities: vec![accepted, accepted_gear],
        failed_roots: vec![rejected],
    };
    assert_eq!(
        delivery::failed_sources(&[rejected_gear, accepted_gear], &receipt),
        vec![rejected, rejected_gear]
    );
}

#[test]
fn id_reservation_receipt_reuses_materialization_and_retains_unexpected_outcomes() {
    let (_dir, mut service, regions) = service();
    let def = definition::definition();
    let key = bace_gameplay_api::GeneratorSpawnKey {
        generator: def.identity,
        profile_id: 0,
        occurrence: 1,
    };
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key,
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 1,
            random_identity: [5; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(100)],
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let raw = Arc::new(
        materialization::materialize(
            &regions.region,
            &request,
            &service.config.treasure_assets,
            &service.config.random,
            false,
        )
        .unwrap(),
    );
    service.accept_request(request.clone()).unwrap();
    let mut delivery = service
        .delivery(
            GeneratorAction::SupplyRequestIds {
                key,
                expected: 1,
                ids: vec![EntityId(101)],
            },
            delivery::Purpose::Reserve(key),
        )
        .unwrap();
    delivery.submitted = true;
    let correlation = delivery.correlation;
    let work = service.jobs.get_mut(&key).unwrap();
    work.raw = Some(raw.clone());
    work.ids = vec![EntityId(101)];
    work.delivery = Some(delivery);
    work.phase = Phase::Ids;
    service
        .accept_generator_outcome(GeneratorCommandOutcome {
            correlation,
            result: Err(GeneratorServiceError::Capacity),
            admission: None,
            request: None,
        })
        .unwrap();
    let work = service.jobs.get_mut(&key).unwrap();
    assert!(!work.delivery.as_ref().unwrap().submitted);
    assert_eq!(work.ids, vec![EntityId(101)]);
    work.delivery.as_mut().unwrap().submitted = true;
    let mut refreshed = request;
    refreshed.entities.push(EntityId(101));
    refreshed.next_slots = Some((7, 2));
    let receipt = GeneratorCommandOutcome {
        correlation,
        result: Ok(()),
        admission: None,
        request: Some(refreshed.clone()),
    };
    service.accept_generator_outcome(receipt.clone()).unwrap();
    let work = &service.jobs[&key];
    assert!(work.ids.is_empty());
    assert_eq!(work.request, refreshed);
    assert!(Arc::ptr_eq(work.raw.as_ref().unwrap(), &raw));
    service.accept_generator_outcome(receipt).unwrap();
    assert!(service.unexpected_outcome.is_some());
    assert!(service.failure().is_some());
    assert!(service.jobs.contains_key(&key));
    service.cold.shutdown().unwrap();
}

#[test]
fn shop_container_keeps_child_graph_and_merge_receipt_discards_whole_incoming_tree() {
    let (_dir, service, regions) = service();
    let def = definition::definition();
    let key = bace_gameplay_api::GeneratorSpawnKey {
        generator: def.identity,
        profile_id: 0,
        occurrence: 1,
    };
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key,
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Shop {
                vendor: EntityId(1),
            },
            first_spawn: true,
            due_tick: 1,
            random_identity: [5; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(100), EntityId(101)],
        landblock: 0x0101,
        next_slots: None,
    };
    let raw = materialization::materialize(
        &regions.region,
        &request,
        &service.config.treasure_assets,
        &service.config.random,
        false,
    )
    .unwrap();
    let materialization::Materialized::Items(items) = raw else {
        panic!("item tree");
    };
    let ready = materialization::bind_items(&request, &items, |_| {
        panic!("shop objects require no world shape")
    })
    .unwrap();
    let GeneratorAction::AdmitStockTrees { trees, .. } = &ready.action else {
        panic!("stock tree");
    };
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].items.len(), 1);
    assert_eq!(trees[0].templates[&EntityId(101)].weenie_id, 11);
    let mut receipt = bace_simulation::GeneratorItemAdmission {
        key,
        roots: vec![EntityId(100)],
        entities: vec![EntityId(100), EntityId(101)],
        failed_roots: vec![],
    };
    assert!(stock::valid_receipt(&ready.action, &receipt));
    receipt.entities.pop();
    assert!(!stock::valid_receipt(&ready.action, &receipt));
    receipt.entities.clear();
    receipt.roots = vec![EntityId(99)];
    assert!(stock::valid_receipt(&ready.action, &receipt));
    receipt.entities.push(EntityId(101));
    assert!(!stock::valid_receipt(&ready.action, &receipt));
    let mut batch = trees.clone();
    let mut merged = batch[0].clone();
    merged.root = EntityId(102);
    merged.items.clear();
    merged.containers.clear();
    merged.templates.clear();
    batch.push(merged);
    let action = GeneratorAction::AdmitStockTrees { key, trees: batch };
    receipt.roots = vec![EntityId(100), EntityId(100)];
    receipt.entities = vec![EntityId(100), EntityId(101)];
    assert!(
        stock::valid_receipt(&action, &receipt),
        "merged roots preserve one ordered result per incoming tree"
    );
    receipt.roots.pop();
    assert!(
        !stock::valid_receipt(&action, &receipt),
        "deduplicated roots lose source-to-retained correlation"
    );
    assert!(
        ready.sources.is_empty(),
        "vendor owns source templates; no duplicate world tree"
    );
    service.cold.shutdown().unwrap();
}
#[test]
fn treasure_forest_preserves_creature_and_container_root_order() {
    let (_dir, service, regions) = service();
    let def = definition::definition();
    let mut profile = def.profiles[0].clone();
    profile.where_create = 64;
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key: bace_gameplay_api::GeneratorSpawnKey {
                generator: def.identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile,
            destination: bace_gameplay_api::GeneratorDestination::Default(def.location),
            first_spawn: true,
            due_tick: 0,
            random_identity: [9; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(100)],
        landblock: 0x0101,
        next_slots: None,
    };
    let mut creature = template(200, false);
    creature.weenie_type = 10;
    let raw = mixed::materialize(
        &regions.region,
        &request,
        vec![creature.clone(), template(10, true), creature],
        &service.config.random,
        false,
    )
    .unwrap();
    assert_eq!(raw.count(), 4);
    assert!(raw.bytes().unwrap() > 0);
    let materialization::Materialized::Mixed(roots) = raw else {
        panic!("mixed roots");
    };
    assert!(
        matches!(&roots[0],materialization::Materialized::Creature{source,..}if source.weenie_id==200)
    );
    assert!(matches!(&roots[1],materialization::Materialized::Items(items)if items.len()==2));
    assert!(
        matches!(&roots[2],materialization::Materialized::Creature{source,..}if source.weenie_id==200)
    );
    let mut contained = request.clone();
    contained.intent.destination = bace_gameplay_api::GeneratorDestination::Contain {
        container: EntityId(1),
    };
    contained.next_slots = Some((0, 0));
    let mut creature = template(200, false);
    creature.weenie_type = 10;
    let raw = mixed::materialize(
        &regions.region,
        &contained,
        vec![creature.clone()],
        &service.config.random,
        false,
    )
    .unwrap();
    assert!(
        matches!(raw, materialization::Materialized::Mixed(ref roots)
            if matches!(roots.as_slice(), [materialization::Materialized::NestedItems { creatures, .. }]
                if creatures.len() == 1 && creatures[0].root_index == 0)),
        "single creature treasure retains the constructed forest owner path"
    );
    creature.weenie_type = 12;
    assert!(
        mixed::materialize(
            &regions.region,
            &contained,
            vec![creature],
            &service.config.random,
            false,
        )
        .is_err(),
        "specialized contained subtype cannot fall through to a generic item"
    );
    service.cold.shutdown().unwrap();
}

#[test]
fn contained_create_list_leaf_creature_keeps_source_parent_and_reserved_order() {
    let (_dir, service, mut regions) = service();
    let def = definition::definition();
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key: bace_gameplay_api::GeneratorSpawnKey {
                generator: def.identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 0,
            random_identity: [9; 16],
            random_key_version: 1,
        },
        entities: vec![EntityId(100), EntityId(101), EntityId(102)],
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let mut parent = template(10, true);
    parent
        .properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 2,
            destination_type: 1,
            weenie_class_id: 200,
            stack_size: 1,
            palette: 0,
            shade: 0.,
            try_to_bond: false,
        });
    let mut creature = template(200, false);
    creature.weenie_type = 10;
    Arc::get_mut(&mut regions.region)
        .unwrap()
        .catalog
        .templates
        .insert(200, Arc::new(creature));
    let raw = mixed::materialize(
        &regions.region,
        &request,
        vec![parent],
        &service.config.random,
        false,
    )
    .unwrap();
    let materialization::Materialized::Mixed(trees) = raw else {
        panic!("contained creature tree must use constructed forest admission");
    };
    let [materialization::Materialized::NestedItems { items, creatures }] = trees.as_slice() else {
        panic!("source-ordered contained CreateList tree");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[2].source.weenie_type, 10);
    assert_eq!(items[2].generator_parent_index, Some(0));
    assert_eq!(items[2].source_destination, Some(1));
    assert_eq!(creatures.len(), 1);
    assert_eq!(creatures[0].root_index, 2);
    assert!(creatures[0].gear.is_empty());
    service.cold.shutdown().unwrap();
}

#[test]
fn contained_creature_and_cow_construct_selected_wield_loadouts_with_reserved_ids() {
    let (_dir, service, mut regions) = service();
    let def = definition::definition();
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key: bace_gameplay_api::GeneratorSpawnKey {
                generator: def.identity,
                profile_id: 0,
                occurrence: 1,
            },
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 0,
            random_identity: [9; 16],
            random_key_version: 1,
        },
        entities: (100..106).map(EntityId).collect(),
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let mut parent = template(10, true);
    for (database_record_id, weenie_class_id) in [(2, 210), (3, 211)] {
        parent
            .properties
            .create_list
            .push(bace_content::CreateListEntry {
                database_record_id,
                destination_type: 1,
                weenie_class_id,
                stack_size: 1,
                palette: 0,
                shade: 0.0,
                try_to_bond: false,
            });
    }
    let mut weapon = template(212, false);
    weapon.weenie_type = 3;
    weapon.properties.ints.push(bace_content::Property {
        id: 9,
        value: 0x100000,
    });
    for (id, kind) in [(210, 10), (211, 15)] {
        let mut creature = template(id, false);
        creature.weenie_type = kind;
        creature
            .properties
            .create_list
            .push(bace_content::CreateListEntry {
                database_record_id: 4,
                destination_type: 2,
                weenie_class_id: 212,
                stack_size: 1,
                palette: 0,
                shade: 0.0,
                try_to_bond: false,
            });
        Arc::get_mut(&mut regions.region)
            .unwrap()
            .catalog
            .templates
            .insert(id, Arc::new(creature));
    }
    Arc::get_mut(&mut regions.region)
        .unwrap()
        .catalog
        .templates
        .insert(212, Arc::new(weapon));
    let raw = mixed::materialize(
        &regions.region,
        &request,
        vec![parent],
        &service.config.random,
        false,
    )
    .unwrap();
    assert_eq!(raw.count(), 6);
    let materialization::Materialized::Mixed(trees) = raw else {
        panic!("constructed Contain forest");
    };
    let [materialization::Materialized::NestedItems { items, creatures }] = trees.as_slice() else {
        panic!("nested Creature sidecars");
    };
    assert_eq!(items.len(), 4);
    assert_eq!(creatures.len(), 2);
    assert_eq!(creatures[0].root_index, 2);
    assert_eq!(creatures[0].gear_start, 4);
    assert_eq!(creatures[0].source.weenie_type, 10);
    assert_eq!(creatures[1].root_index, 3);
    assert_eq!(creatures[1].gear_start, 5);
    assert_eq!(creatures[1].source.weenie_type, 15);
    for sidecar in creatures {
        assert_eq!(sidecar.gear.len(), 1);
        assert_eq!(sidecar.gear[0].source.weenie_id, 212);
        assert_ne!(sidecar.gear[0].wielded_location, 0);
        assert_eq!(sidecar.gear[0].equip_order, Some(0));
        assert!(!sidecar.gear[0].death_drop);
    }
    let mut table = None;
    let prepared = materialization::bind(
        Err("contained roots do not require world geometry".into()),
        &mut table,
        &regions.region.generation,
        &regions.region,
        &request,
        &trees[0],
    )
    .unwrap();
    let GeneratorAction::AdmitItemTrees { items, roots, .. } = prepared.action else {
        panic!("generic prefix of nested forest");
    };
    assert_eq!(roots, [EntityId(100)]);
    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        (100..104).map(EntityId).collect::<Vec<_>>()
    );
    assert_eq!(prepared.sources.len(), 4);
    service.cold.shutdown().unwrap();
}

#[test]
fn nested_creature_gear_creature_recursively_reserves_a_distinct_constructed_root() {
    let (_dir, service, mut regions) = service();
    let def = definition::definition();
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key: bace_gameplay_api::GeneratorSpawnKey {
                generator: def.identity,
                profile_id: 0,
                occurrence: 2,
            },
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 0,
            random_identity: [8; 16],
            random_key_version: 1,
        },
        entities: (100..105).map(EntityId).collect(),
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let mut parent = template(10, true);
    parent
        .properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 2,
            destination_type: 1,
            weenie_class_id: 210,
            stack_size: 1,
            palette: 0,
            shade: 0.0,
            try_to_bond: false,
        });
    let mut creature = template(210, false);
    creature.weenie_type = 10;
    creature
        .properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 3,
            destination_type: 2,
            weenie_class_id: 211,
            stack_size: 1,
            palette: 0,
            shade: 0.0,
            try_to_bond: false,
        });
    let mut cow = template(211, false);
    cow.weenie_type = 15;
    cow.properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 4,
            destination_type: 2,
            weenie_class_id: 212,
            stack_size: 1,
            palette: 0,
            shade: 0.0,
            try_to_bond: false,
        });
    let mut weapon = template(212, false);
    weapon.weenie_type = 3;
    weapon.properties.ints.push(bace_content::Property {
        id: 9,
        value: 0x100000,
    });
    for source in [creature, cow, weapon] {
        Arc::get_mut(&mut regions.region)
            .unwrap()
            .catalog
            .templates
            .insert(source.weenie_id, Arc::new(source));
    }
    let raw = mixed::materialize(
        &regions.region,
        &request,
        vec![parent],
        &service.config.random,
        false,
    )
    .unwrap();
    assert_eq!(raw.count(), 5);
    let materialization::Materialized::Mixed(trees) = raw else {
        panic!("nested constructed forest");
    };
    let [materialization::Materialized::NestedItems { items, creatures }] = trees.as_slice() else {
        panic!("nested constructed sidecars");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(creatures.len(), 2);
    assert_eq!((creatures[0].root_index, creatures[0].gear_start), (2, 3));
    assert_eq!(creatures[0].gear[0].source.weenie_type, 15);
    assert_eq!(creatures[0].gear[0].wielded_location, 0);
    assert_eq!((creatures[1].root_index, creatures[1].gear_start), (3, 4));
    assert_eq!(creatures[1].gear[0].source.weenie_id, 212);
    assert_ne!(creatures[1].gear[0].wielded_location, 0);
    service.cold.shutdown().unwrap();
}

#[test]
fn contained_creature_root_and_nested_cow_share_one_constructed_forest() {
    let (_dir, service, mut regions) = service();
    let def = definition::definition();
    let request = GeneratorHostRequest {
        intent: bace_gameplay_api::GeneratorSpawnIntent {
            key: bace_gameplay_api::GeneratorSpawnKey {
                generator: def.identity,
                profile_id: 0,
                occurrence: 3,
            },
            profile: def.profiles[0].clone(),
            destination: bace_gameplay_api::GeneratorDestination::Contain {
                container: EntityId(1),
            },
            first_spawn: true,
            due_tick: 0,
            random_identity: [7; 16],
            random_key_version: 1,
        },
        entities: (100..103).map(EntityId).collect(),
        landblock: 0x0101,
        next_slots: Some((0, 0)),
    };
    let mut creature = template(210, false);
    creature.weenie_type = 10;
    creature
        .properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 3,
            destination_type: 2,
            weenie_class_id: 211,
            stack_size: 1,
            palette: 0,
            shade: 0.0,
            try_to_bond: false,
        });
    let mut cow = template(211, false);
    cow.weenie_type = 15;
    cow.properties
        .create_list
        .push(bace_content::CreateListEntry {
            database_record_id: 4,
            destination_type: 2,
            weenie_class_id: 212,
            stack_size: 1,
            palette: 0,
            shade: 0.0,
            try_to_bond: false,
        });
    let mut weapon = template(212, false);
    weapon.weenie_type = 3;
    weapon.properties.ints.push(bace_content::Property {
        id: 9,
        value: 0x100000,
    });
    for source in [creature.clone(), cow, weapon] {
        Arc::get_mut(&mut regions.region)
            .unwrap()
            .catalog
            .templates
            .insert(source.weenie_id, Arc::new(source));
    }
    let raw = mixed::materialize(
        &regions.region,
        &request,
        vec![creature],
        &service.config.random,
        false,
    )
    .unwrap();
    assert_eq!(raw.count(), 3);
    let materialization::Materialized::Mixed(trees) = raw else {
        panic!("contained constructed root");
    };
    let [materialization::Materialized::NestedItems { items, creatures }] = trees.as_slice() else {
        panic!("constructed root and child sidecars");
    };
    assert_eq!(items.len(), 1);
    assert_eq!((creatures[0].root_index, creatures[0].gear_start), (0, 1));
    assert_eq!(creatures[0].gear[0].source.weenie_type, 15);
    assert_eq!((creatures[1].root_index, creatures[1].gear_start), (1, 2));
    assert_eq!(creatures[1].gear[0].source.weenie_id, 212);
    service.cold.shutdown().unwrap();
}

#[test]
fn nested_creature_container_uses_creature_capacity_and_access_policy() {
    let mut source = template(200, false);
    source.weenie_type = 10;
    source
        .properties
        .ints
        .iter_mut()
        .find(|p| p.id == 6)
        .unwrap()
        .value = -1;
    source
        .properties
        .ints
        .iter_mut()
        .find(|p| p.id == 7)
        .unwrap()
        .value = -1;
    let (_, container) = crate::generator_items::prepare_inventory_item(
        &source,
        EntityId(200),
        1,
        bace_inventory::ItemPlace::Contained {
            container: EntityId(100),
            slot: 0,
            equipped: 0,
        },
    )
    .unwrap();
    let container = container.unwrap();
    assert_eq!((container.slots, container.pack_slots), (0, 0));
    assert!(!container.accessible);
    assert_eq!(container.burden_limit, u64::MAX);
}

#[test]
fn transient_world_metadata_is_budgeted_without_inventing_an_accepted_pose() {
    let (_dir, service, _) = service();
    let state = template(10, true);
    let source = crate::region_unload_saves::RegionItemSource {
        item: crate::game_inventory::FrozenInventoryItem {
            source_destination: None,
            corpse: None,
            construction: None,
            entity: bace_storage_codec::EntitySaveV1 {
                object_id: 100,
                template_revision: 1,
                mutation_revision: 1,
                state,
            },
            placement: None,
            enchantments: vec![],
            persisted_version: 0,
        },
        corpse: None,
    };
    let batch = PreparedRegionSources::prepare(vec![source]).unwrap();
    assert!(batch.sources()[0].item.placement.is_none());
    assert_eq!(batch.ids().collect::<Vec<_>>(), vec![100]);
    service.cold.shutdown().unwrap();
}
