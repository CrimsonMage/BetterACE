//! Immutable cell collision data and bounded continuous movement queries.
//! Prepared from authored DAT polygons, never inferred from visual bounds.
use crate::{BspQueryError, CollisionSphere, GdlePolygon, SweepContact};
/// ACE PhysicsObj.FindObjCollisions player exemption, with Free interpreted as
/// impenetrable by WeenieObject.IsImpenetrable. Nonplayers remain physical.
pub fn players_collide(first: Option<u32>, second: Option<u32>) -> bool {
    match (first, second) {
        (Some(a), Some(b)) => a == 0x20 || b == 0x20 || a == 4 && b == 4 || a == 0x40 && b == 0x40,
        _ => true,
    }
}
use bace_geometry::Vec3;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct CollisionFace {
    pub polygon: GdlePolygon,
    pub two_sided: bool,
    pub object: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct CellPortalGeometry {
    pub destination: u32,
    pub polygon: GdlePolygon,
    pub translation: Vec3,
}
#[derive(Clone, Debug)]
pub struct StaticBsp {
    pub geometry: std::sync::Arc<crate::GdleBspCell>,
    pub object: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct GeometryCell {
    pub id: u32,
    pub terrain: bool,
    pub restriction: Option<u32>,
    pub solids: Vec<StaticBsp>,
    pub static_primitives: Vec<DynamicSphere>,
    pub boundary: Vec<crate::CollisionPlane>,
    pub faces: Vec<CollisionFace>,
    pub portals: Vec<CellPortalGeometry>,
}
#[derive(Clone, Copy, Debug)]
pub struct CollisionCylinder {
    pub base: Vec3,
    pub radius: f32,
    pub height: f32,
}
#[derive(Clone, Debug)]
pub struct CollisionShape {
    spheres: Vec<CollisionSphere>,
    cylinders: Vec<CollisionCylinder>,
    nominal_dimensions: Option<(f32, f32)>,
    pub step_up: f32,
    pub step_down: f32,
}
impl CollisionShape {
    pub fn prepare(
        spheres: Vec<CollisionSphere>,
        step_up: f32,
        step_down: f32,
    ) -> Result<Self, BspQueryError> {
        if spheres.is_empty()
            || spheres.len() > 16
            || spheres.iter().any(|s| {
                !s.center.is_finite()
                    || !s.radius.is_finite()
                    || !(0.0002..=100.0).contains(&s.radius)
            })
            || !step_up.is_finite()
            || !step_down.is_finite()
            || !(0.0..=10.0).contains(&step_up)
            || !(0.0..=10.0).contains(&step_down)
        {
            return Err(BspQueryError::InvalidGeometry);
        }
        Ok(Self {
            spheres,
            cylinders: Vec::new(),
            nominal_dimensions: None,
            step_up,
            step_down,
        })
    }
    /// Authored CSetup radius/height scaled by the admitted object scale. These
    /// are separate from sphere/cylinder extents used by collision queries.
    pub fn with_nominal_dimensions(
        mut self,
        radius: f32,
        height: f32,
    ) -> Result<Self, BspQueryError> {
        if !radius.is_finite()
            || !height.is_finite()
            || !(0.0..=10000.0).contains(&radius)
            || !(0.0..=10000.0).contains(&height)
        {
            return Err(BspQueryError::InvalidGeometry);
        }
        self.nominal_dimensions = Some((radius, height));
        Ok(self)
    }
    pub fn nominal_radius(&self) -> Option<f32> {
        self.nominal_dimensions.map(|v| v.0)
    }
    pub fn nominal_height(&self) -> Option<f32> {
        self.nominal_dimensions.map(|v| v.1)
    }
    pub fn with_cylinders(
        mut self,
        cylinders: Vec<CollisionCylinder>,
    ) -> Result<Self, BspQueryError> {
        if cylinders.len() > 16
            || cylinders.iter().any(|c| {
                !c.base.is_finite()
                    || !c.radius.is_finite()
                    || !c.height.is_finite()
                    || !(0.0002..=100.0).contains(&c.radius)
                    || !(0.0..=100.0).contains(&c.height)
            })
        {
            return Err(BspQueryError::InvalidGeometry);
        }
        self.cylinders = cylinders;
        Ok(self)
    }
    pub fn cylinders(&self) -> &[CollisionCylinder] {
        &self.cylinders
    }
    pub fn obstacles(&self) -> impl Iterator<Item = (CollisionSphere, Option<f32>)> + '_ {
        self.cylinders
            .iter()
            .map(|c| {
                (
                    CollisionSphere {
                        center: c.base,
                        radius: c.radius,
                    },
                    Some(c.height),
                )
            })
            .chain(
                self.spheres
                    .iter()
                    .filter(|_| self.cylinders.is_empty())
                    .map(|s| (*s, None)),
            )
    }
    pub fn horizontal_radius(&self) -> f32 {
        self.obstacles()
            .map(|(s, _)| s.center.x.hypot(s.center.y) + s.radius)
            .fold(0.0, f32::max)
    }
    pub fn height(&self) -> f32 {
        self.obstacles()
            .map(|(s, h)| s.center.z + h.unwrap_or(s.radius))
            .fold(0.0, f32::max)
    }
    pub fn spheres(&self) -> &[CollisionSphere] {
        &self.spheres
    }
}
#[derive(Clone, Copy, Debug)]
pub struct DynamicSphere {
    pub object: u32,
    pub cell: u32,
    pub sphere: CollisionSphere,
    pub cylinder_height: Option<f32>,
    pub player_status: Option<u32>,
}
impl DynamicSphere {
    pub fn overlaps(&self, sphere: CollisionSphere) -> bool {
        let offset = sphere.center - self.sphere.center;
        let reach = sphere.radius + self.sphere.radius - 0.001;
        if let Some(height) = self.cylinder_height {
            offset.x * offset.x + offset.y * offset.y < reach * reach
                && offset.z > -sphere.radius
                && offset.z < height + sphere.radius
        } else {
            offset.length_squared() < reach * reach
        }
    }
    pub fn cast(&self, from: Vec3, delta: Vec3, radius: f32) -> Option<SweepContact> {
        if let Some(height) = self.cylinder_height {
            crate::sweep::sweep_cylinder(
                from,
                delta,
                radius,
                self.sphere.center,
                self.sphere.radius,
                height,
            )
        } else {
            crate::sweep_spheres(from, delta, radius, self.sphere.center, self.sphere.radius)
        }
    }
}
#[derive(Clone, Debug)]
pub struct GeometryRegion {
    landblock_metric: Option<(f32, i32)>,
    building_cells: BTreeSet<u32>,
    cells: std::sync::Arc<BTreeMap<u32, GeometryCell>>,
    disabled: BTreeSet<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryMove {
    pub cell: u32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub grounded: bool,
    pub contacted_object: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeometryError {
    #[error("missing admitted cell geometry")]
    MissingCell,
    #[error("cell entry restriction {0} has no admitted grant")]
    Restricted(u32),
    #[error("invalid collision geometry or state")]
    Invalid,
    #[error("collision query work budget exceeded")]
    Budget,
}
#[derive(Clone, Copy, Debug)]
pub struct GeometryTrace {
    pub destination: u32,
    pub translation: Vec3,
    pub contact: Option<SweepContact>,
}
pub struct GeometryStep<'a> {
    pub cell: u32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub seconds: f32,
    pub shape: &'a CollisionShape,
    pub dynamics: &'a [DynamicSphere],
    pub ignore: u32,
    pub was_grounded: bool,
    pub player_status: Option<u32>,
    pub allowed_restrictions: &'a [u32],
}
struct Budget(usize);
impl Budget {
    fn spend(&mut self) -> Result<(), GeometryError> {
        self.0 = self.0.checked_sub(1).ok_or(GeometryError::Budget)?;
        Ok(())
    }
}
struct Hit {
    contact: SweepContact,
    object: Option<u32>,
}
impl GeometryRegion {
    pub fn prepare(cells: Vec<GeometryCell>) -> Result<Self, GeometryError> {
        if cells.is_empty() || cells.len() > 1024 {
            return Err(GeometryError::Budget);
        }
        let mut values = BTreeMap::new();
        let mut total = 0usize;
        for cell in cells {
            total = total
                .checked_add(cell.faces.len() + cell.portals.len())
                .ok_or(GeometryError::Budget)?;
            if total > 262144
                || cell.id == 0
                || cell.boundary.is_empty()
                || cell.boundary.len() > 128
                || cell.boundary.iter().any(|p| p.validate().is_err())
                || cell.static_primitives.len() > 65536
                || cell.solids.len() > 4096
                || cell.faces.len() > 65536
                || cell.portals.len() > 256
                || cell
                    .portals
                    .iter()
                    .any(|p| p.destination == 0 || !p.translation.is_finite())
                || values.contains_key(&cell.id)
            {
                return Err(GeometryError::Invalid);
            }
            values.insert(cell.id, cell);
        }
        Ok(Self {
            landblock_metric: None,
            cells: std::sync::Arc::new(values),
            disabled: BTreeSet::new(),
            building_cells: BTreeSet::new(),
        })
    }
    pub fn with_landblock_metric(
        mut self,
        square_length: f32,
        side: i32,
    ) -> Result<Self, GeometryError> {
        if !square_length.is_finite()
            || !(0.1..=1000.0).contains(&square_length)
            || ![2, 4, 8, 16].contains(&side)
        {
            return Err(GeometryError::Invalid);
        }
        self.landblock_metric = Some((square_length, side));
        Ok(self)
    }
    /// GDLE Position::get_offset / LandDefs::get_block_offset. Accepted poses
    /// use global axes inside each landblock; cross-cell velocity needs no rotation.
    pub fn frame_offset(&self, from: u32, to: u32) -> Result<Vec3, GeometryError> {
        if !self.cells.contains_key(&from) || !self.cells.contains_key(&to) {
            return Err(GeometryError::MissingCell);
        }
        if from >> 16 == to >> 16 {
            return Ok(Vec3::ZERO);
        }
        let (square, side) = self.landblock_metric.ok_or(GeometryError::MissingCell)?;
        let x = ((to >> 24) as i32 - (from >> 24) as i32) * side;
        let y = (((to >> 16) & 255) as i32 - ((from >> 16) & 255) as i32) * side;
        Ok(Vec3::new(x as f32 * square, y as f32 * square, 0.0))
    }
    /// Source building attachment is to the outdoor cell containing its frame
    /// origin (GDLE LandBlock::init_buildings), not every intersecting mesh cell.
    pub fn with_building_cells(mut self, cells: &[u32]) -> Result<Self, GeometryError> {
        if cells.len() > 4096 {
            return Err(GeometryError::Budget);
        }
        for &cell in cells {
            if !self.cells.get(&cell).is_some_and(|c| c.terrain) {
                return Err(GeometryError::MissingCell);
            }
            self.building_cells.insert(cell);
        }
        Ok(self)
    }
    /// Source indoor scatter scans admitted environmental cells in identity order.
    pub fn indoor_cell_at(&self, landblock: u32, position: Vec3) -> Option<u32> {
        if !position.is_finite() {
            return None;
        }
        self.cells
            .values()
            .find(|cell| {
                !cell.terrain
                    && cell.id >> 16 == landblock
                    && cell
                        .boundary
                        .iter()
                        .all(|p| p.normal.dot(position) + p.distance >= -0.0002)
            })
            .map(|cell| cell.id)
    }
    pub fn has_building(&self, cell: u32) -> Result<bool, GeometryError> {
        if !self.cells.contains_key(&cell) {
            return Err(GeometryError::MissingCell);
        }
        Ok(self.building_cells.contains(&cell))
    }
    /// Exact plane height and authored upward normal of an outdoor terrain triangle.
    pub fn ground_at(&self, cell: u32, position: Vec3) -> Result<(f32, Vec3), GeometryError> {
        if !position.is_finite() {
            return Err(GeometryError::Invalid);
        }
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        if !geometry.terrain {
            return Err(GeometryError::Invalid);
        }
        for face in geometry.faces.iter().take(2) {
            let plane = face.polygon.plane();
            if plane.normal.z <= 0.0 {
                return Err(GeometryError::Invalid);
            }
            let z = -(plane.normal.x * position.x + plane.normal.y * position.y + plane.distance)
                / plane.normal.z;
            if face
                .polygon
                .contains_projection(Vec3::new(position.x, position.y, z), 0.001)
            {
                return Ok((z, plane.normal));
            }
        }
        Err(GeometryError::MissingCell)
    }
    /// Replace one cold-prepared landblock while retaining other active cells.
    /// The world validates all current actors before installing the candidate.
    pub fn replace_landblock(
        &self,
        landblock: u16,
        replacement: &Self,
    ) -> Result<Self, GeometryError> {
        let base = u32::from(landblock) << 16;
        if replacement.cells.keys().any(|id| id & 0xffff0000 != base) {
            return Err(GeometryError::Invalid);
        }
        let mut cells: Vec<_> = self
            .cells
            .values()
            .filter(|c| c.id & 0xffff0000 != base)
            .cloned()
            .collect();
        cells.extend(replacement.cells.values().cloned());
        let mut result = Self::prepare(cells)?;
        result.building_cells.extend(
            self.building_cells
                .iter()
                .filter(|id| **id & 0xffff0000 != base)
                .copied(),
        );
        result
            .building_cells
            .extend(replacement.building_cells.iter().copied());
        result.disabled = self.disabled.clone();
        Ok(result)
    }
    pub fn validate_access(&self, cell: u32, allowed: &[u32]) -> Result<(), GeometryError> {
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        if allowed.len() > 64 {
            return Err(GeometryError::Budget);
        }
        if let Some(id) = geometry.restriction
            && !allowed.contains(&id)
        {
            return Err(GeometryError::Restricted(id));
        }
        Ok(())
    }
    pub fn cell(&self, id: u32) -> Option<&GeometryCell> {
        self.cells.get(&id)
    }
    pub fn set_object_solid(&mut self, object: u32, solid: bool) -> Result<(), GeometryError> {
        if object == 0
            || !self.cells.values().any(|c| {
                c.faces.iter().any(|f| f.object == Some(object))
                    || c.static_primitives.iter().any(|p| p.object == object)
            })
        {
            return Err(GeometryError::Invalid);
        }
        if solid {
            self.disabled.remove(&object);
        } else {
            if self.disabled.len() >= 4096 && !self.disabled.contains(&object) {
                return Err(GeometryError::Budget);
            }
            self.disabled.insert(object);
        }
        Ok(())
    }
    pub fn trace_sphere(
        &self,
        cell: u32,
        from: Vec3,
        to: Vec3,
        radius: f32,
    ) -> Result<GeometryTrace, GeometryError> {
        if !from.is_finite()
            || !to.is_finite()
            || !radius.is_finite()
            || !(0.0002..=100.0).contains(&radius)
        {
            return Err(GeometryError::Invalid);
        }
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        let contact = self
            .sweep(
                cell,
                from,
                to - from,
                &[CollisionSphere {
                    center: Vec3::ZERO,
                    radius,
                }],
                &[],
                0,
                &mut Budget(262144),
                None,
            )?
            .map(|h| h.contact);
        let end = contact.map_or(to, |h| from + (to - from) * h.fraction);

        for portal in &geometry.portals {
            let plane = portal.polygon.plane();
            let a = plane.normal.dot(from) + plane.distance;
            let b = plane.normal.dot(end) + plane.distance;
            if a == b {
                continue;
            }
            let t = a / (a - b);
            if !(0.0..=1.0).contains(&t)
                || !portal
                    .polygon
                    .contains_projection(from + (end - from) * t, 0.0002)
            {
                continue;
            }
            let next = self
                .cells
                .get(&portal.destination)
                .ok_or(GeometryError::MissingCell)?;
            if next
                .boundary
                .iter()
                .all(|p| p.normal.dot(end + portal.translation) + p.distance >= -0.0002)
            {
                return Ok(GeometryTrace {
                    destination: portal.destination,
                    translation: portal.translation,
                    contact,
                });
            }
        }
        if geometry
            .boundary
            .iter()
            .all(|p| p.normal.dot(end) + p.distance >= -0.0002)
        {
            return Ok(GeometryTrace {
                destination: cell,
                translation: Vec3::ZERO,
                contact,
            });
        }
        Err(GeometryError::MissingCell)
    }
    pub fn segment_clear(
        &self,
        cell: u32,
        from: Vec3,
        to: Vec3,
        ignore: Option<u32>,
    ) -> Result<bool, GeometryError> {
        if !from.is_finite() || !to.is_finite() {
            return Err(GeometryError::Invalid);
        }
        let spheres = [CollisionSphere {
            center: Vec3::ZERO,
            radius: 0.00021,
        }];
        Ok(self
            .sweep(
                cell,
                from,
                to - from,
                &spheres,
                &[],
                ignore.unwrap_or(0),
                &mut Budget(262144),
                None,
            )?
            .is_none())
    }

    pub fn validate_placement(
        &self,
        cell: u32,
        position: Vec3,
        shape: &CollisionShape,
        dynamics: &[DynamicSphere],
        ignore: u32,
    ) -> Result<(), GeometryError> {
        let status = dynamics
            .iter()
            .find(|d| d.object == ignore)
            .and_then(|d| d.player_status);
        self.validate_placement_for(cell, position, shape, dynamics, ignore, status)
    }
    #[allow(clippy::too_many_arguments)]
    fn validate_placement_for(
        &self,
        cell: u32,
        position: Vec3,
        shape: &CollisionShape,
        dynamics: &[DynamicSphere],
        ignore: u32,
        player_status: Option<u32>,
    ) -> Result<(), GeometryError> {
        if !position.is_finite() || dynamics.len() > 65536 {
            return Err(GeometryError::Invalid);
        }
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        self.validate_cell_center(cell, position, shape)?;
        if geometry.terrain {
            let mut floor = None;
            for face in geometry.faces.iter().take(2) {
                let p = face.polygon.plane();
                if p.normal.z <= 0.0 {
                    return Err(GeometryError::Invalid);
                }
                let z =
                    -(p.normal.x * position.x + p.normal.y * position.y + p.distance) / p.normal.z;
                if face
                    .polygon
                    .contains_projection(Vec3::new(position.x, position.y, z), 0.001)
                {
                    floor = Some(z);
                    break;
                }
            }
            if floor.is_none_or(|z| position.z < z - 0.001) {
                return Err(GeometryError::Invalid);
            }
        }
        let mut budget = Budget(262144);
        let mut bsp_budget = crate::BspQueryBudget::default();
        for sphere in &shape.spheres {
            let sphere = CollisionSphere {
                center: position + sphere.center,
                radius: sphere.radius,
            };
            for solid in &geometry.solids {
                if solid.object.is_some_and(|id| self.disabled.contains(&id)) {
                    continue;
                }
                let query = CollisionSphere {
                    radius: (sphere.radius - 0.001).max(0.00021),
                    ..sphere
                };
                if solid
                    .geometry
                    .intersects_solid(query, true, &mut bsp_budget)
                    .map_err(|_| GeometryError::Budget)?
                {
                    return Err(GeometryError::Invalid);
                }
            }
            for other in &geometry.static_primitives {
                budget.spend()?;
                if other.object != 0 && self.disabled.contains(&other.object) {
                    continue;
                }
                if other.overlaps(sphere) {
                    return Err(GeometryError::Invalid);
                }
            }
            for face in &geometry.faces {
                budget.spend()?;
                if face.object.is_some_and(|id| self.disabled.contains(&id)) {
                    continue;
                }
                if face
                    .polygon
                    .contact(CollisionSphere {
                        radius: (sphere.radius - 0.001).max(0.00021),
                        ..sphere
                    })
                    .is_some()
                {
                    return Err(GeometryError::Invalid);
                }
            }
            for other in dynamics.iter().filter(|d| {
                d.cell == cell
                    && d.object != ignore
                    && players_collide(player_status, d.player_status)
            }) {
                budget.spend()?;
                if other.overlaps(sphere) {
                    return Err(GeometryError::Invalid);
                }
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn sweep(
        &self,
        cell: u32,
        position: Vec3,
        displacement: Vec3,
        spheres: &[CollisionSphere],
        dynamics: &[DynamicSphere],
        ignore: u32,
        budget: &mut Budget,
        player_status: Option<u32>,
    ) -> Result<Option<Hit>, GeometryError> {
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        let mut hit: Option<Hit> = None;
        for sphere in spheres {
            for (neighbor, offset) in std::iter::once((geometry, Vec3::ZERO)).chain(
                geometry
                    .portals
                    .iter()
                    .filter_map(|p| self.cells.get(&p.destination).map(|c| (c, p.translation))),
            ) {
                for other in &neighbor.static_primitives {
                    budget.spend()?;
                    if other.object != 0
                        && (other.object == ignore || self.disabled.contains(&other.object))
                    {
                        continue;
                    }
                    if let Some(contact) = other.cast(
                        position + sphere.center + offset,
                        displacement,
                        sphere.radius,
                    ) && hit
                        .as_ref()
                        .is_none_or(|h| contact.fraction < h.contact.fraction)
                    {
                        hit = Some(Hit {
                            contact,
                            object: (other.object != 0).then_some(other.object),
                        });
                    }
                }
                for face in &neighbor.faces {
                    budget.spend()?;
                    if face.object == Some(ignore)
                        || face.object.is_some_and(|id| self.disabled.contains(&id))
                    {
                        continue;
                    }
                    if let Some(contact) = face.polygon.sweep_sphere(
                        position + sphere.center + offset,
                        displacement,
                        sphere.radius,
                        face.two_sided,
                    ) && hit
                        .as_ref()
                        .is_none_or(|h| contact.fraction < h.contact.fraction)
                    {
                        hit = Some(Hit {
                            contact,
                            object: face.object,
                        });
                    }
                }
            }
            for (cell_id, offset) in std::iter::once((cell, Vec3::ZERO)).chain(
                geometry
                    .portals
                    .iter()
                    .map(|p| (p.destination, p.translation)),
            ) {
                for other in dynamics.iter().filter(|d| {
                    d.cell == cell_id
                        && d.object != ignore
                        && crate::players_collide(player_status, d.player_status)
                }) {
                    budget.spend()?;
                    if let Some(contact) = other.cast(
                        position + sphere.center + offset,
                        displacement,
                        sphere.radius,
                    ) && hit
                        .as_ref()
                        .is_none_or(|h| contact.fraction < h.contact.fraction)
                    {
                        hit = Some(Hit {
                            contact,
                            object: Some(other.object),
                        });
                    }
                }
            }
        }
        Ok(hit)
    }
    /// Continuous casts against every collision primitive; bounded slide and
    /// stair trials. Query errors return no accepted state to the body owner.
    pub fn move_body(&self, input: GeometryStep<'_>) -> Result<GeometryMove, GeometryError> {
        let GeometryStep {
            cell,
            position,
            velocity,
            seconds,
            shape,
            dynamics,
            ignore,
            was_grounded,
            player_status,
            allowed_restrictions,
        } = input;
        if !position.is_finite()
            || !velocity.is_finite()
            || !seconds.is_finite()
            || !(0.0..=0.1).contains(&seconds)
            || dynamics.len() > 65536
        {
            return Err(GeometryError::Invalid);
        }
        let mut budget = Budget(262144);
        let mut current = position;
        let mut remaining = velocity * seconds;
        let mut accepted_velocity = velocity;
        let mut grounded = false;
        let mut contacted_object = None;
        for _ in 0..4 {
            if remaining.length_squared() < 0.00000001 {
                break;
            }
            let Some(hit) = self.sweep(
                cell,
                current,
                remaining,
                shape.spheres(),
                dynamics,
                ignore,
                &mut budget,
                player_status,
            )?
            else {
                current = current + remaining;
                break;
            };
            if contacted_object.is_none() {
                contacted_object = hit.object;
            }
            let fraction = (hit.contact.fraction - 0.00001).max(0.0);
            current = current + remaining * fraction;
            remaining = remaining * (1.0 - fraction);
            let normal = hit.contact.normal;
            if normal.z >= 0.66417414 {
                grounded = true;
            }
            if was_grounded && normal.z < 0.66417414 && shape.step_up > 0.0 {
                let up = Vec3::new(0.0, 0.0, shape.step_up);
                let horizontal = Vec3::new(remaining.x, remaining.y, 0.0);
                if self
                    .sweep(
                        cell,
                        current,
                        up,
                        shape.spheres(),
                        dynamics,
                        ignore,
                        &mut budget,
                        player_status,
                    )?
                    .is_none()
                    && self
                        .sweep(
                            cell,
                            current + up,
                            horizontal,
                            shape.spheres(),
                            dynamics,
                            ignore,
                            &mut budget,
                            player_status,
                        )?
                        .is_none()
                {
                    let down = Vec3::new(0.0, 0.0, -(shape.step_up + shape.step_down));
                    if let Some(landing) = self.sweep(
                        cell,
                        current + up + horizontal,
                        down,
                        shape.spheres(),
                        dynamics,
                        ignore,
                        &mut budget,
                        player_status,
                    )? && landing.contact.normal.z >= 0.66417414
                    {
                        current = current + up + horizontal + down * landing.contact.fraction;
                        grounded = true;
                        accepted_velocity.z = 0.0;
                        break;
                    }
                }
            }
            let into = remaining.dot(normal);
            if into < 0.0 {
                remaining = remaining - normal * into;
            }
            let into = accepted_velocity.dot(normal);
            if into < 0.0 {
                accepted_velocity = accepted_velocity - normal * into;
            }
        }
        if was_grounded && velocity.z <= 0.0 {
            let down = Vec3::new(0.0, 0.0, -shape.step_down.max(0.04));
            if let Some(hit) = self.sweep(
                cell,
                current,
                down,
                shape.spheres(),
                dynamics,
                ignore,
                &mut budget,
                player_status,
            )? && hit.contact.normal.z >= 0.66417414
            {
                current = current + down * hit.contact.fraction;
                grounded = true;
                accepted_velocity.z = 0.0;
            }
        }
        let mut destination = cell;
        // Same-landblock portals preserve coordinates; outdoor block seams carry
        // their explicit prepared translation. Destination must already exist.
        for portal in &self
            .cells
            .get(&cell)
            .ok_or(GeometryError::MissingCell)?
            .portals
        {
            budget.spend()?;
            let plane = portal.polygon.plane();
            let start = plane.normal.dot(position) + plane.distance;
            let end = plane.normal.dot(current) + plane.distance;
            if start * end < 0.0 || start == 0.0 && end < 0.0 {
                let t = start / (start - end);
                let point = position + (current - position) * t;
                if portal.polygon.contains_projection(point, 0.0002) {
                    let next = self
                        .cells
                        .get(&portal.destination)
                        .ok_or(GeometryError::MissingCell)?;
                    if !next
                        .boundary
                        .iter()
                        .all(|p| p.normal.dot(current + portal.translation) + p.distance >= -0.0002)
                    {
                        continue;
                    }
                    destination = portal.destination;
                    current = current + portal.translation;
                    break;
                }
            }
        }
        if destination != cell && player_status.is_some() {
            self.validate_access(destination, allowed_restrictions)?;
        }
        self.validate_placement_for(destination, current, shape, dynamics, ignore, player_status)?;
        Ok(GeometryMove {
            cell: destination,
            position: current,
            velocity: accepted_velocity,
            grounded,
            contacted_object,
        })
    }
}

mod unload;

mod placement;
