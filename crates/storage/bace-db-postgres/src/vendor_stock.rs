//! Unplaced vendor marker CAS within the same transaction as item placement.
use crate::{PgStore, StoreError};
use bace_persistence::{
    DurableItemPlace, OperationOutcome, SaveAck, StoredAggregate, StoredVendorSource,
    StoredVendorState, StoredVendorStock, StoredVendorStockForest, StoredVendorStockItem,
    VendorStockOperation, VendorStockWrite,
};
use bace_storage_codec::VendorStockSaveV1;
use sqlx::postgres::PgRow;
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};

impl PgStore {
    /// Read a single exact durable vendor source and world placement. Runtime
    /// still checks the active accepted source revision/hash before Use.
    pub async fn load_vendor_source(
        &self,
        vendor_object_id: u32,
    ) -> Result<Option<StoredVendorSource>, StoreError> {
        let row = sqlx::query("SELECT s.version,s.payload,p.cell_id FROM entity_snapshots s JOIN item_places p ON p.item_id=s.object_id AND p.kind=1 WHERE s.object_id=$1")
            .bind(i64::from(vendor_object_id))
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| parse_vendor_source_row(row, vendor_object_id))
            .transpose()
    }

    /// Cold-load a matching vendor aggregate and stock forest under the same
    /// hierarchy sequencer as purchases. An orphaned marker is an error.
    pub async fn load_vendor_state(
        &self,
        vendor_object_id: u32,
    ) -> Result<Option<StoredVendorState>, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_hierarchy(&mut tx).await?;
        let source = sqlx::query("SELECT s.version,s.payload,p.cell_id FROM entity_snapshots s JOIN item_places p ON p.item_id=s.object_id AND p.kind=1 WHERE s.object_id=$1")
            .bind(i64::from(vendor_object_id))
            .fetch_optional(&mut *tx)
            .await?
            .map(|row| parse_vendor_source_row(row, vendor_object_id))
            .transpose()?;
        let forest = load_vendor_stock_forest_locked(&mut tx, vendor_object_id).await?;
        let state = match source {
            Some(source) => Some(StoredVendorState { source, forest }),
            None if forest.is_none() => None,
            None => return Err(StoreError::Invalid("vendor stock missing world source")),
        };
        tx.rollback().await?;
        Ok(state)
    }

    pub async fn load_vendor_stock(
        &self,
        vendor_object_id: u32,
    ) -> Result<Option<StoredVendorStock>, StoreError> {
        let row = sqlx::query(
            "SELECT marker_id,version,stock_revision,source_revision,source_hash,payload FROM vendor_stock_markers WHERE vendor_id=$1",
        )
        .bind(i64::from(vendor_object_id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| parse_marker_row(row, vendor_object_id))
            .transpose()
            .map(|marker| marker.map(|(stored, _)| stored))
    }

    /// Cold-load the complete ordered V5 stock forest under the same hierarchy
    /// sequencer used by placement and owned item writes. Missing/corrupt trees
    /// are errors, never partial stock.
    pub async fn load_vendor_stock_forest(
        &self,
        vendor_object_id: u32,
    ) -> Result<Option<StoredVendorStockForest>, StoreError> {
        let mut tx = self.pool.begin().await?;
        lock_hierarchy(&mut tx).await?;
        let forest = load_vendor_stock_forest_locked(&mut tx, vendor_object_id).await?;
        tx.rollback().await?;
        Ok(forest)
    }

    pub async fn vendor_stock_operation(
        &self,
        operation: &VendorStockOperation,
    ) -> Result<OperationOutcome, StoreError> {
        crate::placements::execute_vendor(
            self,
            &operation.inventory,
            operation.world_epoch,
            operation.vendor_expected_version,
            &operation.marker,
        )
        .await
    }
}

