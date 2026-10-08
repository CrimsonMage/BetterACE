use super::*;
use bace_geometry::Vec3;
use bace_physics::{
    CollisionFace, CollisionPlane, CollisionShape, CollisionSphere, GdlePolygon, GeometryCell,
    GeometryRegion,
};
fn fixture() -> (Kernel, PreparedResidentRegion) {
    let geometry = Arc::new(
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
    let mut kernel = Kernel::with_gameplay_limits(World::default(), 8, 8, 64).unwrap();
    kernel
        .configure_generators(
            Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
            1000,
            true,
        )
        .unwrap();
    let epoch = kernel.request_region(0x1234, false).unwrap();
    let item = InventoryItem {
        id: EntityId(100),
        revision: 7,
        template: 10,
        stack_key: 1,
        place: ItemPlace::World,
        stack: 1,
        maximum_stack: 1,
        unit_burden: 1,
        unit_value: 1,
        pack_slot: true,
        is_container: true,
        attuned: false,
        trade_reserved: false,
        active_pet: false,
        unique: false,
        quest_allowed: true,
        valid_wield: 0,
        incompatible_wield: 0,
        wield_requirements_met: false,
        structure: None,
    };
    let mut child = item.clone();
    child.id = EntityId(101);
    child.is_container = false;
    child.pack_slot = false;
    child.place = ItemPlace::Contained {
        container: item.id,
        slot: 0,
        equipped: 0,
    };
    let items = PreparedWorldRegionItems {
        containers: vec![InventoryContainer {
            id: item.id,
            revision: 7,
            root_owner: None,
            slots: 10,
            pack_slots: 10,
            burden_limit: 100,
            accessible: true,
            open: false,
            generation: epoch,
        }],
        roots: vec![PreparedWorldRegionRoot {
            entity: item.id,
            location: bace_gameplay_api::GeneratorLocation {
                cell: 0x12340001,
                origin: [10., 10., 0.],
                rotation: [0., 0., 0., 1.],
            },
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
            corpse: Some(PreparedWorldCorpse {
                state: bace_world::CorpseState {
                    operation: 55,
                    source: EntityId(5),
                    template: 10,
                    owner: Some(EntityId(5)),
                    items: vec![child.id],
                },
                expires_tick: 900,
                access: None,
            }),
        }],
        registries: vec![
            (item.id, EnchantmentRegistry::restore(8, 4, vec![]).unwrap()),
            (
                child.id,
                EnchantmentRegistry::restore(8, 3, vec![]).unwrap(),
            ),
        ],
        items: vec![item, child],
        constructed: vec![],
    };
    (
        kernel,
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
                epoch,
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
        },
    )
}
#[test]
fn restores_complete_tree_registry_and_original_corpse_deadline_atomically() {
    let (mut k, input) = fixture();
    assert!(k.admit_resident_region_complete(input).is_ok());
    assert!(k.world.is_anchor(EntityId(100)));
    assert_eq!(k.world.corpse(EntityId(100)).unwrap().operation, 55);
    assert_eq!(k.inventory.item(EntityId(101)).unwrap().revision, 7);
    assert_eq!(k.magic.registry(EntityId(100)).unwrap().revision(), 4);
    assert_eq!(k.corpse_expiry.deadlines[&EntityId(100)].tick, 900);
    assert_eq!(
        k.region_residency.state(0x1234).unwrap().phase,
        crate::RegionPhase::Active
    );
}
#[test]
fn occupied_region_refresh_adds_and_removes_plain_authored_visibility() {
    let (mut k, initial) = fixture();
    assert!(k.admit_resident_region_complete(initial).is_ok());
    let (_, mut addition) = fixture();
    let shape = addition.items.roots[0].shape.clone();
    addition.items = PreparedWorldRegionItems::default();
    addition.source_manifest = [2; 32];
    addition.region.revision = 2;
    addition.region.roots.push(crate::PreparedGeneratorRoot {
        script: None,
        entity: EntityId(200),
        location: bace_gameplay_api::GeneratorLocation {
            cell: 0x12340001,
            origin: [17., 17., 0.],
            rotation: [0., 0., 0., 1.],
        },
        shape,
        creature: None,
        loadout: None,
    });
    addition.refresh = Some(ResidentRegionRefresh {
        expected_epoch: 1,
        expected_revision: 1,
        expected_manifest: [1; 32],
        remove_roots: vec![],
    });
    assert!(k.refresh_resident_region_content(addition).is_ok());
    let mut visible = Vec::new();
    k.world
        .visibility_candidates(EntityId(100), &mut visible, 16)
        .unwrap();
    assert!(
        visible
            .iter()
            .any(|candidate| candidate.entity == EntityId(200))
    );
    assert!(k.inventory.item(EntityId(101)).is_some());
    assert_eq!(k.region_content_heads[&0x1234], (1, 2, [2; 32]));

    let (_, mut removal) = fixture();
    removal.items = PreparedWorldRegionItems::default();
    removal.source_manifest = [3; 32];
    removal.region.revision = 3;
    removal.refresh = Some(ResidentRegionRefresh {
        expected_epoch: 1,
        expected_revision: 2,
        expected_manifest: [2; 32],
        remove_roots: vec![EntityId(200)],
    });
    assert!(k.refresh_resident_region_content(removal).is_ok());
    k.world
        .visibility_candidates(EntityId(100), &mut visible, 16)
        .unwrap();
    assert!(
        !visible
            .iter()
            .any(|candidate| candidate.entity == EntityId(200))
    );
    assert!(k.inventory.item(EntityId(101)).is_some());
    assert_eq!(k.region_content_heads[&0x1234], (1, 3, [3; 32]));
}
#[test]
fn rejected_refresh_preserves_durable_region_tree_for_shutdown_checkpoint() {
    let (mut k, initial) = fixture();
    assert!(k.admit_resident_region_complete(initial).is_ok());
    let (_, mut stale) = fixture();
    stale.items = PreparedWorldRegionItems::default();
    stale.source_manifest = [2; 32];
    stale.region.revision = 2;
    stale.refresh = Some(ResidentRegionRefresh {
        expected_epoch: 1,
        expected_revision: 99,
        expected_manifest: [1; 32],
        remove_roots: vec![EntityId(100)],
    });
    assert!(k.refresh_resident_region_content(stale).is_err());
    assert!(k.inventory.item(EntityId(100)).is_some());
    assert!(k.inventory.item(EntityId(101)).is_some());
    k.region_residency.begin_drain();
    k.region_residency.advance(k.tick).unwrap();
    k.step_region_unloads();
    let ticket = k
        .take_region_unload_proposal()
        .expect("durable region checkpoint");
    assert_eq!(ticket.landblock, 0x1234);
    assert!(
        ticket
            .items
            .iter()
            .any(|item| item.item.id == EntityId(100))
    );
    assert!(
        ticket
            .items
            .iter()
            .any(|item| item.item.id == EntityId(101))
    );
}
#[test]
fn bad_forest_returns_all_inputs_without_geometry_or_partial_owner_state() {
    let (mut k, mut input) = fixture();
    input.items.items[1].place = ItemPlace::Contained {
        container: EntityId(999),
        slot: 0,
        equipped: 0,
    };
    let (_, returned) = k.admit_resident_region_complete(input).err().unwrap();
    assert_eq!(returned.items.items.len(), 2);
    assert_eq!(returned.items.registries.len(), 2);
    assert!(k.world.geometry().is_none());
    assert!(k.inventory.item(EntityId(100)).is_none());
    assert!(k.magic.registry(EntityId(100)).is_none());
    assert!(k.corpse_expiry.deadlines.is_empty());
    assert_eq!(
        k.region_residency.state(0x1234).unwrap().phase,
        crate::RegionPhase::Preparing
    );
}
#[test]
fn mismatched_registry_capacity_and_corpse_contents_cannot_partially_admit() {
    let (mut k, mut input) = fixture();
    input.items.roots[0]
        .corpse
        .as_mut()
        .unwrap()
        .state
        .items
        .clear();
    assert!(k.admit_resident_region_complete(input).is_err());
    assert!(!k.world.contains_identity(EntityId(100)));
    let (_, mut input) = fixture();
    input.items.registries[1].0 = EntityId(100);
    assert!(k.admit_resident_region_complete(input).is_err());
    assert!(!k.inventory.has_state());
}
