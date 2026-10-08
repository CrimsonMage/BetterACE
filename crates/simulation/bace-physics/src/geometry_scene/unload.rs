//! Immutable region eviction; other admitted cells retain their prepared geometry.
use super::*;
impl GeometryRegion {
    pub fn cell_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.cells.keys().copied()
    }
    pub fn without_landblock(&self, landblock: u16) -> Option<Self> {
        let base = u32::from(landblock) << 16;
        let cells: BTreeMap<_, _> = self
            .cells
            .iter()
            .filter(|(id, _)| **id & 0xffff0000 != base)
            .map(|(&id, cell)| (id, cell.clone()))
            .collect();
        if cells.is_empty() {
            return None;
        }
        let mut result = Self {
            landblock_metric: self.landblock_metric,
            building_cells: self
                .building_cells
                .iter()
                .filter(|id| **id & 0xffff0000 != base)
                .copied()
                .collect(),
            cells: std::sync::Arc::new(cells),
            disabled: self.disabled.clone(),
        };
        result.disabled.retain(|id| {
            result.cells.values().any(|c| {
                c.faces.iter().any(|f| f.object == Some(*id))
                    || c.static_primitives.iter().any(|p| p.object == *id)
            })
        });
        Some(result)
    }
}
