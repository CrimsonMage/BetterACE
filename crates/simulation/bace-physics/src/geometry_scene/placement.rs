//! GDLE ObjCell.cpp::find_cell_list selects a cell using the primary sphere
//! center (lines 326–337), not the object's frame origin. EnvCell.cpp::
//! find_visible_child_cell tries the authored cell then its visible-cell list.
use super::*;
impl GeometryRegion {
    pub(super) fn validate_cell_center(
        &self,
        cell: u32,
        position: Vec3,
        shape: &CollisionShape,
    ) -> Result<(), GeometryError> {
        let geometry = self.cells.get(&cell).ok_or(GeometryError::MissingCell)?;
        let primary = shape.spheres.first().ok_or(GeometryError::Invalid)?;
        let center = position + primary.center;
        if !center.is_finite() {
            return Err(GeometryError::Invalid);
        }
        if geometry
            .boundary
            .iter()
            .any(|p| p.normal.dot(center) + p.distance < -0.0002)
        {
            return Err(GeometryError::MissingCell);
        }
        Ok(())
    }
    /// Resolve an authored indoor cell hint using its exact cold-prepared PVS
    /// order. This never moves a position and does not replace shape admission.
    pub fn placement_cell(
        &self,
        authored: u32,
        position: Vec3,
        shape: &CollisionShape,
        visible: &[u32],
    ) -> Result<u32, GeometryError> {
        if !position.is_finite() || visible.len() > 4096 {
            return Err(GeometryError::Invalid);
        }
        if !self.cells.contains_key(&authored) {
            return Err(GeometryError::MissingCell);
        }
        for &cell in visible {
            if cell >> 16 != authored >> 16 || !self.cells.contains_key(&cell) {
                return Err(GeometryError::MissingCell);
            }
        }
        for cell in std::iter::once(authored).chain(visible.iter().copied()) {
            if self.validate_cell_center(cell, position, shape).is_ok() {
                return Ok(cell);
            }
        }
        Err(GeometryError::MissingCell)
    }
}
