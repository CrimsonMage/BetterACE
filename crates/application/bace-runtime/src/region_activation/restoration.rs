//! Saved world objects retain their exact instance setup/scale, never a newer template.
use super::*;
impl VerifiedRegionAssets {
    pub fn prepare_world_item_shape(
        &mut self,
        state: &bace_content::WeenieV1,
    ) -> Result<Arc<CollisionShape>, String> {
        let setup = state
            .properties
            .data_ids
            .iter()
            .find(|p| p.id == 1)
            .ok_or("saved world item has no setup")?
            .value;
        let scale = state
            .properties
            .floats
            .iter()
            .find(|p| p.id == 39)
            .map_or(1., |p| p.value) as f32;
        let mut budget = 16 * 1024 * 1024;
        let setup = CollisionSetup::decode(&read(&mut self.portal, setup, &mut budget)?)
            .map_err(|e| e.to_string())?;
        crate::world_admission::prepare_collision_shape(&setup, scale)
    }
    pub fn prepare_world_spell_table(&mut self) -> Result<Arc<bace_dat::SpellTable>, String> {
        let mut budget = 64 * 1024 * 1024;
        Ok(Arc::new(
            bace_dat::SpellTable::decode(&read(
                &mut self.portal,
                bace_dat::SpellTable::RECORD_ID,
                &mut budget,
            )?)
            .map_err(|e| e.to_string())?,
        ))
    }
}