async fn lock_hierarchy(tx: &mut Transaction<'_, Postgres>) -> Result<(), StoreError> {
    sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn parse_vendor_source_row(
    row: PgRow,
    vendor_object_id: u32,
) -> Result<StoredVendorSource, StoreError> {
    let cell = u32::try_from(row.get::<i64, _>("cell_id"))
        .map_err(|_| StoreError::Invalid("vendor world cell"))?;
    let bytes: Vec<u8> = row.get("payload");
    let source = bace_storage_codec::ItemSaveV5::decode(&bytes)
        .map_err(|_| StoreError::Invalid("vendor source requires item V5"))?;
    if source.entity.object_id != vendor_object_id
        || !matches!(source.previous.previous.previous.placement, bace_storage_codec::ItemPlacementV2::World(ref position) if position.obj_cell_id == cell)
    {
        return Err(StoreError::Invalid("vendor source world identity"));
    }
    Ok(StoredVendorSource {
        aggregate: StoredAggregate {
            object_id: vendor_object_id,
            persisted_version: row.get("version"),
            bytes,
        },
        cell,
    })
}

async fn load_vendor_stock_forest_locked(
    tx: &mut Transaction<'_, Postgres>,
    vendor_object_id: u32,
) -> Result<Option<StoredVendorStockForest>, StoreError> {
    let row = sqlx::query("SELECT marker_id,version,stock_revision,source_revision,source_hash,payload FROM vendor_stock_markers WHERE vendor_id=$1")
            .bind(i64::from(vendor_object_id))
            .fetch_optional(&mut **tx)
            .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (stored_marker, marker) = parse_marker_row(row, vendor_object_id)?;
    let ids: Vec<i64> = marker
        .defaults
        .iter()
        .map(|entry| (entry.root, &entry.child_ids))
        .chain(
            marker
                .unique
                .iter()
                .map(|entry| (entry.root, &entry.child_ids)),
        )
        .flat_map(|(root, children)| std::iter::once(root).chain(children.iter().copied()))
        .map(i64::from)
        .collect();
    if ids.is_empty() {
        return Ok(Some(StoredVendorStockForest {
            marker: stored_marker,
            items: Vec::new(),
        }));
    }
    let bounds = sqlx::query("SELECT count(*) AS count,coalesce(sum(octet_length(payload)),0) AS bytes FROM entity_snapshots WHERE object_id=ANY($1)")
            .bind(&ids)
            .fetch_one(&mut **tx)
            .await?;
    if bounds.get::<i64, _>("count") != ids.len() as i64
        || bounds.get::<i64, _>("bytes") > 64 * 1024 * 1024
    {
        return Err(StoreError::Invalid("vendor stock forest count or bytes"));
    }
    let rows = sqlx::query("SELECT s.object_id,s.version,s.payload,o.container_id,o.slot,o.pack_slot,o.equipped FROM entity_snapshots s JOIN item_ownership o ON o.item_id=s.object_id JOIN item_places p ON p.item_id=s.object_id AND p.kind=0 WHERE s.object_id=ANY($1)")
            .bind(&ids)
            .fetch_all(&mut **tx)
            .await?;
    if rows.len() != ids.len() {
        return Err(StoreError::Invalid("vendor stock forest placement"));
    }
    let mut located = BTreeMap::new();
    for row in rows {
        let id = u32::try_from(row.get::<i64, _>("object_id"))
            .map_err(|_| StoreError::Invalid("vendor stock item id"))?;
        let placement = DurableItemPlace::Contained {
            container: u32::try_from(row.get::<i64, _>("container_id"))
                .map_err(|_| StoreError::Invalid("vendor stock container id"))?,
            slot: u32::try_from(row.get::<i64, _>("slot"))
                .map_err(|_| StoreError::Invalid("vendor stock item slot"))?,
            pack_slot: row.get("pack_slot"),
            equipped: u32::try_from(row.get::<i64, _>("equipped"))
                .map_err(|_| StoreError::Invalid("vendor stock equipment slot"))?,
        };
        let bytes: Vec<u8> = row.get("payload");
        let item = bace_storage_codec::ItemSaveV5::decode(&bytes)
            .map_err(|_| StoreError::Invalid("vendor stock item V5 payload"))?;
        if item.entity.object_id != id
            || crate::placements::dto_place(&item.previous.previous.previous.placement) != placement
        {
            return Err(StoreError::Invalid("vendor stock item identity"));
        }
        located.insert(
            id,
            StoredVendorStockItem {
                aggregate: StoredAggregate {
                    object_id: id,
                    persisted_version: row.get("version"),
                    bytes,
                },
                placement,
            },
        );
    }
    let mut items = Vec::with_capacity(ids.len());
    for (root, children) in marker
        .defaults
        .iter()
        .map(|entry| (entry.root, &entry.child_ids))
        .chain(
            marker
                .unique
                .iter()
                .map(|entry| (entry.root, &entry.child_ids)),
        )
    {
        let root_item = located
            .remove(&root)
            .ok_or(StoreError::Invalid("vendor stock missing root"))?;
        if !matches!(root_item.placement, DurableItemPlace::Contained { container, .. } if container == vendor_object_id)
        {
            return Err(StoreError::Invalid("vendor stock root owner"));
        }
        items.push(root_item);
        let mut accepted = BTreeSet::from([root]);
        for child in children {
            let item = located
                .remove(child)
                .ok_or(StoreError::Invalid("vendor stock missing child"))?;
            if !matches!(item.placement, DurableItemPlace::Contained { container, .. } if accepted.contains(&container))
            {
                return Err(StoreError::Invalid("vendor stock child ancestry"));
            }
            accepted.insert(*child);
            items.push(item);
        }
    }
    if !located.is_empty() {
        return Err(StoreError::Invalid("vendor stock extra item"));
    }
    Ok(Some(StoredVendorStockForest {
        marker: stored_marker,
        items,
    }))
}

fn parse_marker_row(
    row: PgRow,
    vendor_object_id: u32,
) -> Result<(StoredVendorStock, VendorStockSaveV1), StoreError> {
    let marker_object_id = u32::try_from(row.get::<i64, _>("marker_id"))
        .map_err(|_| StoreError::Invalid("vendor marker id"))?;
    let bytes: Vec<u8> = row.get("payload");
    let marker = VendorStockSaveV1::decode(&bytes)
        .map_err(|_| StoreError::Invalid("vendor marker payload"))?;
    if marker.vendor_object_id != vendor_object_id
        || marker.marker_object_id != marker_object_id
        || marker.stock_revision as i64 != row.get::<i64, _>("stock_revision")
        || marker.source_revision as i64 != row.get::<i64, _>("source_revision")
        || marker.source_hash.as_slice() != row.get::<Vec<u8>, _>("source_hash")
    {
        return Err(StoreError::Invalid("vendor marker relational identity"));
    }
    Ok((
        StoredVendorStock {
            vendor_object_id,
            marker_object_id,
            persisted_version: row.get("version"),
            bytes,
        },
        marker,
    ))
}

pub(crate) fn validate(
    write: &VendorStockWrite,
    operation: &bace_persistence::PlacementOperation,
) -> Result<VendorStockSaveV1, StoreError> {
    let marker = VendorStockSaveV1::decode(&write.bytes)
        .map_err(|_| StoreError::Invalid("vendor marker payload"))?;
    if write.marker_object_id != marker.marker_object_id
        || write.expected_version < 0
        || write.expected_version == i64::MAX
        || write.mutation_revision == 0
        || marker.stock_revision <= write.expected_stock_revision
        || marker.stock_revision > i64::MAX as u64
        || marker.source_revision > i64::MAX as u64
        || write.expected_version == 0 && write.expected_stock_revision != 0
        || write.expected_version != 0 && write.expected_stock_revision == 0
        || !operation.participants.contains(&marker.marker_object_id)
        || !operation.participants.contains(&marker.vendor_object_id)
        || operation
            .snapshots
            .iter()
            .any(|s| s.object_id == marker.marker_object_id)
        || operation
            .changes
            .iter()
            .any(|c| c.item == marker.marker_object_id)
    {
        return Err(StoreError::Invalid("vendor marker CAS or participants"));
    }
    Ok(marker)
}

pub(crate) async fn apply(
    tx: &mut Transaction<'_, Postgres>,
    write: &VendorStockWrite,
    marker: &VendorStockSaveV1,
) -> Result<SaveAck, StoreError> {
    let version = if write.expected_version == 0 {
        let collision: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM entity_snapshots WHERE object_id=$1 UNION ALL SELECT 1 FROM item_places WHERE item_id=$1 UNION ALL SELECT 1 FROM players WHERE object_id=$1 UNION ALL SELECT 1 FROM vendor_stock_markers WHERE marker_id=$1)",
        )
        .bind(i64::from(marker.marker_object_id))
        .fetch_one(&mut **tx)
        .await?;
        if collision {
            return Err(StoreError::OwnershipConflict);
        }
        let count = sqlx::query("INSERT INTO vendor_stock_markers(vendor_id,marker_id,version,stock_revision,source_revision,source_hash,payload) VALUES($1,$2,1,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
            .bind(i64::from(marker.vendor_object_id))
            .bind(i64::from(marker.marker_object_id))
            .bind(marker.stock_revision as i64)
            .bind(marker.source_revision as i64)
            .bind(marker.source_hash.as_slice())
            .bind(&write.bytes)
            .execute(&mut **tx)
            .await?
            .rows_affected();
        if count != 1 {
            return Err(StoreError::Conflict(marker.marker_object_id));
        }
        1
    } else {
        let count = sqlx::query("UPDATE vendor_stock_markers SET version=version+1,stock_revision=$5,payload=$6 WHERE vendor_id=$1 AND marker_id=$2 AND version=$3 AND stock_revision=$4 AND source_revision=$7 AND source_hash=$8")
            .bind(i64::from(marker.vendor_object_id))
            .bind(i64::from(marker.marker_object_id))
            .bind(write.expected_version)
            .bind(write.expected_stock_revision as i64)
            .bind(marker.stock_revision as i64)
            .bind(&write.bytes)
            .bind(marker.source_revision as i64)
            .bind(marker.source_hash.as_slice())
            .execute(&mut **tx)
            .await?
            .rows_affected();
        if count != 1 {
            return Err(StoreError::Conflict(marker.marker_object_id));
        }
        write.expected_version + 1
    };
    Ok(SaveAck {
        object_id: marker.marker_object_id,
        mutation_revision: write.mutation_revision,
        persisted_version: version,
    })
}

