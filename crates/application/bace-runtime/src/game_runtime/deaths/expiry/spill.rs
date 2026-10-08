//! Verified DAT shape admission for the original corpse spill positions.
use super::*;
use crate::game_inventory::FrozenInventoryItem;
pub(super) struct Prepared {
    pub prepared: Option<bace_simulation::PreparedCorpseSpill>,
    pub positions: BTreeMap<u32, bace_content::Position>,
    pub visibility: Vec<crate::visibility_assets::PreparedVisibilityObject>,
}
pub(super) fn prepare(
    ticket: &CorpseExpiryTicket,
    items: &[FrozenInventoryItem],
    geometry: Option<(
        crate::region_activation::RegionAssetManifest,
        Arc<bace_physics::GeometryRegion>,
    )>,
) -> Result<Prepared, String> {
    let mut result = Prepared {
        prepared: None,
        positions: BTreeMap::new(),
        visibility: Vec::new(),
    };
    let Some(intent) = &ticket.spill else {
        return Ok(result);
    };
    let (manifest, geometry) = geometry.ok_or("corpse spill geometry missing")?;
    let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest)?;
    let mut actors = Vec::with_capacity(intent.roots.len());
    for id in &intent.roots {
        let source = &items
            .iter()
            .find(|i| i.entity.object_id == id.0)
            .ok_or("spill item source missing")?
            .entity
            .state;
        let scale = source
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map(|p| p.value);
        let position = crate::corpse_expiry_saves::corpse_spill_position(intent.position, scale)?;
        let heading = 2. * intent.position.rotation[2].atan2(intent.position.rotation[3]);
        let shape = assets.prepare_world_item_shape(source)?;
        let body = bace_physics::Body::spawn_geometry(
            &geometry,
            bace_physics::GeometrySpawn {
                cell: intent.position.cell,
                position: bace_geometry::Vec3::new(
                    position.position_x,
                    position.position_y,
                    position.position_z,
                ),
                shape,
                capabilities: bace_motion::Capabilities {
                    speed: 0.,
                    jump_impulse: 0.,
                },
                heading,
                maximum_turn_rate: 0.,
            },
        )
        .map_err(|e| e.to_string())?;
        let p = body.accepted().position();
        result.positions.insert(
            id.0,
            bace_content::Position {
                obj_cell_id: intent.position.cell,
                position_x: p.x,
                position_y: p.y,
                position_z: p.z,
                rotation_x: intent.position.rotation[0],
                rotation_y: intent.position.rotation[1],
                rotation_z: intent.position.rotation[2],
                rotation_w: intent.position.rotation[3],
            },
        );
        actors.push(bace_entity::Actor {
            id: *id,
            cell: bace_types::CellId(intent.position.cell),
            body,
        });
    }
    let mut rows = Vec::with_capacity(intent.roots.len());
    for id in &intent.roots {
        let item = items
            .iter()
            .find(|item| item.entity.object_id == id.0)
            .ok_or("spill visible source missing")?;
        let revision = item
            .persisted_version
            .checked_add(1)
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value != 0)
            .ok_or("spill visible revision invalid")?;
        rows.push(crate::visibility_assets::VisibilitySource {
            entity: *id,
            incarnation: ticket.inventory.operation,
            revision,
            source: &item.entity.state,
            equipment: vec![],
            missile_combat: false,
        });
    }
    result.visibility = assets.prepare_visibility_sources(rows)?;
    result.prepared = Some(bace_simulation::PreparedCorpseSpill {
        operation: ticket.inventory.operation,
        actors,
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content_tools::decode;
    use bace_storage_codec::{EntitySaveV1, PackKey, PackLookup, load_manifest};
    use bace_types::EntityId;

    #[test]
    #[ignore = "requires approved BACE_DAT_DIRECTORY and complete BACE_WORLD_MANIFEST"]
    fn actual_dat_spill_admits_source_pose_and_visible_item() {
        let dat = std::path::PathBuf::from(
            std::env::var_os("BACE_DAT_DIRECTORY").expect("approved DAT directory"),
        );
        let path = std::path::PathBuf::from(
            std::env::var_os("BACE_WORLD_MANIFEST").expect("complete world manifest"),
        );
        let manifest = crate::region_activation::RegionAssetManifest {
            portal: dat.join("client_portal.dat"),
            cell: dat.join("client_cell_1.dat"),
            portal_sha256: "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4"
                .into(),
            cell_sha256: "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e".into(),
        };
        let mut assets = crate::region_activation::VerifiedRegionAssets::open(&manifest).unwrap();
        let geometry = assets.prepare_geometry(0x8602).unwrap();
        let catalog = load_manifest(&path, Default::default()).unwrap();
        let pack = catalog
            .open(path.parent().unwrap(), Default::default())
            .unwrap();
        let PackLookup::Record(tables) = pack
            .lookup(PackKey {
                namespace: 52,
                id: 1,
            })
            .unwrap()
        else {
            panic!("complete world pack lacks ACE table set");
        };
        let tables = bace_content_tools::decode_treasure_table_set(tables.bytes()).unwrap();
        if bace_loot::ace_tables::active_id().is_none() {
            bace_loot::ace_tables::install(tables).unwrap();
        } else {
            assert_eq!(bace_loot::ace_tables::active_id(), Some(tables.id));
        }
        let PackLookup::Record(record) = pack
            .lookup(PackKey {
                namespace: 1,
                id: 273,
            })
            .unwrap()
        else {
            panic!("complete world pack lacks Pyreal WCID 273");
        };
        let source: bace_content::WeenieV1 = decode(record.bytes()).unwrap();
        let scale = source
            .properties
            .floats
            .iter()
            .find(|value| value.id == 39)
            .map(|value| value.value);
        let id = EntityId(0x8000_0100);
        let corpse = EntityId(0x8000_0101);
        let cell = 0x8602_0001;
        let floor = geometry
            .cell(cell)
            .unwrap()
            .faces
            .iter()
            .take(2)
            .find_map(|face| {
                let plane = face.polygon.plane();
                let z = -(plane.normal.x * 12. + plane.normal.y * 12. + plane.distance)
                    / plane.normal.z;
                face.polygon
                    .contains_projection(bace_geometry::Vec3::new(12., 12., z), 0.001)
                    .then_some(z)
            })
            .expect("approved DAT outdoor terrain at 0x86020001");
        let location = bace_gameplay_api::GeneratorLocation {
            // The 0x8602 land lattice and floor come from the approved cell
            // DAT; the source corpse pose is one metre above that floor.
            cell,
            origin: [12., 12., floor + 1.],
            rotation: [0., 0., 0., 1.],
        };
        let ticket = CorpseExpiryTicket {
            corpse,
            death_operation: 13,
            expires_tick: 30,
            inventory: bace_simulation::InventoryTicket {
                operation: 14,
                actor: corpse,
                proposal: bace_inventory::InventoryProposal {
                    changes: vec![],
                    participants: vec![],
                    actor_burden: 0,
                    requires_pickup_motion: false,
                },
            },
            transient: vec![],
            spill: Some(bace_simulation::CorpseSpillIntent {
                roots: vec![id],
                position: location,
            }),
            enchantments: Default::default(),
        };
        let item = FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            enchantments: vec![],
            entity: EntitySaveV1 {
                object_id: id.0,
                template_revision: 1,
                mutation_revision: 4,
                state: source,
            },
            placement: None,
            persisted_version: 4,
        };
        let prepared = prepare(&ticket, &[item], Some((manifest, geometry))).unwrap();
        let expected = crate::corpse_expiry_saves::corpse_spill_position(location, scale).unwrap();
        assert_eq!(prepared.positions[&id.0], expected);
        let world = prepared.prepared.unwrap();
        assert_eq!(world.operation, 14);
        assert_eq!(world.actors.len(), 1);
        assert_eq!(
            world.actors[0].body.accepted().position().z,
            expected.position_z
        );
        assert_eq!(prepared.visibility.len(), 1);
        assert_eq!(prepared.visibility[0].description.object_id, id.0);
        assert_eq!(prepared.visibility[0].incarnation, 14);
        assert_eq!(prepared.visibility[0].revision, 5);
        let create = prepared.visibility[0]
            .description
            .encode_create(bace_wire::ObjectCodecLimits {
                max_message_bytes: 1024 * 1024,
                max_model_entries: 4096,
                max_children: 128,
                max_restrictions: 256,
                max_motion_commands: 32,
                max_string_bytes: 4096,
            })
            .unwrap();
        assert_eq!(u32::from_le_bytes(create[..4].try_into().unwrap()), 0xf745);
        assert_eq!(u32::from_le_bytes(create[4..8].try_into().unwrap()), id.0);
    }
}
