//! Cold conversion of verified DAT geometry into immutable physics inputs.
//! Archive fingerprints/iterations and content identity are checked by the
//! preparation owner before calling. No synthetic geometry fallback is supplied.
use bace_dat::{
    EnvCell, Environment, GraphicsObject, Landblock, ModelFrame, ModelPolygon, ModelVertex,
    RegionLand,
};
use bace_geometry::Vec3;
use bace_physics::{CellPortalGeometry, CollisionFace, CollisionPlane, GdlePolygon, GeometryCell};
use std::collections::BTreeMap;
fn vector(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], v[1], v[2])
}
fn rotate(v: Vec3, q: [f32; 4]) -> Vec3 {
    let u = Vec3::new(q[1], q[2], q[3]);
    let w = q[0];
    let cross = |a: Vec3, b: Vec3| {
        Vec3::new(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x,
        )
    };
    v + cross(u, v) * (2.0 * w) + cross(u, cross(u, v)) * 2.0
}
fn valid_frame(frame: &ModelFrame) -> Result<(), String> {
    let length = frame.rotation.iter().map(|v| v * v).sum::<f32>();
    if !vector(frame.origin).is_finite()
        || !length.is_finite()
        || !(0.999..=1.001).contains(&length)
    {
        return Err("invalid geometry frame".into());
    }
    Ok(())
}
fn polygon(
    p: &ModelPolygon,
    vertices: &BTreeMap<u16, ModelVertex>,
    frame: &ModelFrame,
) -> Result<GdlePolygon, String> {
    let points = p
        .vertices
        .iter()
        .map(|id| {
            vertices
                .get(id)
                .map(|v| rotate(vector(v.position), frame.rotation) + vector(frame.origin))
                .ok_or_else(|| "missing collision vertex".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    GdlePolygon::prepare(points).map_err(|e| e.to_string())
}
fn plane(p: [f32; 4], frame: &ModelFrame) -> CollisionPlane {
    let normal = rotate(Vec3::new(p[0], p[1], p[2]), frame.rotation);
    CollisionPlane {
        normal,
        distance: p[3] - normal.dot(vector(frame.origin)),
    }
}
fn prepare_bsp(
    tree: &bace_dat::BspTree,
    source: &BTreeMap<u16, ModelPolygon>,
    vertices: &BTreeMap<u16, ModelVertex>,
    frame: &ModelFrame,
) -> Result<bace_physics::StaticBsp, String> {
    use bace_physics::{BspCollisionNode, BspLimits, CollisionSphere, GdleBspCell, StaticBsp};
    let mut indices = BTreeMap::new();
    let mut polygons = Vec::new();
    for (id, p) in source {
        indices.insert(*id, polygons.len());
        polygons.push(polygon(p, vertices, frame)?);
    }
    let mut nodes = Vec::new();
    for node in &tree.nodes {
        let sphere = node.sphere.as_ref().ok_or("physics BSP missing bounds")?;
        let bounds = CollisionSphere {
            center: rotate(vector(sphere.origin), frame.rotation) + vector(frame.origin),
            radius: sphere.radius,
        };
        nodes.push(if node.leaf_index.is_some() {
            BspCollisionNode::Leaf {
                bounds,
                solid: node.solid.ok_or("physics BSP leaf flag")? != 0,
                polygons: node
                    .polygons
                    .iter()
                    .map(|id| {
                        indices
                            .get(id)
                            .copied()
                            .ok_or("physics BSP missing polygon")
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }
        } else {
            BspCollisionNode::Branch {
                bounds,
                plane: plane(node.splitting_plane.ok_or("physics BSP plane")?, frame),
                positive: node.positive.ok_or("physics BSP positive child")?,
                negative: node.negative.ok_or("physics BSP negative child")?,
            }
        });
    }
    Ok(StaticBsp {
        geometry: std::sync::Arc::new(
            GdleBspCell::prepare(nodes, polygons, BspLimits::default())
                .map_err(|e| e.to_string())?,
        ),
        object: None,
    })
}
/// Each static object's complete prepared collision is required; an absent
/// graphics record cannot silently turn a building or tree into empty space.
pub fn indoor_cell(
    cell: &EnvCell,
    environment: &Environment,
    statics: &BTreeMap<u32, GraphicsObject>,
) -> Result<GeometryCell, String> {
    indoor_cell_connected(cell, environment, statics, &mut [])
}
pub fn indoor_cell_connected(
    cell: &EnvCell,
    environment: &Environment,
    statics: &BTreeMap<u32, GraphicsObject>,
    outdoors: &mut [GeometryCell],
) -> Result<GeometryCell, String> {
    indoor_cell_with_assets(cell, environment, statics, &BTreeMap::new(), outdoors)
}
pub fn indoor_cell_with_assets(
    cell: &EnvCell,
    environment: &Environment,
    statics: &BTreeMap<u32, GraphicsObject>,
    setups: &BTreeMap<u32, bace_dat::CollisionSetup>,
    outdoors: &mut [GeometryCell],
) -> Result<GeometryCell, String> {
    if environment.id != cell.environment_id {
        return Err("environment identity mismatch".into());
    }
    valid_frame(&cell.position)?;
    let geometry = environment
        .cells
        .get(&u32::from(cell.cell_structure))
        .ok_or("missing environment cell structure")?;
    let mut boundary = Vec::new();
    let mut index = 0;
    for _ in 0..128 {
        let node = geometry
            .cell_bsp
            .nodes
            .get(index)
            .ok_or("invalid cell BSP index")?;
        if let Some(p) = node.splitting_plane {
            boundary.push(plane(p, &cell.position));
        }
        let Some(next) = node.positive else { break };
        index = next;
    }
    let mut solids = vec![prepare_bsp(
        &geometry.physics_bsp,
        &geometry.physics_polygons,
        &geometry.vertices,
        &cell.position,
    )?];
    let mut faces = geometry
        .physics_polygons
        .values()
        .map(|p| {
            Ok(CollisionFace {
                polygon: polygon(p, &geometry.vertices, &cell.position)?,
                two_sided: p.cull == 1 || p.cull == 2,
                object: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut static_primitives = Vec::new();
    for object in &cell.static_objects {
        let prepared = prepare_static_object(object.id, &object.frame, statics, setups)?;
        if faces.len() + prepared.faces.len() > 65536
            || solids.len() + prepared.solids.len() > 4096
            || static_primitives.len() + prepared.primitives.len() > 65536
        {
            return Err("static geometry capacity".into());
        }
        faces.extend(prepared.faces);
        solids.extend(prepared.solids);
        static_primitives.extend(prepared.primitives);
    }
    let mut portals = Vec::new();
    for portal in &cell.portals {
        let p = geometry
            .polygons
            .get(&portal.polygon_id)
            .ok_or("missing portal polygon")?;
        if portal.other_cell_id == 0xffff {
            let source = polygon(p, &geometry.vertices, &cell.position)?;
            let before = portals.len();
            for outdoor in outdoors
                .iter_mut()
                .filter(|c| c.id & 0xffff0000 == cell.id & 0xffff0000 && c.id & 0xffff <= 64)
            {
                let clipped = clip_portal(source.vertices(), &outdoor.boundary);
                if clipped.len() < 3 {
                    continue;
                }
                let Ok(polygon) = GdlePolygon::prepare(clipped) else {
                    continue;
                };
                portals.push(CellPortalGeometry {
                    destination: outdoor.id,
                    polygon: polygon.clone(),
                    translation: Vec3::ZERO,
                });
            }
            if portals.len() == before {
                return Err("outdoor portal needs admitted landscape linkage".into());
            }
            continue;
        }
        portals.push(CellPortalGeometry {
            destination: (cell.id & 0xffff0000) | u32::from(portal.other_cell_id),
            polygon: polygon(p, &geometry.vertices, &cell.position)?,
            translation: Vec3::ZERO,
        });
    }
    // Reverse edges publish only after complete cell preparation succeeds.
    if outdoors.iter().any(|out| {
        out.portals.len() + portals.iter().filter(|p| p.destination == out.id).count() > 256
    }) {
        return Err("outdoor portal capacity".into());
    }
    for portal in &portals {
        if let Some(outdoor) = outdoors.iter_mut().find(|c| c.id == portal.destination) {
            outdoor.portals.push(CellPortalGeometry {
                destination: cell.id,
                polygon: portal.polygon.clone(),
                translation: Vec3::ZERO,
            });
        }
    }
    Ok(GeometryCell {
        id: cell.id,
        terrain: false,
        restriction: cell.restriction_object.filter(|id| *id != 0),
        solids,
        static_primitives,
        boundary,
        faces,
        portals,
    })
}
/// Pinned LandblockStruct.ConstructPolygons split arithmetic, including u32
/// wrapping and the original double comparison. DAT height LUT is mandatory.
pub fn terrain_cells(block: &Landblock, land: &RegionLand) -> Result<Vec<GeometryCell>, String> {
    if land.square_length != 24.0 || land.vertices_per_cell != 1 || land.landblock_length != 8 {
        return Err("unsupported terrain lattice".into());
    }
    let base = block.id & 0xffff0000;
    let gx = (base >> 24) * 8;
    let gy = ((base >> 16) & 255) * 8;
    let mut cells = Vec::with_capacity(64);
    for x in 0..8u32 {
        for y in 0..8u32 {
            let make = |dx: u32, dy: u32| {
                Vec3::new(
                    (x + dx) as f32 * 24.0,
                    (y + dy) as f32 * 24.0,
                    land.heights[usize::from(block.heights[((x + dx) * 9 + y + dy) as usize])],
                )
            };
            let a = make(0, 0);
            let b = make(1, 0);
            let c = make(0, 1);
            let d = make(1, 1);
            let magic_a = (gx + x).wrapping_mul(214614067).wrapping_add(1813693831);
            let magic_b = (gx + x).wrapping_mul(1109124029);
            let split = (gy + y)
                .wrapping_mul(magic_a)
                .wrapping_sub(magic_b)
                .wrapping_sub(1369149221);
            let triangles = if f64::from(split) * 2.3283064e-10 < 0.5 {
                [vec![a, b, c], vec![d, c, b]]
            } else {
                [vec![a, b, d], vec![a, d, c]]
            };
            let faces = triangles
                .into_iter()
                .map(|v| {
                    GdlePolygon::prepare(v)
                        .map(|polygon| CollisionFace {
                            polygon,
                            two_sided: false,
                            object: None,
                        })
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let xmin = x as f32 * 24.0;
            let ymin = y as f32 * 24.0;
            let boundary = vec![
                CollisionPlane {
                    normal: Vec3::new(1.0, 0.0, 0.0),
                    distance: -xmin,
                },
                CollisionPlane {
                    normal: Vec3::new(-1.0, 0.0, 0.0),
                    distance: xmin + 24.0,
                },
                CollisionPlane {
                    normal: Vec3::new(0.0, 1.0, 0.0),
                    distance: -ymin,
                },
                CollisionPlane {
                    normal: Vec3::new(0.0, -1.0, 0.0),
                    distance: ymin + 24.0,
                },
            ];
            let id = base + x * 8 + y + 1;
            let mut portals = Vec::new();
            for (dx, dy, points) in [
                (
                    1,
                    0,
                    vec![
                        b,
                        d,
                        d + Vec3::new(0.0, 0.0, land.sky_height),
                        b + Vec3::new(0.0, 0.0, land.sky_height),
                    ],
                ),
                (
                    -1,
                    0,
                    vec![
                        c,
                        a,
                        a + Vec3::new(0.0, 0.0, land.sky_height),
                        c + Vec3::new(0.0, 0.0, land.sky_height),
                    ],
                ),
                (
                    0,
                    1,
                    vec![
                        d,
                        c,
                        c + Vec3::new(0.0, 0.0, land.sky_height),
                        d + Vec3::new(0.0, 0.0, land.sky_height),
                    ],
                ),
                (
                    0,
                    -1,
                    vec![
                        a,
                        b,
                        b + Vec3::new(0.0, 0.0, land.sky_height),
                        a + Vec3::new(0.0, 0.0, land.sky_height),
                    ],
                ),
            ] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                let global_x = gx as i32 + nx;
                let global_y = gy as i32 + ny;
                if !(0..2048).contains(&global_x) || !(0..2048).contains(&global_y) {
                    continue;
                }
                let block_x = (global_x as u32) >> 3;
                let block_y = (global_y as u32) >> 3;
                let destination = (block_x << 24)
                    | (block_y << 16)
                    | ((global_x as u32 & 7) * 8 + (global_y as u32 & 7) + 1);
                let translation = Vec3::new(
                    ((base >> 24) as i32 - block_x as i32) as f32 * 192.0,
                    (((base >> 16) & 255) as i32 - block_y as i32) as f32 * 192.0,
                    0.0,
                );
                portals.push(CellPortalGeometry {
                    destination,
                    polygon: GdlePolygon::prepare(points).map_err(|e| e.to_string())?,
                    translation,
                });
            }
            cells.push(GeometryCell {
                id,
                terrain: true,
                restriction: None,
                solids: vec![],
                static_primitives: vec![],
                boundary,
                faces,
                portals,
            });
        }
    }
    Ok(cells)
}

fn clip_portal(vertices: &[Vec3], planes: &[CollisionPlane]) -> Vec<Vec3> {
    let mut polygon = vertices.to_vec();
    for plane in planes {
        if polygon.is_empty() {
            break;
        }
        let mut output = Vec::with_capacity(polygon.len() + 1);
        let mut previous = *polygon.last().expect("nonempty polygon");
        let mut previous_distance = plane.normal.dot(previous) + plane.distance;
        for current in polygon {
            let distance = plane.normal.dot(current) + plane.distance;
            if (distance >= 0.0) != (previous_distance >= 0.0) {
                output.push(
                    previous
                        + (current - previous)
                            * (previous_distance / (previous_distance - distance)),
                );
            }
            if distance >= 0.0 {
                output.push(current);
            }
            previous = current;
            previous_distance = distance;
        }
        polygon = output;
    }
    polygon
}

pub struct PreparedStaticGeometry {
    pub faces: Vec<CollisionFace>,
    pub solids: Vec<bace_physics::StaticBsp>,
    pub primitives: Vec<bace_physics::DynamicSphere>,
}
/// Complete terrain/static portion of an outdoor block. Geometry from each DAT
/// object/building is distributed conservatively across every overlapping cell;
/// immutable BSP nodes are shared instead of copied for each cell.
pub fn outdoor_block(
    block: &Landblock,
    info: Option<&bace_dat::LandblockInfo>,
    land: &RegionLand,
    graphics: &BTreeMap<u32, GraphicsObject>,
    setups: &BTreeMap<u32, bace_dat::CollisionSetup>,
) -> Result<Vec<GeometryCell>, String> {
    let mut cells = terrain_cells(block, land)?;
    let Some(info) = info else {
        return if block.has_objects {
            Err("missing landblock info".into())
        } else {
            Ok(cells)
        };
    };
    if info.id & 0xffff0000 != block.id & 0xffff0000 {
        return Err("landblock info identity mismatch".into());
    }
    for (cell, restriction) in &info.restrictions {
        let target = cells
            .iter_mut()
            .find(|c| c.id == *cell)
            .ok_or("restriction references a different landscape cell")?;
        target.restriction = (*restriction != 0).then_some(*restriction);
    }
    let objects = info
        .objects
        .iter()
        .map(|o| (o.id, &o.frame))
        .chain(info.buildings.iter().map(|b| (b.model, &b.frame)));
    let mut total_faces = 128usize;
    for (id, frame) in objects {
        let object = prepare_static_object(id, frame, graphics, setups)?;
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        let mut bound = |v: Vec3| {
            min = Vec3::new(min.x.min(v.x), min.y.min(v.y), min.z.min(v.z));
            max = Vec3::new(max.x.max(v.x), max.y.max(v.y), max.z.max(v.z));
        };
        for face in &object.faces {
            for point in face.polygon.vertices() {
                bound(*point);
            }
        }
        for primitive in &object.primitives {
            let radius = primitive.sphere.radius;
            bound(primitive.sphere.center - Vec3::new(radius, radius, radius));
            bound(
                primitive.sphere.center
                    + Vec3::new(radius, radius, primitive.cylinder_height.unwrap_or(radius)),
            );
        }
        if object.faces.is_empty() && object.primitives.is_empty() {
            continue;
        }
        let mut assigned = false;
        for cell in &mut cells {
            let index = (cell.id & 0xffff) - 1;
            let x = (index / 8) as f32 * 24.0;
            let y = (index % 8) as f32 * 24.0;
            if max.x < x || max.y < y || min.x > x + 24.0 || min.y > y + 24.0 {
                continue;
            }
            assigned = true;
            for face in &object.faces {
                let points = face.polygon.vertices();
                if points.iter().all(|p| p.x < x)
                    || points.iter().all(|p| p.x > x + 24.0)
                    || points.iter().all(|p| p.y < y)
                    || points.iter().all(|p| p.y > y + 24.0)
                {
                    continue;
                }
                total_faces = total_faces
                    .checked_add(1)
                    .ok_or("outdoor collision capacity")?;
                if total_faces > 262144 || cell.faces.len() >= 65536 {
                    return Err("outdoor collision capacity".into());
                }
                cell.faces.push(face.clone());
            }
            if cell.solids.len() + object.solids.len() > 4096
                || cell.static_primitives.len() + object.primitives.len() > 65536
            {
                return Err("outdoor collision capacity".into());
            }
            cell.solids.extend(object.solids.iter().cloned());
            cell.static_primitives
                .extend(object.primitives.iter().copied());
        }
        if !assigned {
            return Err(
                "static object outside source block requires neighboring block preparation".into(),
            );
        }
    }
    Ok(cells)
}
/// Static Setup selection follows PartArray.CreateSetup/SetPlacementFrame:
/// placement 0x65, then placement0; a single unplaced part uses its object frame.
pub fn prepare_static_object(
    id: u32,
    frame: &ModelFrame,
    graphics: &BTreeMap<u32, GraphicsObject>,
    setups: &BTreeMap<u32, bace_dat::CollisionSetup>,
) -> Result<PreparedStaticGeometry, String> {
    valid_frame(frame)?;
    let mut result = PreparedStaticGeometry {
        faces: Vec::new(),
        solids: Vec::new(),
        primitives: Vec::new(),
    };
    let mut parts = Vec::new();
    let setup = if id >> 24 == 2 {
        Some(
            setups
                .get(&id)
                .filter(|s| s.id == id)
                .ok_or_else(|| format!("missing static setup {id:08x}"))?,
        )
    } else {
        None
    };
    if let Some(setup) = setup {
        if setup.parts.len() > 256 {
            return Err("static setup part limit".into());
        }
        let placement = setup
            .placements
            .get(&0x65)
            .or_else(|| setup.placements.get(&0));
        for (index, id) in setup.parts.iter().enumerate() {
            let part_frame = if let Some(placement) = placement {
                combine_frame(
                    frame,
                    placement
                        .parts
                        .get(index)
                        .ok_or("missing static part placement")?,
                )?
            } else if setup.parts.len() == 1 {
                frame.clone()
            } else {
                return Err("missing multi-part static placement".into());
            };
            let scale = setup
                .default_scales
                .as_ref()
                .map(|v| v.get(index).copied().ok_or("missing static part scale"))
                .transpose()?
                .unwrap_or([1.0; 3]);
            parts.push((*id, part_frame, scale));
        }
    } else if id >> 24 == 1 {
        parts.push((id, frame.clone(), [1.0; 3]));
    } else {
        return Err("unsupported static collision asset kind".into());
    }
    let has_bsp = parts.iter().try_fold(false, |found, (id, _, _)| {
        graphics
            .get(id)
            .filter(|g| g.id == *id)
            .map(|g| found || g.physics_bsp.is_some())
            .ok_or_else(|| format!("missing static graphics {id:08x}"))
    })?;
    if has_bsp {
        for (id, part_frame, scale) in parts {
            let model = graphics
                .get(&id)
                .ok_or("missing prepared static graphics")?;
            let Some(tree) = &model.physics_bsp else {
                continue;
            };
            if result.faces.len() + model.physics_polygons.len() > 65536 {
                return Err("static polygon capacity".into());
            }
            let (vertices, tree) = scaled_geometry(&model.vertices, tree, scale)?;
            result.solids.push(prepare_bsp(
                &tree,
                &model.physics_polygons,
                &vertices,
                &part_frame,
            )?);
            for p in model.physics_polygons.values() {
                result.faces.push(CollisionFace {
                    polygon: polygon(p, &vertices, &part_frame)?,
                    two_sided: p.cull == 1 || p.cull == 2,
                    object: None,
                });
            }
        }
    } else if let Some(setup) = setup {
        let shape = crate::world_admission::prepare_collision_shape(setup, 1.0)?;
        // Dummy moving spheres are a placement aid, not fabricated target geometry.
        if !setup.spheres.is_empty() || !setup.cylinders.is_empty() {
            for (sphere, cylinder_height) in shape.obstacles() {
                if cylinder_height.is_some()
                    && rotate(Vec3::new(0.0, 0.0, 1.0), frame.rotation).z < 0.999
                {
                    return Err(
                        "tilted static cylinder requires full oriented-cylinder solver".into(),
                    );
                }
                result.primitives.push(bace_physics::DynamicSphere {
                    object: 0,
                    cell: 0,
                    sphere: bace_physics::CollisionSphere {
                        center: rotate(sphere.center, frame.rotation) + vector(frame.origin),
                        radius: sphere.radius,
                    },
                    cylinder_height,
                    player_status: None,
                });
            }
        }
    }
    Ok(result)
}
fn combine_frame(parent: &ModelFrame, local: &ModelFrame) -> Result<ModelFrame, String> {
    valid_frame(parent)?;
    valid_frame(local)?;
    let a = parent.rotation;
    let b = local.rotation;
    let rotation = [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ];
    let origin = vector(parent.origin) + rotate(vector(local.origin), parent.rotation);
    Ok(ModelFrame {
        origin: [origin.x, origin.y, origin.z],
        rotation,
    })
}
fn scaled_geometry(
    vertices: &BTreeMap<u16, ModelVertex>,
    tree: &bace_dat::BspTree,
    scale: [f32; 3],
) -> Result<(BTreeMap<u16, ModelVertex>, bace_dat::BspTree), String> {
    if scale
        .iter()
        .any(|v| !v.is_finite() || !(0.001..=100.0).contains(v))
    {
        return Err("invalid static part scale".into());
    }
    let mut vertices = vertices.clone();
    for vertex in vertices.values_mut() {
        for (v, s) in vertex.position.iter_mut().zip(scale) {
            *v *= s;
        }
    }
    let mut tree = tree.clone();
    for node in &mut tree.nodes {
        if let Some(sphere) = &mut node.sphere {
            for (v, s) in sphere.origin.iter_mut().zip(scale) {
                *v *= s;
            }
            sphere.radius *= scale.into_iter().fold(0.0, f32::max);
        }
        if let Some(p) = &mut node.splitting_plane {
            for (v, s) in p[..3].iter_mut().zip(scale) {
                *v /= s;
            }
            let length = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if length == 0.0 || !length.is_finite() {
                return Err("invalid scaled BSP plane".into());
            }
            for v in p {
                *v /= length;
            }
        }
    }
    Ok((vertices, tree))
}

/// GDLE LandDefs::adjust_to_outside for the admitted 192-unit/8-cell landscape.
/// Frame translation is retained across block boundaries; invalid world edges fail.
pub fn normalize_outdoor(cell: u32, mut position: Vec3) -> Result<(u32, Vec3), String> {
    let local = cell & 0xffff;
    if !((1..=64).contains(&local) || (0x100..=0xfffd).contains(&local) || local == 0xffff)
        || !position.is_finite()
        || position.x.abs() > 1_000_000.0
        || position.y.abs() > 1_000_000.0
    {
        return Err("invalid outdoor position".into());
    }
    if position.x.abs() < 0.0002 {
        position.x = 0.0;
    }
    if position.y.abs() < 0.0002 {
        position.y = 0.0;
    }
    let x = i64::from(cell >> 24) * 8 + (position.x / 24.0).floor() as i64;
    let y = i64::from((cell >> 16) & 255) * 8 + (position.y / 24.0).floor() as i64;
    if !(0..2040).contains(&x) || !(0..2040).contains(&y) {
        return Err("outdoor world boundary".into());
    }
    let id = (((x as u32 >> 3) << 8 | (y as u32 >> 3)) << 16)
        | (1 + ((x as u32 & 7) << 3) + (y as u32 & 7));
    position.x -= (position.x / 192.0).floor() * 192.0;
    position.y -= (position.y / 192.0).floor() * 192.0;
    Ok((id, position))
}

/// Building attachment follows LandBlock::init_buildings and
/// BuildingObj::add_to_cell. A large mesh does not mark every overlapped cell.
pub fn building_cells(info: &bace_dat::LandblockInfo) -> Result<Vec<u32>, String> {
    let mut cells = std::collections::BTreeSet::new();
    for building in &info.buildings {
        valid_frame(&building.frame)?;
        let (cell, _) = normalize_outdoor(
            (info.id & 0xffff0000) | 0xffff,
            vector(building.frame.origin),
        )?;
        cells.insert(cell);
    }
    Ok(cells.into_iter().collect())
}

/// ACE Landblock.SpawnEncounters: clamped 24-unit coordinates, actual terrain Z,
/// and the source building-cell filter before collision admission.
pub fn encounter_position(
    region: &bace_physics::GeometryRegion,
    landblock: u16,
    x: i32,
    y: i32,
) -> Result<(u32, Vec3), String> {
    let p = Vec3::new(
        (x as f32 * 24.0).clamp(0.5, 191.5),
        (y as f32 * 24.0).clamp(0.5, 191.5),
        0.0,
    );
    let (cell, mut p) = normalize_outdoor((u32::from(landblock) << 16) | 1, p)?;
    if region.has_building(cell).map_err(|e| e.to_string())? {
        return Err("encounter cell contains a building".into());
    }
    p.z = region.ground_at(cell, p).map_err(|e| e.to_string())?.0;
    Ok((cell, p))
}
