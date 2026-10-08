//! Bounded scalar GDLE BSP placement/contact queries. Not a movement solver.
//! Source/PhatSDK/{BSPData,Transition}.cpp at 353cbab52ef7da2b7063bc3e3f008461d8531693.
//! GDLEnhanced source attribution retained; AGPL-3.0-only.
use crate::gdle_polygon::{CollisionPlane, EPSILON, GdlePolygon, bounded};
use bace_geometry::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionSphere {
    pub center: Vec3,
    pub radius: f32,
}
impl CollisionSphere {
    fn validate(self, query: bool) -> Result<(), BspQueryError> {
        if !bounded(self.center)
            || !self.radius.is_finite()
            || self.radius > 1_000_000.0
            || self.radius < 0.0
            || (query && self.radius <= EPSILON)
        {
            return Err(BspQueryError::InvalidGeometry);
        }
        Ok(())
    }
    fn intersects(self, other: Self) -> bool {
        let offset = self.center - other.center;
        let reach = self.radius + other.radius;
        let distance = offset.dot(offset) - (reach * reach);
        EPSILON > distance
    }
}
#[derive(Clone, Debug)]
pub enum BspCollisionNode {
    Branch {
        bounds: CollisionSphere,
        plane: CollisionPlane,
        positive: usize,
        negative: usize,
    },
    Leaf {
        bounds: CollisionSphere,
        solid: bool,
        polygons: Vec<usize>,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct BspLimits {
    pub max_nodes: usize,
    pub max_polygons: usize,
    pub max_polygon_references: usize,
    pub max_depth: usize,
}
impl Default for BspLimits {
    fn default() -> Self {
        Self {
            max_nodes: 65_536,
            max_polygons: 65_536,
            max_polygon_references: 262_144,
            max_depth: 128,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct BspQueryBudget {
    nodes: usize,
    polygons: usize,
    steps: usize,
}
impl BspQueryBudget {
    pub fn new(nodes: usize, polygons: usize, steps: usize) -> Self {
        Self {
            nodes,
            polygons,
            steps,
        }
    }
}
impl Default for BspQueryBudget {
    fn default() -> Self {
        Self::new(262_144, 262_144, 1024)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BspQueryError {
    #[error("invalid or unsupported collision geometry")]
    InvalidGeometry,
    #[error("BSP preparation or query budget exhausted")]
    Limit,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BspContact {
    pub polygon: usize,
    pub point: Vec3,
    pub plane: CollisionPlane,
    pub approaching: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BspPathContact {
    pub step: u32,
    pub total_steps: u32,
    pub center: Vec3,
    pub solid: bool,
    pub contact: Option<BspContact>,
}
#[derive(Clone, Debug)]
pub struct GdleBspCell {
    nodes: Vec<BspCollisionNode>,
    polygons: Vec<GdlePolygon>,
}
fn spend(left: &mut usize) -> Result<(), BspQueryError> {
    *left = left.checked_sub(1).ok_or(BspQueryError::Limit)?;
    Ok(())
}
impl GdleBspCell {
    /// Root is node zero. Reject cycles, shared children, unreachable nodes and
    /// missing indices before queries; no pointer aliasing or per-query allocation.
    pub fn prepare(
        nodes: Vec<BspCollisionNode>,
        polygons: Vec<GdlePolygon>,
        limits: BspLimits,
    ) -> Result<Self, BspQueryError> {
        if nodes.is_empty()
            || nodes.len() > limits.max_nodes
            || polygons.len() > limits.max_polygons
            || limits.max_depth > 128
            || limits.max_depth == 0
        {
            return Err(BspQueryError::Limit);
        }
        let mut seen = vec![false; nodes.len()];
        let mut stack = vec![(0, 0)];
        let mut references = 0usize;
        while let Some((index, depth)) = stack.pop() {
            if depth >= limits.max_depth {
                return Err(BspQueryError::Limit);
            }
            let node = nodes.get(index).ok_or(BspQueryError::InvalidGeometry)?;
            if seen[index] {
                return Err(BspQueryError::InvalidGeometry);
            }
            seen[index] = true;
            match node {
                BspCollisionNode::Branch {
                    bounds,
                    plane,
                    positive,
                    negative,
                } => {
                    bounds.validate(false)?;
                    plane.validate()?;
                    stack.push((*negative, depth + 1));
                    stack.push((*positive, depth + 1));
                }
                BspCollisionNode::Leaf {
                    bounds,
                    polygons: ids,
                    ..
                } => {
                    // GDLE returns immediately for an empty leaf before reading
                    // its bounds. Retail DATs retain finite 0xCDCDCDCD filler in
                    // those unused sphere fields; do not turn padding into a
                    // collision-shape requirement. Nonfinite input still fails.
                    if ids.is_empty() {
                        if !bounds.center.is_finite() || !bounds.radius.is_finite() {
                            return Err(BspQueryError::InvalidGeometry);
                        }
                    } else {
                        bounds.validate(false)?;
                    }
                    references = references
                        .checked_add(ids.len())
                        .ok_or(BspQueryError::Limit)?;
                    if references > limits.max_polygon_references {
                        return Err(BspQueryError::Limit);
                    }
                    if ids.iter().any(|i| *i >= polygons.len()) {
                        return Err(BspQueryError::InvalidGeometry);
                    }
                }
            }
        }
        if seen.iter().any(|v| !*v) {
            return Err(BspQueryError::InvalidGeometry);
        }
        Ok(Self { nodes, polygons })
    }
    pub fn intersects_solid(
        &self,
        sphere: CollisionSphere,
        check_center: bool,
        budget: &mut BspQueryBudget,
    ) -> Result<bool, BspQueryError> {
        sphere.validate(true)?;
        self.solid(0, sphere, check_center, budget)
    }
    fn solid(
        &self,
        index: usize,
        sphere: CollisionSphere,
        check_center: bool,
        budget: &mut BspQueryBudget,
    ) -> Result<bool, BspQueryError> {
        spend(&mut budget.nodes)?;
        match &self.nodes[index] {
            BspCollisionNode::Leaf {
                bounds,
                solid,
                polygons,
            } => {
                if polygons.is_empty() {
                    return Ok(false);
                }
                if *solid && check_center {
                    return Ok(true);
                }
                if !bounds.intersects(sphere) {
                    return Ok(false);
                }
                for &polygon in polygons {
                    spend(&mut budget.polygons)?;
                    if self.polygons[polygon].contact(sphere).is_some() {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            BspCollisionNode::Branch {
                bounds,
                plane,
                positive,
                negative,
            } => {
                if !bounds.intersects(sphere) {
                    return Ok(false);
                }
                let distance = plane.dot(sphere.center);
                let reach = sphere.radius - EPSILON;
                if distance >= reach {
                    return self.solid(*positive, sphere, check_center, budget);
                }
                if distance <= -reach {
                    return self.solid(*negative, sphere, check_center, budget);
                }
                let positive_center = if distance < 0.0 { false } else { check_center };
                if self.solid(*positive, sphere, positive_center, budget)? {
                    return Ok(true);
                }
                self.solid(
                    *negative,
                    sphere,
                    if distance < 0.0 { check_center } else { false },
                    budget,
                )
            }
        }
    }
    /// Preserve pos_hits_sphere's negative contact: a polygon may be touched while
    /// moving away (approaching=false), which upstream communicates via hit_poly.
    pub fn sphere_contact(
        &self,
        sphere: CollisionSphere,
        movement: Vec3,
        budget: &mut BspQueryBudget,
    ) -> Result<Option<BspContact>, BspQueryError> {
        sphere.validate(true)?;
        if !bounded(movement) {
            return Err(BspQueryError::InvalidGeometry);
        }
        let mut contact = None;
        self.contact(0, sphere, movement, budget, &mut contact)?;
        Ok(contact)
    }
    fn contact(
        &self,
        index: usize,
        sphere: CollisionSphere,
        movement: Vec3,
        budget: &mut BspQueryBudget,
        result: &mut Option<BspContact>,
    ) -> Result<bool, BspQueryError> {
        spend(&mut budget.nodes)?;
        match &self.nodes[index] {
            BspCollisionNode::Leaf {
                bounds, polygons, ..
            } => {
                if polygons.is_empty() || !bounds.intersects(sphere) {
                    return Ok(false);
                }
                for &index in polygons {
                    spend(&mut budget.polygons)?;
                    let polygon = &self.polygons[index];
                    if let Some(point) = polygon.contact(sphere) {
                        let approaching = movement.dot(polygon.plane.normal) < 0.0;
                        *result = Some(BspContact {
                            polygon: index,
                            point,
                            plane: polygon.plane,
                            approaching,
                        });
                        if approaching {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            BspCollisionNode::Branch {
                bounds,
                plane,
                positive,
                negative,
            } => {
                if !bounds.intersects(sphere) {
                    return Ok(false);
                }
                let distance = plane.dot(sphere.center);
                let reach = sphere.radius - EPSILON;
                if distance >= reach {
                    return self.contact(*positive, sphere, movement, budget, result);
                }
                if distance <= -reach {
                    return self.contact(*negative, sphere, movement, budget, result);
                }
                if self.contact(*positive, sphere, movement, budget, result)? {
                    return Ok(true);
                }
                self.contact(*negative, sphere, movement, budget, result)
            }
        }
    }
    /// GDLE non-viewer radius-sized subdivision, followed by its exact BSP queries.
    /// Returns the first colliding sample, not TOI, sliding, stepping or an accepted
    /// state. A budget error rejects the whole observation, never an open path.
    pub fn sample_path(
        &self,
        start: CollisionSphere,
        end: Vec3,
        budget: &mut BspQueryBudget,
    ) -> Result<Option<BspPathContact>, BspQueryError> {
        start.validate(true)?;
        if !bounded(end) {
            return Err(BspQueryError::InvalidGeometry);
        }
        let (count, step) = sphere_path_steps(start, end, budget.steps)?;
        if count == 0 {
            return Ok(None);
        }
        let mut sphere = start;
        for i in 1..=count {
            spend(&mut budget.steps)?;
            sphere.center = sphere.center + step;
            let solid = self.intersects_solid(sphere, true, budget)?;
            let contact = self.sphere_contact(sphere, step, budget)?;
            if solid || contact.is_some_and(|c| c.approaching) {
                return Ok(Some(BspPathContact {
                    step: i,
                    total_steps: count,
                    center: sphere.center,
                    solid,
                    contact,
                }));
            }
        }
        Ok(None)
    }
}

/// Exact non-viewer, same-cell subdivision from GDLE calc_num_steps.
/// A zero-length path has zero samples and must be placement-checked separately.
pub fn sphere_path_steps(
    start: CollisionSphere,
    end: Vec3,
    max_steps: usize,
) -> Result<(u32, Vec3), BspQueryError> {
    start.validate(true)?;
    if !bounded(end) {
        return Err(BspQueryError::InvalidGeometry);
    }
    let offset = end - start.center;
    let ratio = offset.length_squared().sqrt() / start.radius;
    let count = if ratio > 1.0 {
        ratio.ceil()
    } else if offset != Vec3::ZERO {
        1.0
    } else {
        0.0
    };
    if !count.is_finite()
        || f64::from(count) > max_steps as f64
        || f64::from(count) > f64::from(u32::MAX)
    {
        return Err(BspQueryError::Limit);
    }
    if count == 0.0 {
        return Ok((0, Vec3::ZERO));
    }
    Ok((count as u32, offset * ((1.0f64 / f64::from(count)) as f32)))
}
