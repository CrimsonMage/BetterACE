//! Exact NPC journal receipts advance the retained world item baselines.
use super::*;
impl RegionService {
    pub(crate) fn npc_item_source(
        &self,
        id: EntityId,
    ) -> Option<&crate::region_unload_saves::RegionItemSource> {
        self.sources.get(&id.0)
    }
    pub(crate) fn adopt_npc_item_snapshots(
        &mut self,
        rows: &[bace_persistence::SaveSnapshot],
    ) -> Result<(), String> {
        if rows.len() > 1024 {
            return Err("NPC item baseline count".into());
        }
        let mut replacements = Vec::with_capacity(rows.len());
        for row in rows {
            let old = self
                .sources
                .get(&row.object_id)
                .ok_or("NPC item source disappeared")?;
            let decoded =
                bace_storage_codec::ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
            let version = row
                .expected_version
                .checked_add(1)
                .ok_or("NPC item version overflow")?;
            if decoded.entity.object_id != row.object_id
                || decoded.entity.mutation_revision != row.mutation_revision
                || old.item.entity.state.weenie_id != decoded.entity.state.weenie_id
                || old.item.source_destination != decoded.source_destination
                || old.corpse.is_some()
            {
                return Err("NPC item baseline identity".into());
            }
            if old.item.persisted_version == version
                && old.item.entity == decoded.entity
                && old.item.enchantments == decoded.enchantments
                && old.item.source_destination == decoded.source_destination
            {
                continue;
            }
            if old.item.persisted_version != row.expected_version
                || old.item.entity.mutation_revision > row.mutation_revision
            {
                return Err("NPC item baseline version".into());
            }
            let item = crate::game_inventory::FrozenInventoryItem::decode(
                &row.bytes,
                Some(decoded.placement.clone()),
                version,
            )
            .map_err(|e| e.to_string())?;
            replacements.push((row.object_id, item, row.bytes.len()));
        }
        for (id, item, size) in replacements {
            self.sources
                .get_mut(&id)
                .expect("NPC baseline preflight")
                .item = item;
            self.source_bytes.insert(id, size);
        }
        Ok(())
    }
}