/// Check the committed relational forest, including unchanged stock, before
/// publishing the marker. Each descendant must follow a previously named owner
/// in its own tree; every named item must be a frozen V5 item.
pub(crate) async fn validate_stock_forest(
    tx: &mut Transaction<'_, Postgres>,
    marker: &VendorStockSaveV1,
) -> Result<(), StoreError> {
    let mut referenced_ids = BTreeSet::new();
    for (root, children) in marker
        .defaults
        .iter()
        .map(|entry| (entry.root, &entry.child_ids))
        .chain(
            marker
                .unique
                .iter()
                .map(|entry| (entry.root, &entry.child_ids)),
        )
    {
        referenced_ids.insert(root);
        // The frozen list keeps source construction order. Actual nested parents
        // are checked below against the same tree's accepted prefix.
        for child in children {
            referenced_ids.insert(*child);
        }
    }
    if referenced_ids.is_empty() {
        return Ok(());
    }
    let ids: Vec<i64> = referenced_ids.iter().map(|id| i64::from(*id)).collect();
    let rows = sqlx::query("SELECT o.item_id,o.container_id,substring(s.payload from 1 for 16) AS header FROM item_ownership o JOIN item_places p ON p.item_id=o.item_id AND p.kind=0 JOIN entity_snapshots s ON s.object_id=o.item_id WHERE o.item_id=ANY($1)")
        .bind(&ids)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() != ids.len() {
        return Err(StoreError::Invalid("vendor stock item missing"));
    }
    let parents: BTreeMap<u32, u32> = rows
        .into_iter()
        .map(|row| {
            let item = row.get::<i64, _>("item_id") as u32;
            let parent = row.get::<i64, _>("container_id") as u32;
            let header: Vec<u8> = row.get("header");
            if header.len() != 16
                || &header[..8] != b"ACERBIN\0"
                || u16::from_le_bytes([header[10], header[11]]) != 101
                || u16::from_le_bytes([header[12], header[13]]) != 5
            {
                return Err(StoreError::Invalid("vendor stock requires item V5"));
            }
            Ok((item, parent))
        })
        .collect::<Result<_, _>>()?;
    for (root, children) in marker
        .defaults
        .iter()
        .map(|entry| (entry.root, &entry.child_ids))
        .chain(
            marker
                .unique
                .iter()
                .map(|entry| (entry.root, &entry.child_ids)),
        )
    {
        if parents.get(&root) != Some(&marker.vendor_object_id) {
            return Err(StoreError::Invalid("vendor stock root placement"));
        }
        let mut accepted = BTreeSet::from([root]);
        for child in children {
            let parent = parents
                .get(child)
                .ok_or(StoreError::Invalid("vendor stock child placement"))?;
            if !accepted.contains(parent) {
                return Err(StoreError::Invalid("vendor stock child order or parent"));
            }
            accepted.insert(*child);
        }
    }
    Ok(())
}
