//! Valuable final-item pickup shortens the exact persisted corpse deadline.
use crate::game_inventory::FrozenInventoryItem;
use bace_persistence::PlacementOperation;
use bace_simulation::CorpseDecayChange;
use bace_storage_codec::ItemSaveV5;
use std::collections::BTreeSet;
pub(super) fn freeze(
    operation: &mut PlacementOperation,
    sources: &[FrozenInventoryItem],
    changes: &[CorpseDecayChange],
    captured_tick: u64,
    unix_millis: u64,
) -> Result<(), String> {
    let unix = i64::try_from(unix_millis / 1000).map_err(|_| "corpse clock overflow")?;
    let mut seen = BTreeSet::new();
    for change in changes {
        if !seen.insert(change.corpse)
            || change.prepared_tick > captured_tick
            || change.after_expires_tick > change.before_expires_tick
            || change.after_expires_tick
                > change
                    .prepared_tick
                    .checked_add(450)
                    .ok_or("corpse decay overflow")?
            || !operation
                .snapshots
                .iter()
                .any(|r| r.object_id == change.corpse.0)
        {
            return Err("corpse decay fence mismatch".into());
        }
    }
    for row in &mut operation.snapshots {
        let Some(source) = sources.iter().find(|s| s.entity.object_id == row.object_id) else {
            continue;
        };
        let Some(before) = &source.corpse else {
            if source.entity.state.weenie_type == 11 {
                return Err("corpse descriptor missing".into());
            }
            continue;
        };
        if before.corpse.entity.object_id != row.object_id
            || before.source.is_none()
            || before.operation.is_none()
            || before.corpse.entity.state.weenie_id != source.entity.state.weenie_id
            || before.corpse.entity.mutation_revision > source.entity.mutation_revision
        {
            return Err("corpse baseline identity mismatch".into());
        }
        let item = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
        let mut after = before.clone();
        after.corpse.entity = item.previous.previous.previous.entity;
        after.placement = item.previous.previous.previous.placement;
        after.enchantments = item.previous.previous.enchantments;
        if let Some(change) = changes.iter().find(|c| c.corpse.0 == row.object_id) {
            if before.operation != Some(change.death_operation) {
                return Err("corpse death operation mismatch".into());
            }
            let ticks = change.after_expires_tick.saturating_sub(captured_tick);
            let seconds =
                i64::try_from(ticks.div_ceil(30)).map_err(|_| "corpse expiry overflow")?;
            after.corpse.expires_at = before
                .corpse
                .expires_at
                .min(unix.checked_add(seconds).ok_or("corpse expiry overflow")?);
            let remaining = (ticks as f64 / 30.)
                .min(after.corpse.expires_at.saturating_sub(unix).max(0) as f64);
            crate::game_inventory::set(
                &mut after.corpse.entity.state.properties.floats,
                44,
                remaining,
            );
        }
        row.bytes = after.encode().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "corpse_tests.rs"]
mod tests;
