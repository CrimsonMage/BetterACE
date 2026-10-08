//! Cold map-click preparation from fingerprint-verified DAT records.
//! Pinned ACE PositionExtensions.AdjustMapCoords, BuildingObj.GetMinZ and
//! LandblockStruct.CalcWater. Missing geometry never preserves client Z.
use bace_dat::{DatArchive, EnvCell, Environment, Landblock, LandblockInfo, RegionLand};
use bace_gameplay_api::staff::{MapTeleportRequest, PreparedMapTeleport, StaffDestination};
use bace_geometry::Vec3;
use bace_physics::GeometryRegion;
use std::collections::{BTreeMap, BTreeSet};

pub struct StaffMapAssets {
    portal: DatArchive,
    cell: DatArchive,
    land: RegionLand,
}
impl StaffMapAssets {
    pub fn open(manifest: &crate::region_activation::RegionAssetManifest) -> Result<Self, String> {
        for (path, hash) in [
            (&manifest.portal, &manifest.portal_sha256),
            (&manifest.cell, &manifest.cell_sha256),
        ] {
            if hash.len() != 64 || bace_dat::fingerprint(path).map_err(|e| e.to_string())? != *hash
            {
                return Err("unapproved map DAT fingerprint".into());
            }
        }
        let mut portal = DatArchive::open(&manifest.portal).map_err(|e| e.to_string())?;
        let cell = DatArchive::open(&manifest.cell).map_err(|e| e.to_string())?;
        if portal.header().dataset != 1 || cell.header().dataset != 2 {
            return Err("incorrect map DAT datasets".into());
        }
        let land = RegionLand::decode_prefix(&portal.read(0x13000000).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Self { portal, cell, land })
    }
    /// Invoke only on bounded asset-preparation capacity, never the world thread.
    pub fn prepare(
        &mut self,
        request: MapTeleportRequest,
        expected_epoch: u16,
    ) -> Result<PreparedMapTeleport, String> {
        validate_request(request)?;
        let base = request.cell & 0xffff0000;
        let block = Landblock::decode(&self.cell.read(base | 0xffff).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let info = if block.has_objects {
            Some(
                LandblockInfo::decode(&self.cell.read(base | 0xfffe).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let mut cells = BTreeMap::new();
        let mut environments = BTreeMap::new();
        let mut pending = Vec::new();
        if let Some(info) = &info {
            for building in &info.buildings {
                pending.extend(
                    building
                        .portals
                        .iter()
                        .filter(|p| p.other_cell != 0xffff)
                        .map(|p| base | u32::from(p.other_cell)),
                );
            }
        }
        while let Some(id) = pending.pop() {
            if cells.contains_key(&id) {
                continue;
            }
            if cells.len() >= 4096 || pending.len() > 65536 {
                return Err("map building cell capacity".into());
            }
            let cell = EnvCell::decode(&self.cell.read(id).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if cell.id != id {
                return Err("map cell identity".into());
            }
            if let std::collections::btree_map::Entry::Vacant(entry) =
                environments.entry(cell.environment_id)
            {
                let environment = Environment::decode(
                    &self
                        .portal
                        .read(cell.environment_id)
                        .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                entry.insert(environment);
            }
            pending.extend(cell.visible_cells.iter().map(|id| base | u32::from(*id)));
            cells.insert(id, cell);
        }
        prepare_map_destination(
            request,
            expected_epoch,
            &block,
            &self.land,
            info.as_ref(),
            &cells,
            &environments,
        )
    }
}
fn validate_request(request: MapTeleportRequest) -> Result<(), String> {
    if !(1..=64).contains(&(request.cell & 0xffff))
        || request
            .origin
            .iter()
            .chain(&request.rotation)
            .any(|v| !v.is_finite())
        || request.origin[..2]
            .iter()
            .any(|v| !(0.0..192.0).contains(v))
        || !(0.999..=1.001).contains(&request.rotation.iter().map(|v| v * v).sum::<f32>())
    {
        return Err("invalid map click".into());
    }
    Ok(())
}
/// Pure adapter for already verified immutable assets. The archive owner must
/// supply every referenced cell/environment; the world separately gates collision.
pub fn prepare_map_destination(
    request: MapTeleportRequest,
    expected_epoch: u16,
    block: &Landblock,
    land: &RegionLand,
    info: Option<&LandblockInfo>,
    cells: &BTreeMap<u32, EnvCell>,
    environments: &BTreeMap<u32, Environment>,
) -> Result<PreparedMapTeleport, String> {
    validate_request(request)?;
    let base = request.cell & 0xffff0000;
    if block.id != base | 0xffff || cells.len() > 4096 || environments.len() > 4096 {
        return Err("map assets identity or capacity".into());
    }
    if block.has_objects && info.is_none() {
        return Err("missing map landblock info".into());
    }
    if info.is_some_and(|i| i.id != base | 0xfffe || i.buildings.len() > 4096) {
        return Err("invalid map landblock info".into());
    }
    // Every vertex participates in at least one of the 64 outdoor cells.
    let entirely_water = block
        .terrain
        .iter()
        .all(|t| (16..=20).contains(&((t >> 2) & 31)));
    let terrain = GeometryRegion::prepare(crate::region_geometry::terrain_cells(block, land)?)
        .map_err(|e| e.to_string())?;
    let outdoor = base | (request.origin[0] as u32 / 24 * 8 + request.origin[1] as u32 / 24 + 1);
    let mut position = Vec3::new(request.origin[0], request.origin[1], 0.0);
    position.z = terrain
        .ground_at(outdoor, position)
        .map_err(|e| e.to_string())?
        .0;
    let mut destination_cell = request.cell;
    // SortCell.add_building assigns the last building attached to that cell.
    let building = info.and_then(|i| {
        i.buildings.iter().rev().find(|b| {
            crate::region_geometry::normalize_outdoor(
                base | 1,
                Vec3::new(b.frame.origin[0], b.frame.origin[1], b.frame.origin[2]),
            )
            .is_ok_and(|(cell, _)| cell == request.cell)
        })
    });
    if let Some(building) = building {
        let mut ordered = Vec::new();
        let mut visited = BTreeSet::new();
        let mut pending: Vec<_> = building
            .portals
            .iter()
            .rev()
            .filter(|p| p.other_cell != 0xffff)
            .map(|p| base | u32::from(p.other_cell))
            .collect();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            if visited.len() > 4096 {
                return Err("map building closure capacity".into());
            }
            let cell = cells.get(&id).ok_or("missing map building cell")?;
            ordered.push(cell);
            pending.extend(
                cell.visible_cells
                    .iter()
                    .rev()
                    .map(|id| base | u32::from(*id)),
            );
        }
        let mut min_z = f32::MAX;
        let mut vertices = 0usize;
        for cell in &ordered {
            let environment = environments
                .get(&cell.environment_id)
                .filter(|e| e.id == cell.environment_id)
                .ok_or("missing map environment")?;
            // Source inspects every cell structure in each referenced environment,
            // and deliberately uses local vertex Z, without the building frame.
            for geometry in environment.cells.values() {
                vertices = vertices
                    .checked_add(geometry.vertices.len())
                    .ok_or("map vertex overflow")?;
                if vertices > 1_048_576 {
                    return Err("map vertex capacity".into());
                }
                for vertex in geometry.vertices.values() {
                    if !vertex.position[2].is_finite() {
                        return Err("invalid map vertex".into());
                    }
                    min_z = min_z.min(vertex.position[2]);
                }
            }
        }
        if min_z > 0.0 && min_z < f32::MAX {
            position.z += min_z;
        }
        destination_cell = outdoor;
        for cell in ordered {
            if point_in_cell(
                position,
                cell,
                environments
                    .get(&cell.environment_id)
                    .ok_or("missing map environment")?,
            )? {
                destination_cell = cell.id;
                break;
            }
        }
    }
    Ok(PreparedMapTeleport {
        requested_cell: request.cell,
        requested_xy: [request.origin[0], request.origin[1]],
        destination: StaffDestination {
            cell: destination_cell,
            origin: [position.x, position.y, position.z],
            rotation: request.rotation,
        },
        entirely_water,
        expected_epoch,
    })
}
fn point_in_cell(point: Vec3, cell: &EnvCell, environment: &Environment) -> Result<bool, String> {
    let q = cell.position.rotation;
    if cell
        .position
        .origin
        .iter()
        .chain(&q)
        .any(|v| !v.is_finite())
        || !(0.999..=1.001).contains(&q.iter().map(|v| v * v).sum::<f32>())
    {
        return Err("invalid map cell frame".into());
    }
    let p = point
        - Vec3::new(
            cell.position.origin[0],
            cell.position.origin[1],
            cell.position.origin[2],
        );
    let u = Vec3::new(-q[1], -q[2], -q[3]);
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    let p = p + cross(u, p) * (2.0 * q[0]) + cross(u, cross(u, p)) * 2.0;
    let tree = &environment
        .cells
        .get(&u32::from(cell.cell_structure))
        .ok_or("missing map cell structure")?
        .cell_bsp;
    let mut index = 0;
    for _ in 0..128 {
        let node = tree.nodes.get(index).ok_or("invalid map cell BSP")?;
        let Some(plane) = node.splitting_plane else {
            return Ok(true);
        };
        if plane.iter().any(|v| !v.is_finite()) {
            return Err("invalid map cell plane".into());
        }
        let distance = p.x * plane[0] + p.y * plane[1] + p.z * plane[2] + plane[3];
        if distance < -0.0002 {
            return Ok(false);
        }
        let Some(next) = node.positive else {
            return Ok(true);
        };
        index = next;
    }
    Err("map cell BSP depth".into())
}
