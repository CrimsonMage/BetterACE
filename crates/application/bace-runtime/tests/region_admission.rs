use bace_geometry::Vec3;
use bace_physics::{CollisionFace, CollisionPlane, GdlePolygon, GeometryCell, GeometryRegion};
use bace_runtime::simulation::{SimulationConfig, SimulationWorker, WorkerError};
use bace_simulation::{PreparedGeneratorRegion, PreparedResidentRegion, PreparedWorldRegionItems};
use std::sync::Arc;
fn prepared() -> PreparedResidentRegion {
    let geometry = Arc::new(
        GeometryRegion::prepare(vec![GeometryCell {
            id: 0x12340001,
            terrain: false,
            restriction: None,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![CollisionPlane {
                normal: Vec3::new(1., 0., 0.),
                distance: 0.,
            }],
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
            portals: vec![],
        }])
        .unwrap(),
    );
    let mut items = PreparedWorldRegionItems::default();
    items.registries.push((
        bace_types::EntityId(123),
        bace_magic::EnchantmentRegistry::restore(8, 77, vec![]).unwrap(),
    ));
    PreparedResidentRegion {
        source_manifest: [1; 32],
        refresh: None,
        bindings: vec![],
        visibility: geometry
            .cell_ids()
            .filter(|id| id & 0xffff >= 0x100)
            .map(|id| bace_world::PreparedCellVisibility {
                cell: bace_types::CellId(id),
                seen_outside: false,
                visible_cells: std::sync::Arc::from([]),
            })
            .collect(),
        region: PreparedGeneratorRegion {
            landblock: 0x1234,
            epoch: 1,
            revision: 1,
            geometry,
            roots: vec![],
            containers: vec![],
            definitions: vec![],
            templates: vec![],
            creatures: vec![],
        },
        items,
        dungeon: true,
        keep_alive: 0,
    }
}
#[test]
fn rejected_region_transfer_and_closed_ingress_return_full_registry_owner() {
    let kernel = bace_simulation::Kernel::new(bace_world::World::default(), 8).unwrap();
    let worker = SimulationWorker::spawn(kernel, SimulationConfig::default()).unwrap();
    let input = worker.input();
    assert!(input.try_admit_resident_region(5, prepared()).is_ok());
    let outcome = worker
        .region_admission_outcomes()
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(outcome.correlation, 5);
    let (_, returned) = outcome.result.err().unwrap();
    assert_eq!(returned.items.registries[0].1.revision(), 77);
    worker.shutdown().unwrap();
    let returned = input.try_admit_resident_region(6, *returned).err().unwrap();
    assert_eq!(returned.items.registries[0].1.revision(), 77);
}
#[test]
fn bounded_unread_region_outcomes_survive_worker_shutdown() {
    let kernel = bace_simulation::Kernel::new(bace_world::World::default(), 8).unwrap();
    let worker = SimulationWorker::spawn_with_output_capacity(
        kernel,
        SimulationConfig {
            command_capacity: 8,
            ..Default::default()
        },
        1,
    )
    .unwrap();
    let input = worker.input();
    for correlation in 1..=3 {
        assert!(
            input
                .try_admit_resident_region(correlation, prepared())
                .is_ok()
        );
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    let Err(WorkerError::RecoveryRequired(mut exit)) = worker.shutdown() else {
        panic!("unread owner transfers must require recovery");
    };
    let mut ids = Vec::new();
    for outcome in exit.region_admissions.outcomes.drain(..) {
        ids.push(outcome.correlation);
        assert_eq!(
            outcome.result.err().unwrap().1.items.registries[0]
                .1
                .revision(),
            77
        );
    }
    while let Some(outcome) = exit.kernel.take_region_admission_outcome() {
        ids.push(outcome.correlation);
        assert_eq!(
            outcome.result.err().unwrap().1.items.registries[0]
                .1
                .revision(),
            77
        );
    }
    for command in exit.unprocessed_commands {
        if let bace_simulation::Command::AdmitResidentRegion(request) = command {
            ids.push(request.correlation);
            assert_eq!(request.prepared.items.registries[0].1.revision(), 77);
        }
    }
    // The bounded output lane intentionally stops dispatch, so the third
    // transfer can still be queued on the recovered single kernel owner.
    for _ in 0..8 {
        if !exit.kernel.has_region_admission_work() {
            break;
        }
        exit.kernel.step().unwrap();
        while let Some(outcome) = exit.kernel.take_region_admission_outcome() {
            ids.push(outcome.correlation);
            assert_eq!(
                outcome.result.err().unwrap().1.items.registries[0]
                    .1
                    .revision(),
                77
            );
        }
    }
    ids.sort();
    assert_eq!(ids, vec![1, 2, 3]);
}
