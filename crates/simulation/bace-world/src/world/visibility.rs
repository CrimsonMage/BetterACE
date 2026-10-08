//! Pinned ACE ObjectMaint PVS rules. Spatial indexes retain identities/cells only;
//! all distances read the single accepted actor/projectile owner at query time.
use super::*;
use bace_gameplay_api::visibility::VisibilityCandidate;
use std::{collections::BTreeSet, sync::Arc};
const MAX_ENTITIES: usize = 65536;
const MAX_CELLS: usize = 65536;
const MAX_REFERENCES: usize = 1_048_576;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCellVisibility {
    pub cell: CellId,
    pub seen_outside: bool,
    pub visible_cells: Arc<[CellId]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VisibilityError {
    #[error("visibility geometry metadata is missing")]
    MissingCell,
    #[error("invalid visibility geometry metadata")]
    InvalidMetadata,
    #[error("visibility capacity exceeded")]
    Capacity,
    #[error("visibility observer is missing")]
    MissingActor,
    #[error("accepted visibility position is invalid")]
    InvalidPosition,
}
pub(super) struct VisibilityState {
    cells: BTreeMap<CellId, PreparedCellVisibility>,
    by_cell: BTreeMap<CellId, BTreeSet<EntityId>>,
    by_entity: BTreeMap<EntityId, CellId>,
    scratch: Vec<EntityId>,
    dirty: bool,
}
impl Default for VisibilityState {
    fn default() -> Self {
        Self {
            cells: BTreeMap::new(),
            by_cell: BTreeMap::new(),
            by_entity: BTreeMap::new(),
            scratch: Vec::new(),
            dirty: true,
        }
    }
}
impl VisibilityState {
    pub(super) fn invalidate(&mut self) {
        self.dirty = true;
    }
    pub(super) fn track(&mut self, id: EntityId, cell: CellId) {
        if self.by_entity.get(&id) == Some(&cell) {
            return;
        }
        if let Some(old) = self.by_entity.insert(id, cell)
            && let Some(bucket) = self.by_cell.get_mut(&old)
        {
            bucket.remove(&id);
            if bucket.is_empty() {
                self.by_cell.remove(&old);
            }
        }
        self.by_cell.entry(cell).or_default().insert(id);
    }
    pub(super) fn untrack(&mut self, id: EntityId) {
        if let Some(old) = self.by_entity.remove(&id)
            && let Some(bucket) = self.by_cell.get_mut(&old)
        {
            bucket.remove(&id);
            if bucket.is_empty() {
                self.by_cell.remove(&old);
            }
        }
    }
    fn refresh(
        &mut self,
        actors: &BTreeMap<EntityId, Actor>,
        projectiles: &BTreeMap<EntityId, OwnedProjectile>,
    ) -> Result<(), VisibilityError> {
        if !self.dirty {
            return Ok(());
        }
        if actors.len().saturating_add(projectiles.len()) > MAX_ENTITIES {
            return Err(VisibilityError::Capacity);
        }
        self.scratch.clear();
        self.scratch.extend(
            self.by_entity
                .keys()
                .copied()
                .filter(|id| !actors.contains_key(id) && !projectiles.contains_key(id)),
        );
        while let Some(id) = self.scratch.pop() {
            self.untrack(id);
        }
        for (&id, actor) in actors {
            self.track(id, actor.cell);
        }
        for (&id, projectile) in projectiles {
            self.track(id, projectile.cell);
        }
        self.dirty = false;
        Ok(())
    }
    fn outside(&self, cell: CellId) -> Result<bool, VisibilityError> {
        if outdoor(cell) {
            Ok(true)
        } else {
            self.cells
                .get(&cell)
                .map(|v| v.seen_outside)
                .ok_or(VisibilityError::MissingCell)
        }
    }
}
fn outdoor(cell: CellId) -> bool {
    (1..=64).contains(&(cell.0 & 0xffff))
}
fn indoor(cell: CellId) -> bool {
    (0x100..=0xfffd).contains(&(cell.0 & 0xffff))
}
fn adjacent(a: CellId, b: CellId) -> bool {
    let ax = (a.0 >> 24) as i32;
    let ay = ((a.0 >> 16) & 255) as i32;
    let bx = (b.0 >> 24) as i32;
    let by = ((b.0 >> 16) & 255) as i32;
    (ax - bx).abs() <= 1 && (ay - by).abs() <= 1
}
/// Exact scalar ACE Position.Distance2DSquared operation order. Height is ignored.
pub fn visibility_distance_squared(
    a: CellId,
    p: Vec3,
    b: CellId,
    q: Vec3,
) -> Result<f32, VisibilityError> {
    if !p.is_finite() || !q.is_finite() {
        return Err(VisibilityError::InvalidPosition);
    }
    let (dx, dy) = if a.0 >> 16 == b.0 >> 16 {
        (p.x - q.x, p.y - q.y)
    } else {
        (
            (((a.0 >> 24) as i32 - (b.0 >> 24) as i32) * 192) as f32 + p.x - q.x,
            ((((a.0 >> 16) & 255) as i32 - ((b.0 >> 16) & 255) as i32) * 192) as f32 + p.y - q.y,
        )
    };
    let value = dx * dx + dy * dy;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(VisibilityError::InvalidPosition)
    }
}
impl World {
    pub fn validate_cell_visibility(
        &self,
        rows: &[PreparedCellVisibility],
    ) -> Result<(), VisibilityError> {
        if rows.len() > MAX_CELLS {
            return Err(VisibilityError::Capacity);
        }
        let mut ids = BTreeSet::new();
        let mut refs = self
            .visibility
            .cells
            .values()
            .map(|c| c.visible_cells.len())
            .sum::<usize>();
        let mut additions = 0;
        for row in rows {
            if !indoor(row.cell)
                || !ids.insert(row.cell)
                || row.visible_cells.len() > 4096
                || row.visible_cells.windows(2).any(|p| p[0] >= p[1])
                || row
                    .visible_cells
                    .iter()
                    .any(|c| !indoor(*c) || c.0 >> 16 != row.cell.0 >> 16)
            {
                return Err(VisibilityError::InvalidMetadata);
            }
            if let Some(old) = self.visibility.cells.get(&row.cell) {
                refs -= old.visible_cells.len();
            } else {
                additions += 1;
            }
            refs = refs
                .checked_add(row.visible_cells.len())
                .ok_or(VisibilityError::Capacity)?;
            if refs > MAX_REFERENCES {
                return Err(VisibilityError::Capacity);
            }
        }
        if refs > MAX_REFERENCES
            || self.visibility.cells.len().saturating_add(additions) > MAX_CELLS
        {
            return Err(VisibilityError::Capacity);
        }
        if rows
            .iter()
            .flat_map(|r| r.visible_cells.iter())
            .any(|cell| !ids.contains(cell) && !self.visibility.cells.contains_key(cell))
        {
            return Err(VisibilityError::MissingCell);
        }
        Ok(())
    }
    pub fn install_cell_visibility(
        &mut self,
        rows: Vec<PreparedCellVisibility>,
    ) -> Result<(), VisibilityError> {
        self.validate_cell_visibility(&rows)?;
        for row in rows {
            self.visibility.cells.insert(row.cell, row);
        }
        self.visibility.invalidate();
        Ok(())
    }
    pub fn remove_visibility_region(&mut self, landblock: u16) {
        self.visibility
            .cells
            .retain(|cell, _| cell.0 >> 16 != u32::from(landblock));
        self.visibility.invalidate();
    }
    pub fn cell_visibility(&self, cell: CellId) -> Option<&PreparedCellVisibility> {
        self.visibility.cells.get(&cell)
    }
    /// Caller supplies reusable output storage. Errors clear it and do not alter
    /// per-observer knowledge or authorize any partial create/remove publication.
    pub fn visibility_candidates(
        &mut self,
        observer: EntityId,
        output: &mut Vec<VisibilityCandidate>,
        limit: usize,
    ) -> Result<(), VisibilityError> {
        output.clear();
        if !(1..=MAX_ENTITIES).contains(&limit) {
            return Err(VisibilityError::Capacity);
        }
        self.visibility.refresh(&self.actors, &self.projectiles)?;
        let actor = self
            .actors
            .get(&observer)
            .ok_or(VisibilityError::MissingActor)?;
        let cell = actor.cell;
        let position = actor.body.accepted().position();
        if !outdoor(cell) && !self.visibility.cells.contains_key(&cell) {
            return Err(VisibilityError::MissingCell);
        }
        let mut scratch = std::mem::take(&mut self.visibility.scratch);
        scratch.clear();
        let result = (|| {
            if self.is_in_portal_transit(observer) {
                return Ok(());
            }
            let mut append = |ids: &BTreeSet<EntityId>| -> Result<(), VisibilityError> {
                if scratch.len().saturating_add(ids.len()) > MAX_ENTITIES * 2 {
                    return Err(VisibilityError::Capacity);
                }
                scratch.extend(ids.iter().copied());
                Ok(())
            };
            if let Some(metadata) = self.visibility.cells.get(&cell) {
                if let Some(ids) = self.visibility.by_cell.get(&cell) {
                    append(ids)?;
                }
                for visible in metadata.visible_cells.iter().filter(|c| **c != cell) {
                    if !self.visibility.cells.contains_key(visible) {
                        return Err(VisibilityError::MissingCell);
                    }
                    if let Some(ids) = self.visibility.by_cell.get(visible) {
                        append(ids)?;
                    }
                }
            }
            if self.visibility.outside(cell)? {
                // BTree ranges skip all non-neighbor landblocks; membership
                // retains only IDs, and accepted positions are read below.
                let x = (cell.0 >> 24) as i32;
                let y = ((cell.0 >> 16) & 255) as i32;
                for nx in (x - 1).max(0)..=(x + 1).min(255) {
                    for ny in (y - 1).max(0)..=(y + 1).min(255) {
                        let start = CellId(((nx as u32) << 24) | ((ny as u32) << 16));
                        let end = CellId(start.0 | 0xffff);
                        for (target_cell, ids) in self.visibility.by_cell.range(start..=end) {
                            if self.visibility.outside(*target_cell)? {
                                append(ids)?;
                            }
                        }
                    }
                }
            }
            scratch.sort_unstable();
            scratch.dedup();
            for &id in &scratch {
                if id == observer || self.is_in_portal_transit(id) {
                    continue;
                }
                let (target_cell, target_position) = if let Some(actor) = self.actors.get(&id) {
                    (actor.cell, actor.body.accepted().position())
                } else if let Some(p) = self.projectiles.get(&id) {
                    (p.cell, p.body.position())
                } else {
                    return Err(VisibilityError::MissingActor);
                };
                if self
                    .dormant_landblocks
                    .contains(&((target_cell.0 >> 16) as u16))
                {
                    continue;
                }
                if output.len() == limit {
                    return Err(VisibilityError::Capacity);
                }
                output.push(VisibilityCandidate {
                    entity: id,
                    distance_squared: visibility_distance_squared(
                        cell,
                        position,
                        target_cell,
                        target_position,
                    )?,
                });
            }
            Ok(())
        })();
        scratch.clear();
        self.visibility.scratch = scratch;
        if result.is_err() {
            output.clear();
        }
        result
    }
    /// Pure source-cell predicate used by tests and bounded policy consumers.
    pub fn cells_share_visibility(
        &self,
        observer: CellId,
        target: CellId,
    ) -> Result<bool, VisibilityError> {
        if outdoor(observer) {
            return Ok(adjacent(observer, target) && self.visibility.outside(target)?);
        }
        let row = self
            .visibility
            .cells
            .get(&observer)
            .ok_or(VisibilityError::MissingCell)?;
        Ok(observer == target
            || row.visible_cells.binary_search(&target).is_ok()
            || row.seen_outside && adjacent(observer, target) && self.visibility.outside(target)?)
    }
}

mod launches;
