use super::*;
use crate::saves::{SaveBackend, SaveFailure};
use bace_content::{CreateListEntry, Property, WeenieV1};
use bace_persistence::{OperationOutcome, SaveAck, SaveSnapshot, StoredAggregate};
use std::sync::{Mutex, atomic::AtomicBool};
#[derive(Clone, Default)]
pub(super) struct Repository(pub Arc<Mutex<Vec<u16>>>);
impl GeneratorRepository for Repository {
    async fn allocate(&self, count: u16) -> Result<Vec<u32>, String> {
        let mut log = self.0.lock().unwrap();
        let start = 0x80000000 + log.iter().map(|n| u32::from(*n)).sum::<u32>();
        log.push(count);
        Ok((start..start + u32::from(count)).collect())
    }
    async fn load(&self, _: u32) -> Result<Option<StoredAggregate>, String> {
        Err("unexpected baseline read".into())
    }
}
pub(super) struct NoSave;
impl SaveBackend for NoSave {
    async fn routine(&self, _: &[SaveSnapshot]) -> Result<Vec<SaveAck>, SaveFailure> {
        panic!("no routine state in fixture")
    }
    async fn valuable(&self, _: &str, _: &[SaveSnapshot]) -> Result<OperationOutcome, SaveFailure> {
        panic!("transient generation must not publish valuable success")
    }
}
pub(super) struct Regions {
    pub region: Arc<PreparedRegionActivation>,
    pub batches: Vec<PreparedRegionSources>,
    pub reject: bool,
    pub attempts: usize,
}
impl GeneratorRegions for Regions {
    fn prepared_region(&self, landblock: u16) -> Option<&Arc<PreparedRegionActivation>> {
        (landblock == 0x0101).then_some(&self.region)
    }
    fn record_sources(
        &mut self,
        _: u16,
        batch: PreparedRegionSources,
    ) -> Result<(), Box<PreparedRegionSources>> {
        self.attempts += 1;
        if self.reject {
            return Err(Box::new(batch));
        }
        self.batches.push(batch);
        Ok(())
    }
    fn transient_source(
        &self,
        id: EntityId,
    ) -> Option<&crate::region_unload_saves::RegionItemSource> {
        self.batches
            .iter()
            .flat_map(|b| b.sources())
            .find(|s| s.item.entity.object_id == id.0)
    }
    fn forget_transient_sources(&mut self, ids: &[EntityId]) {
        assert!(ids.is_empty(), "no failed placements in contained fixture");
    }
    fn forget_transient_tree(&mut self, _: EntityId) {
        panic!("no lifecycle action in fixture")
    }
}
pub(super) fn template(id: u32, container: bool) -> WeenieV1 {
    let mut w = WeenieV1 {
        schema_version: 1,
        weenie_id: id,
        class_name: format!("generated_fixture_{id}"),
        weenie_type: if container { 21 } else { 51 },
        last_modified: None,
        properties: Default::default(),
    };
    w.properties.ints = vec![
        Property { id: 6, value: 8 },
        Property { id: 7, value: 2 },
        Property { id: 11, value: 1 },
        Property { id: 12, value: 1 },
    ];
    if container {
        w.properties.create_list.push(CreateListEntry {
            database_record_id: 1,
            destination_type: 1,
            weenie_class_id: 11,
            stack_size: 1,
            palette: 0,
            shade: 0.,
            try_to_bond: false,
        });
    }
    w
}
pub(super) fn service() -> (tempfile::TempDir, GeneratorService<Repository>, Regions) {
    let dir = tempfile::tempdir().unwrap();
    let templates = vec![template(10, true), template(11, false)];
    let pack =
        bace_content_tools::build_world_pack(&templates, &[], dir.path(), &AtomicBool::new(false))
            .unwrap();
    let generation = Arc::new(
        bace_storage_codec::load_manifest(&pack.manifest, Default::default())
            .unwrap()
            .open(dir.path(), Default::default())
            .unwrap(),
    );
    let templates: BTreeMap<_, _> = templates
        .into_iter()
        .map(|w| (w.weenie_id, Arc::new(w)))
        .collect();
    let geometry = Arc::new(
        bace_physics::GeometryRegion::prepare(vec![bace_physics::GeometryCell {
            id: 0x01010001,
            terrain: false,
            restriction: None,
            solids: vec![],
            static_primitives: vec![],
            boundary: vec![bace_physics::CollisionPlane {
                normal: bace_geometry::Vec3::new(1., 0., 0.),
                distance: 0.,
            }],
            faces: vec![],
            portals: vec![],
        }])
        .unwrap(),
    );
    let region = Arc::new(PreparedRegionActivation {
        source_manifest: bace_storage_codec::load_manifest(&pack.manifest, Default::default())
            .unwrap()
            .content_hash(Default::default())
            .unwrap(),
        npc_recovery: BTreeMap::new(),
        generation: generation.clone(),
        visibility: vec![],
        fence: crate::region_activation::RegionActivationFence {
            token: 1,
            activation_epoch: 1,
            landblock: 0x0101,
            content_generation: generation.revision(),
        },
        content: crate::world_content::PreparedRegion {
            templates: templates.clone(),
            content_generation: generation.revision(),
            landblock: 0x0101,
            instances: vec![],
            encounters: vec![],
        },
        catalog: crate::generator_catalog::PreparedGeneratorCatalog {
            templates,
            death: BTreeMap::new(),
            wielded: BTreeMap::new(),
            treasure: BTreeMap::new(),
        },
        item_spells: Err("no equipped spells in contained fixture".into()),
        geometry,
        physical: BTreeMap::new(),
        creatures: BTreeMap::new(),
    });
    let config = GeneratorServiceConfig {
        world_epoch: 9,
        capacity: 2,
        assets: crate::region_activation::RegionAssetManifest {
            portal: dir.path().join("absent-portal.dat"),
            cell: dir.path().join("absent-cell.dat"),
            portal_sha256: String::new(),
            cell_sha256: String::new(),
        },
        generation,
        treasure_assets: Arc::new(Default::default()),
        random: Arc::new(bace_random::RandomRoot::new([7; 32], 1).unwrap()),
        drop_plain_wield: false,
    };
    let service = GeneratorService::start(Repository::default(), config).unwrap();
    (
        dir,
        service,
        Regions {
            region,
            batches: vec![],
            reject: false,
            attempts: 0,
        },
    )
}
pub(super) fn kernel(random: Arc<bace_random::RandomRoot>) -> bace_simulation::Kernel {
    kernel_with_count(random, 1)
}
pub(super) fn kernel_with_count(
    random: Arc<bace_random::RandomRoot>,
    count: i32,
) -> bace_simulation::Kernel {
    let mut k = bace_simulation::Kernel::new(bace_world::World::default(), 16).unwrap();
    k.configure_generators(random, 1000, true).unwrap();
    k.register_inventory_container(bace_inventory::InventoryContainer {
        id: EntityId(1),
        revision: 1,
        root_owner: None,
        slots: 32,
        pack_slots: 8,
        burden_limit: 100000,
        accessible: true,
        open: true,
        generation: 1,
    })
    .unwrap();
    let mut def = definition::definition();
    def.kind = bace_gameplay_api::GeneratorKind::Container;
    def.initial_count = count;
    def.maximum_count = count;
    def.profiles[0].init_create = count;
    def.profiles[0].max_create = count;
    def.profiles[0].where_create = 8;
    k.register_generator(Arc::new(def)).unwrap();
    k
}
