//! Exact first durable receipt for constructed Creature/Cow forests. The
//! database verifies the committed item graph before the operation can succeed.
use crate::{PgStore, StoreError};
use bace_persistence::{
    ConstructedCreaturePromotionOperation, OperationOutcome, PlacementOperation, SaveSnapshot,
};
use bace_storage_codec::{CodecLimits, ItemSaveV5};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};

impl PgStore {
    pub async fn constructed_creature_promotion(
        &self,
        operation: &ConstructedCreaturePromotionOperation,
    ) -> Result<OperationOutcome, StoreError> {
        crate::placements::execute_constructed_promotion(
            self,
            &operation.inventory,
            operation.world_epoch,
            &operation.creature_roots,
        )
        .await
    }
}

pub(crate) fn fresh_roots(operation: &PlacementOperation) -> Result<BTreeSet<u32>, StoreError> {
    let mut roots = BTreeSet::new();
    for snapshot in &operation.snapshots {
        if snapshot.expected_version != 0
            || snapshot.bytes.len() < 16
            || !snapshot.bytes.starts_with(b"ACERBIN\0")
            || u16::from_le_bytes([snapshot.bytes[10], snapshot.bytes[11]]) != 101
            || u16::from_le_bytes([snapshot.bytes[12], snapshot.bytes[13]]) != 5
        {
            continue;
        }
        let info = bace_storage_codec::inspect(
            &snapshot.bytes,
            CodecLimits {
                max_payload_bytes: 17 * 1024 * 1024,
            },
        )
        .map_err(|_| StoreError::Invalid("promotion snapshot envelope"))?;
        if (info.kind, info.schema_version) != (101, 5) {
            continue;
        }
        let item = ItemSaveV5::decode(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("promotion item V5"))?;
        if item.construction.is_some() {
            if item.entity.object_id != snapshot.object_id {
                return Err(StoreError::Invalid("promotion creature identity"));
            }
            roots.insert(snapshot.object_id);
        }
    }
    Ok(roots)
}

pub(crate) fn validate_request(
    operation: &PlacementOperation,
    roots: &[u32],
    fresh: &BTreeSet<u32>,
) -> Result<(), StoreError> {
    if roots.is_empty() || roots.len() > 128 || roots.contains(&0) {
        return Err(StoreError::Invalid("promotion creature roots"));
    }
    let requested: BTreeSet<_> = roots.iter().copied().collect();
    if requested.len() != roots.len() || &requested != fresh {
        return Err(StoreError::Invalid("promotion root set"));
    }
    for root in roots {
        if !operation.changes.iter().any(|change| {
            change.item == *root
                && change.expected.is_none()
                && change.destination != bace_persistence::DurableItemPlace::Removed
        }) {
            return Err(StoreError::Invalid("promotion root fresh placement"));
        }
    }
    Ok(())
}

pub(crate) async fn validate_committed_forest(
    tx: &mut Transaction<'_, Postgres>,
    operation: &PlacementOperation,
    roots: &[u32],
) -> Result<(), StoreError> {
    let snapshots: BTreeMap<u32, &SaveSnapshot> = operation
        .snapshots
        .iter()
        .map(|snapshot| (snapshot.object_id, snapshot))
        .collect();
    let changes: BTreeSet<u32> = operation
        .changes
        .iter()
        .filter(|change| change.expected.is_none())
        .map(|change| change.item)
        .collect();
    let root_set: BTreeSet<u32> = roots.iter().copied().collect();
    let mut frontier: Vec<i64> = roots.iter().map(|id| i64::from(*id)).collect();
    let mut seen = root_set.clone();
    let mut members = BTreeMap::<u32, (u32, u32)>::new();
    for _ in 0..64 {
        if frontier.is_empty() {
            break;
        }
        let rows = sqlx::query("SELECT item_id,container_id,equipped FROM item_ownership WHERE container_id=ANY($1) ORDER BY item_id LIMIT 1025")
            .bind(&frontier)
            .fetch_all(&mut **tx)
            .await?;
        if rows.len() > 1024 {
            return Err(StoreError::Invalid("promotion forest capacity"));
        }
        frontier.clear();
        for row in rows {
            let id = u32::try_from(row.get::<i64, _>("item_id"))
                .map_err(|_| StoreError::Invalid("promotion child id"))?;
            let parent = u32::try_from(row.get::<i64, _>("container_id"))
                .map_err(|_| StoreError::Invalid("promotion parent id"))?;
            let equipped = u32::try_from(row.get::<i64, _>("equipped"))
                .map_err(|_| StoreError::Invalid("promotion equipped location"))?;
            let snapshot = snapshots
                .get(&id)
                .ok_or(StoreError::Invalid("promotion child snapshot missing"))?;
            let item = ItemSaveV5::decode(&snapshot.bytes)
                .map_err(|_| StoreError::Invalid("promotion child requires item V5"))?;
            if snapshot.expected_version != 0
                || !changes.contains(&id)
                || item.entity.object_id != id
                || !matches!(item.previous.previous.previous.placement, bace_storage_codec::ItemPlacementV2::Contained { container, equipped: location, .. } if container == parent && location == equipped)
                || members.insert(id, (parent, equipped)).is_some()
            {
                return Err(StoreError::Invalid("promotion child receipt or placement"));
            }
            if seen.insert(id) {
                frontier.push(i64::from(id));
            }
        }
        if members.len() > 1024 {
            return Err(StoreError::Invalid("promotion forest capacity"));
        }
    }
    if !frontier.is_empty() {
        return Err(StoreError::Invalid("promotion forest depth"));
    }
    for root in roots {
        let snapshot = snapshots
            .get(root)
            .ok_or(StoreError::Invalid("promotion root snapshot missing"))?;
        let item = ItemSaveV5::decode(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("promotion root requires item V5"))?;
        let construction = item
            .construction
            .as_ref()
            .ok_or(StoreError::Invalid("promotion construction missing"))?;
        let owned: BTreeSet<u32> = members
            .keys()
            .copied()
            .filter(|id| belongs_to(*id, *root, &root_set, &members))
            .collect();
        if owned.len() > 1023 {
            return Err(StoreError::Invalid("promotion creature capacity"));
        }
        let actual_equipment: BTreeSet<_> = owned
            .iter()
            .copied()
            .filter(|id| {
                members
                    .get(id)
                    .is_some_and(|(parent, equipped)| *parent == *root && *equipped != 0)
            })
            .collect();
        let ordered_equipment: BTreeSet<_> = construction.equipment_order.iter().copied().collect();
        let misplaced_equipment = owned.iter().any(|id| {
            members
                .get(id)
                .is_some_and(|(parent, equipped)| *parent != *root && *equipped != 0)
        });
        if actual_equipment != ordered_equipment
            || misplaced_equipment
            || construction.death_roster.iter().any(|row| {
                !owned.contains(&row.entity)
                    || members.get(&row.entity).map(|(parent, _)| *parent)
                        != Some(row.parent.unwrap_or(*root))
            })
        {
            return Err(StoreError::Invalid(
                "promotion equipment or death roster graph",
            ));
        }
    }
    Ok(())
}

fn belongs_to(
    id: u32,
    root: u32,
    roots: &BTreeSet<u32>,
    members: &BTreeMap<u32, (u32, u32)>,
) -> bool {
    let mut current = id;
    for _ in 0..64 {
        if current == root {
            return true;
        }
        if roots.contains(&current) {
            return false;
        }
        let Some((parent, _)) = members.get(&current) else {
            return false;
        };
        current = *parent;
    }
    false
}
