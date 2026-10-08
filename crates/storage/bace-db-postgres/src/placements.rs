//! Receipt-backed placement and housing transitions. World/contained/deleted are
//! distinct durable states; row changes and aggregate snapshots commit together.
use crate::{PgStore, StoreError};
use bace_persistence::{
    DurableItemPlace as Place, HouseOwnershipChange, HousingOperation, OperationOutcome,
    OwnershipState, PlacementOperation, VendorStockWrite,
};
use bace_storage_codec::{ItemPlacementV2, ItemSaveV2};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeSet;

#[derive(Default)]
struct PlacementExtensions<'a> {
    vendor: Option<(&'a VendorStockWrite, i64)>,
    promotion: Option<&'a [u32]>,
}
impl PgStore {
    pub async fn item_place(&self, item: u32) -> Result<Option<Place>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let value = read_place(&mut tx, item).await?;
        tx.rollback().await?;
        Ok(value)
    }
    pub async fn placement_operation(
        &self,
        operation: &PlacementOperation,
    ) -> Result<OperationOutcome, StoreError> {
        execute(self, operation, None, None, None, None).await
    }
    pub async fn world_placement_operation(
        &self,
        operation: &bace_persistence::WorldPlacementOperation,
    ) -> Result<OperationOutcome, StoreError> {
        execute(
            self,
            &operation.inventory,
            None,
            None,
            Some(operation.world_epoch),
            None,
        )
        .await
    }
    pub async fn housing_operation(
        &self,
        operation: &HousingOperation,
    ) -> Result<OperationOutcome, StoreError> {
        execute(
            self,
            &operation.inventory,
            Some(operation.ownership),
            None,
            None,
            None,
        )
        .await
    }
}
fn place_hash(hash: &mut Sha256, place: Option<Place>) {
    match place {
        None => hash.update([0]),
        Some(Place::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        }) => {
            hash.update([1]);
            hash.update(container.to_le_bytes());
            hash.update(slot.to_le_bytes());
            hash.update([u8::from(pack_slot)]);
            hash.update(equipped.to_le_bytes());
        }
        Some(Place::World { cell }) => {
            hash.update([2]);
            hash.update(cell.to_le_bytes());
        }
        Some(Place::Removed) => hash.update([3]),
    }
}
fn fingerprint(operation: &PlacementOperation, house: Option<HouseOwnershipChange>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"betterace-placement-operation-v2");
    let mut snapshots: Vec<_> = operation.snapshots.iter().collect();
    snapshots.sort_by_key(|s| s.object_id);
    for s in snapshots {
        h.update(s.object_id.to_le_bytes());
        h.update(s.mutation_revision.to_le_bytes());
        h.update(s.expected_version.to_le_bytes());
        h.update((s.bytes.len() as u64).to_le_bytes());
        h.update(&s.bytes);
    }
    let mut participants = operation.participants.clone();
    participants.sort_unstable();
    h.update((participants.len() as u64).to_le_bytes());
    for id in participants {
        h.update(id.to_le_bytes())
    }
    let mut leases = operation.leases.clone();
    leases.sort_by_key(|l| l.character_id);
    h.update((leases.len() as u64).to_le_bytes());
    for l in leases {
        h.update(l.character_id.to_le_bytes());
        h.update(l.epoch.to_le_bytes());
        h.update([u8::from(l.state == OwnershipState::Online)]);
    }
    let mut changes = operation.changes.clone();
    changes.sort_by_key(|c| c.item);
    h.update((changes.len() as u64).to_le_bytes());
    for c in changes {
        h.update(c.item.to_le_bytes());
        place_hash(&mut h, c.expected);
        place_hash(&mut h, Some(c.destination));
    }
    let mut views = operation.storage_views.clone();
    views.sort_by_key(|v| v.house);
    h.update((views.len() as u64).to_le_bytes());
    for v in views {
        h.update(v.actor.to_le_bytes());
        h.update(v.house.to_le_bytes());
        h.update(v.generation.to_le_bytes());
    }
    if let Some(v) = house {
        h.update([1]);
        h.update(v.house.to_le_bytes());
        h.update(v.house_id.to_le_bytes());
        h.update(v.expected_owner.unwrap_or(0).to_le_bytes());
        h.update(v.expected_generation.to_le_bytes());
        h.update(v.owner.unwrap_or(0).to_le_bytes());
        h.update(v.generation.to_le_bytes());
    } else {
        h.update([0]);
    }
    h.finalize().into()
}
pub(crate) async fn execute(
    store: &PgStore,
    operation: &PlacementOperation,
    house: Option<HouseOwnershipChange>,
    workflow: Option<&bace_persistence::NpcWorkflowUpdate>,
    world_epoch: Option<u64>,
    allegiance: Option<&bace_persistence::AllegianceOperation>,
) -> Result<OperationOutcome, StoreError> {
    execute_inner(
        store,
        operation,
        house,
        workflow,
        world_epoch,
        allegiance,
        PlacementExtensions::default(),
    )
    .await
}

pub(crate) async fn execute_vendor(
    store: &PgStore,
    operation: &PlacementOperation,
    world_epoch: u64,
    vendor_expected_version: i64,
    marker: &VendorStockWrite,
) -> Result<OperationOutcome, StoreError> {
    execute_inner(
        store,
        operation,
        None,
        None,
        Some(world_epoch),
        None,
        PlacementExtensions {
            vendor: Some((marker, vendor_expected_version)),
            promotion: None,
        },
    )
    .await
}

pub(crate) async fn execute_constructed_promotion(
    store: &PgStore,
    operation: &PlacementOperation,
    world_epoch: u64,
    roots: &[u32],
) -> Result<OperationOutcome, StoreError> {
    execute_inner(
        store,
        operation,
        None,
        None,
        Some(world_epoch),
        None,
        PlacementExtensions {
            vendor: None,
            promotion: Some(roots),
        },
    )
    .await
}

async fn execute_inner(
    store: &PgStore,
    operation: &PlacementOperation,
    house: Option<HouseOwnershipChange>,
    workflow: Option<&bace_persistence::NpcWorkflowUpdate>,
    world_epoch: Option<u64>,
    allegiance: Option<&bace_persistence::AllegianceOperation>,
    extensions: PlacementExtensions<'_>,
) -> Result<OperationOutcome, StoreError> {
    let vendor = extensions.vendor;
    let promotion = extensions.promotion;
    if world_epoch.is_some_and(|epoch| epoch == 0 || epoch > i64::MAX as u64)
        || world_epoch.is_some() && workflow.is_some()
        || vendor.is_some()
            && (world_epoch.is_none()
                || house.is_some()
                || workflow.is_some()
                || allegiance.is_some()
                || promotion.is_some())
        || promotion.is_some()
            && (world_epoch.is_none()
                || house.is_some()
                || workflow.is_some()
                || allegiance.is_some())
    {
        return Err(StoreError::Invalid("world placement epoch"));
    }
    if !operation.snapshots.is_empty() || workflow.is_none() && vendor.is_none() {
        crate::writes::validate(&operation.snapshots)?;
    }
    if let Some(update) = workflow {
        let saved = crate::npc_workflow::validate(update)?;
        if !operation.participants.contains(&saved.source)
            || operation
                .snapshots
                .iter()
                .map(|s| s.bytes.len())
                .sum::<usize>()
                .checked_add(update.checkpoint.len())
                .is_none_or(|n| n > 64 * 1024 * 1024)
        {
            return Err(StoreError::Invalid("NPC stage participant/byte bounds"));
        }
    }
    if operation.operation_id.is_empty()
        || operation.operation_id.len() > 128
        || operation.participants.is_empty()
        || operation.participants.len() > 1024
        || operation.leases.len() > 1024
        || operation.changes.len() > 1024
        || operation.storage_views.len() > 64
    {
        return Err(StoreError::Invalid("placement operation bounds"));
    }
    let participants: BTreeSet<_> = operation.participants.iter().copied().collect();
    if participants.len() != operation.participants.len()
        || participants.contains(&0)
        || operation
            .snapshots
            .iter()
            .any(|s| !participants.contains(&s.object_id))
    {
        return Err(StoreError::Invalid("placement participants"));
    }
    validate_death_receipt_marker(operation, world_epoch)?;
    let vendor_marker = vendor
        .map(|(write, version)| {
            if version <= 0 {
                return Err(StoreError::Invalid("vendor source version"));
            }
            crate::vendor_stock::validate(write, operation)
        })
        .transpose()?;
    let fresh_creatures = crate::constructed_promotion::fresh_roots(operation)?;
    match promotion {
        Some(roots) => {
            crate::constructed_promotion::validate_request(operation, roots, &fresh_creatures)?;
        }
        None if !fresh_creatures.is_empty() => {
            return Err(StoreError::Invalid(
                "fresh constructed Creature/Cow requires promotion operation",
            ));
        }
        None => {}
    }
    let mut ids = BTreeSet::new();
    for c in &operation.changes {
        if !ids.insert(c.item)
            || !participants.contains(&c.item)
            || !operation.snapshots.iter().any(|s| s.object_id == c.item)
        {
            return Err(StoreError::Invalid("placement changed item"));
        }
        for p in c.expected.into_iter().chain(std::iter::once(c.destination)) {
            if let Place::Contained {
                container,
                pack_slot,
                equipped,
                ..
            } = p
                && (container == c.item
                    || !participants.contains(&container)
                    || pack_slot && equipped != 0)
            {
                return Err(StoreError::Invalid("placement container"));
            }
            if let Place::World { cell: 0 } = p {
                return Err(StoreError::Invalid("placement cell"));
            }
        }
    }
    let mut leased = BTreeSet::new();
    for l in &operation.leases {
        if !participants.contains(&l.character_id)
            || !leased.insert(l.character_id)
            || l.epoch < 0
            || !matches!(l.state, OwnershipState::Online | OwnershipState::Offline)
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    if let Some(h) = house {
        if !participants.contains(&h.house)
            || h.house_id == 0
            || h.generation == 0
            || h.generation > i64::MAX as u64
            || h.expected_generation > i64::MAX as u64
            || !operation.snapshots.iter().any(|s| s.object_id == h.house)
        {
            return Err(StoreError::Invalid("housing transition"));
        }
        for owner in [h.expected_owner, h.owner].into_iter().flatten() {
            if !participants.contains(&owner) || !leased.contains(&owner) {
                return Err(StoreError::OwnershipConflict);
            }
        }
        if h.expected_owner != h.owner
            && h.generation
                != h.expected_generation
                    .checked_add(1)
                    .ok_or(StoreError::OwnershipConflict)?
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    if let Some(patch) = allegiance {
        crate::allegiance::validate_ledger(patch, true)?;
        if patch.operation_id != operation.operation_id
            || !patch.players.is_empty()
            || patch
                .leases
                .iter()
                .any(|lease| !operation.leases.contains(lease))
        {
            return Err(StoreError::Invalid("composite allegiance participants"));
        }
        let size = operation
            .snapshots
            .iter()
            .map(|s| s.bytes.len())
            .sum::<usize>()
            .checked_add(
                patch
                    .nodes
                    .iter()
                    .chain(&patch.metadata)
                    .map(|w| w.bytes.as_ref().map_or(0, Vec::len))
                    .sum(),
            )
            .and_then(|n| n.checked_add(workflow.map_or(0, |w| w.checkpoint.len())))
            .ok_or(StoreError::Invalid("composite allegiance bytes"))?;
        if size > 64 * 1024 * 1024 {
            return Err(StoreError::Invalid("composite allegiance bytes"));
        }
    }
    let base_hash = fingerprint(operation, house);
    let base_hash = if let Some(patch) = allegiance {
        let mut h = Sha256::new();
        h.update(b"betterace-allegiance-placement-v1");
        h.update(base_hash);
        h.update(crate::allegiance::fingerprint(patch));
        <[u8; 32]>::from(h.finalize())
    } else {
        base_hash
    };
    let hash = if let Some(update) = workflow {
        let mut h = Sha256::new();
        h.update(b"betterace-npc-stage-v1");
        h.update(base_hash);
        h.update(update.invocation);
        h.update(update.world_epoch.to_le_bytes());
        h.update(update.expected_version.to_le_bytes());
        h.update((update.checkpoint.len() as u64).to_le_bytes());
        h.update(&update.checkpoint);
        <[u8; 32]>::from(h.finalize())
    } else if let Some(epoch) = world_epoch {
        let mut h = Sha256::new();
        h.update(b"betterace-world-placement-v1");
        h.update(base_hash);
        h.update(epoch.to_le_bytes());
        <[u8; 32]>::from(h.finalize())
    } else {
        base_hash
    };
    let hash = if let Some((write, vendor_expected_version)) = vendor {
        let mut h = Sha256::new();
        h.update(b"betterace-vendor-stock-placement-v1");
        h.update(hash);
        h.update(vendor_expected_version.to_le_bytes());
        h.update(write.marker_object_id.to_le_bytes());
        h.update(write.expected_version.to_le_bytes());
        h.update(write.expected_stock_revision.to_le_bytes());
        h.update(write.mutation_revision.to_le_bytes());
        h.update((write.bytes.len() as u64).to_le_bytes());
        h.update(&write.bytes);
        <[u8; 32]>::from(h.finalize())
    } else {
        hash
    };
    let hash = if let Some(roots) = promotion {
        let mut h = Sha256::new();
        h.update(b"betterace-constructed-creature-promotion-v1");
        h.update(hash);
        h.update((roots.len() as u64).to_le_bytes());
        for root in roots {
            h.update(root.to_le_bytes());
        }
        <[u8; 32]>::from(h.finalize())
    } else {
        hash
    };
    let mut tx = store.pool.begin().await?;
    let inserted=sqlx::query("INSERT INTO durable_operations(operation_id,request_fingerprint) VALUES($1,$2) ON CONFLICT DO NOTHING").bind(&operation.operation_id).bind(hash.as_slice()).execute(&mut *tx).await?.rows_affected();
    if inserted == 0 {
        let prior: Vec<u8> = sqlx::query_scalar(
            "SELECT request_fingerprint FROM durable_operations WHERE operation_id=$1",
        )
        .bind(&operation.operation_id)
        .fetch_one(&mut *tx)
        .await?;
        if prior != hash {
            return Err(StoreError::OperationMismatch);
        }
        tx.commit().await.map_err(crate::store::commit_error)?;
        return Ok(OperationOutcome::AlreadyCommitted);
    }
    if let Some(expected_epoch) = world_epoch.or_else(|| workflow.map(|update| update.world_epoch))
    {
        let epoch: i64 =
            sqlx::query_scalar("SELECT epoch FROM world_execution_epoch WHERE singleton FOR SHARE")
                .fetch_one(&mut *tx)
                .await?;
        if epoch as u64 != expected_epoch {
            return Err(StoreError::OwnershipConflict);
        }
    }
    sqlx::query("SELECT pg_advisory_xact_lock(8589934591)")
        .execute(&mut *tx)
        .await?;
    if allegiance.is_some() {
        sqlx::query("SELECT pg_advisory_xact_lock(8589934589)")
            .execute(&mut *tx)
            .await?;
    }
    for id in &participants {
        crate::ownership::lock_object(&mut tx, *id).await?;
    }
    let marker_collision: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vendor_stock_markers WHERE marker_id=ANY($1))",
    )
    .bind(
        participants
            .iter()
            .map(|id| i64::from(*id))
            .collect::<Vec<_>>(),
    )
    .fetch_one(&mut *tx)
    .await?;
    if marker_collision && vendor.is_none() {
        return Err(StoreError::OwnershipConflict);
    }
    if let (Some((_, expected_version)), Some(marker)) = (vendor, vendor_marker.as_ref()) {
        let actual: Option<i64> =
            sqlx::query_scalar("SELECT version FROM entity_snapshots WHERE object_id=$1")
                .bind(i64::from(marker.vendor_object_id))
                .fetch_optional(&mut *tx)
                .await?;
        if actual != Some(expected_version) {
            return Err(StoreError::Conflict(marker.vendor_object_id));
        }
    }
    let id_list: Vec<_> = participants.iter().map(|id| i64::from(*id)).collect();
    let chars: Vec<i64> = sqlx::query_scalar(
        "SELECT character_id FROM character_ownership WHERE character_id=ANY($1)",
    )
    .bind(&id_list)
    .fetch_all(&mut *tx)
    .await?;
    if chars
        .into_iter()
        .map(|id| id as u32)
        .collect::<BTreeSet<_>>()
        != leased
    {
        return Err(StoreError::OwnershipConflict);
    }
    for l in &operation.leases {
        crate::ownership::check_lease(&mut tx, *l).await?;
    }
    crate::inventory::check_ancestries(&mut tx, &participants).await?;
    for v in &operation.storage_views {
        if !participants.contains(&v.house)
            || !leased.contains(&v.actor)
            || v.generation > i64::MAX as u64
        {
            return Err(StoreError::Invalid("storage view participant"));
        }
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM house_ownership WHERE object_id=$1 AND owner_id IS NOT NULL AND access_generation=$2)").bind(i64::from(v.house)).bind(v.generation as i64).fetch_one(&mut *tx).await?;
        if !allowed {
            return Err(StoreError::OwnershipConflict);
        }
        let row=sqlx::query("SELECT e.payload,h.owner_id FROM entity_snapshots e JOIN house_ownership h ON h.object_id=e.object_id WHERE h.object_id=$1").bind(i64::from(v.house)).fetch_one(&mut *tx).await?;
        let owner = row
            .get::<Option<i64>, _>("owner_id")
            .ok_or(StoreError::OwnershipConflict)?;
        let same_account:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM players p JOIN players o ON o.account_id=p.account_id WHERE p.object_id=$1 AND o.object_id=$2)").bind(i64::from(v.actor)).bind(owner).fetch_one(&mut *tx).await?;
        let state =
            bace_storage_codec::HouseSaveV3::decode_migrate(&row.get::<Vec<u8>, _>("payload"))
                .map_err(|_| StoreError::Invalid("house access payload"))?;
        if !same_account
            && !state
                .access
                .iter()
                .any(|a| a.player_id == v.actor && a.permissions & 2 != 0)
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    // Any source/destination housing ancestry requires a current server-validated
    // storage view; omission must not bypass an unowned or revoked house.
    let houses: Vec<i64> =
        sqlx::query_scalar("SELECT object_id FROM house_ownership WHERE object_id=ANY($1)")
            .bind(&id_list)
            .fetch_all(&mut *tx)
            .await?;
    for house_id in houses {
        let house_id = house_id as u32;
        for c in &operation.changes {
            let in_house = descends_from(&mut tx, c.item, house_id).await?
                || if let Place::Contained { container, .. } = c.destination {
                    descends_from(&mut tx, container, house_id).await?
                } else {
                    false
                };
            if in_house && !operation.storage_views.iter().any(|v| v.house == house_id) {
                return Err(StoreError::OwnershipConflict);
            }
        }
    }
    for c in &operation.changes {
        if leased.contains(&c.item) {
            return Err(StoreError::Invalid("character placement"));
        }
        if read_place(&mut tx, c.item).await? != c.expected {
            return Err(StoreError::Conflict(c.item));
        }
        let snapshot = operation
            .snapshots
            .iter()
            .find(|s| s.object_id == c.item)
            .expect("validated snapshot");
        let (id, placement) = decode_placed_snapshot(&snapshot.bytes)?;
        if id != c.item || dto_place(&placement) != c.destination {
            return Err(StoreError::Invalid("item placement payload mismatch"));
        }
    }
    // Retained house contents cannot be moved as a side effect of ownership change.
    if let Some(h) = house {
        if h.expected_owner != h.owner {
            for c in &operation.changes {
                if descends_from(&mut tx, c.item, h.house).await? {
                    return Err(StoreError::Invalid(
                        "housing ownership change must retain contents",
                    ));
                }
            }
        }
        update_house(&mut tx, h).await?;
    }
    // Relational rows are changed first within the transaction. The final snapshot
    // identity gate observes the proposed state; any CAS failure rolls all of it back.
    for c in &operation.changes {
        write_place(&mut tx, c.item, c.destination).await?;
    }
    let removed: Vec<i64> = operation
        .changes
        .iter()
        .filter(|c| c.destination == Place::Removed)
        .map(|c| i64::from(c.item))
        .collect();
    let orphaned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM item_ownership WHERE container_id=ANY($1))",
    )
    .bind(removed)
    .fetch_one(&mut *tx)
    .await?;
    if orphaned {
        return Err(StoreError::Invalid("cannot remove a nonempty container"));
    }
    crate::inventory::check_ancestries(&mut tx, &participants).await?;
    if let Some(update) = workflow {
        crate::npc_workflow::commit(&mut tx, update).await?;
    }
    if let Some(patch) = allegiance {
        crate::allegiance::apply_patch(&mut tx, patch).await?;
    }
    let mut acks =
        crate::writes::write_world_death_unchecked(&mut tx, &operation.snapshots).await?;
    if let Some(roots) = promotion {
        crate::constructed_promotion::validate_committed_forest(&mut tx, operation, roots).await?;
    }
    if let (Some((write, _)), Some(marker)) = (vendor, vendor_marker.as_ref()) {
        crate::vendor_stock::validate_stock_forest(&mut tx, marker).await?;
        acks.push(crate::vendor_stock::apply(&mut tx, write, marker).await?);
    }
    if let Some(update) = workflow {
        crate::npc_workflow::validate_committed_inventory(&mut tx, update).await?;
    }
    tx.commit().await.map_err(crate::store::commit_error)?;
    Ok(OperationOutcome::Committed(acks))
}
fn validate_death_receipt_marker(
    operation: &PlacementOperation,
    world_epoch: Option<u64>,
) -> Result<(), StoreError> {
    let mut seen = false;
    for snapshot in &operation.snapshots {
        if snapshot.bytes.len() < 12
            || !snapshot.bytes.starts_with(b"ACERBIN\0")
            || u16::from_le_bytes([snapshot.bytes[10], snapshot.bytes[11]])
                != bace_storage_codec::PVE_DEATH_RECEIPT_KIND
        {
            continue;
        }
        if seen || snapshot.expected_version != 0 || snapshot.mutation_revision != 1 {
            return Err(StoreError::Invalid("PVE death marker CAS"));
        }
        seen = true;
        let marker = bace_storage_codec::PveDeathReceiptV1::decode(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("PVE death marker payload"))?;
        if world_epoch != Some(marker.world_epoch)
            || marker.marker_object_id != snapshot.object_id
            || operation
                .changes
                .iter()
                .any(|change| change.item == snapshot.object_id)
            || ![
                format!("native-death-{:032x}", u128::from_le_bytes(marker.event_id)),
                format!(
                    "native-no-corpse-{:032x}",
                    u128::from_le_bytes(marker.event_id)
                ),
            ]
            .contains(&operation.operation_id)
        {
            return Err(StoreError::Invalid("PVE death marker identity"));
        }
    }
    Ok(())
}
pub(crate) fn dto_place(place: &ItemPlacementV2) -> Place {
    match place {
        ItemPlacementV2::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } => Place::Contained {
            container: *container,
            slot: *slot,
            pack_slot: *pack_slot,
            equipped: *equipped,
        },
        ItemPlacementV2::World(p) => Place::World {
            cell: p.obj_cell_id,
        },
        ItemPlacementV2::Removed => Place::Removed,
    }
}
pub(crate) async fn read_place(
    tx: &mut Transaction<'_, Postgres>,
    item: u32,
) -> Result<Option<Place>, StoreError> {
    let row=sqlx::query("SELECT p.kind,p.cell_id,o.container_id,o.slot,o.pack_slot,o.equipped FROM item_places p FULL JOIN item_ownership o ON o.item_id=p.item_id WHERE COALESCE(p.item_id,o.item_id)=$1").bind(i64::from(item)).fetch_optional(&mut **tx).await?;
    row.map(|r| match r.get::<Option<i16>, _>("kind") {
        Some(1) => Ok(Place::World {
            cell: u32::try_from(r.get::<i64, _>("cell_id"))
                .map_err(|_| StoreError::Invalid("world item cell"))?,
        }),
        Some(2) => Ok(Place::Removed),
        _ => Ok(Place::Contained {
            container: r.get::<i64, _>("container_id") as u32,
            slot: r.get::<i64, _>("slot") as u32,
            pack_slot: r.get("pack_slot"),
            equipped: r.get::<i64, _>("equipped") as u32,
        }),
    })
    .transpose()
}
async fn write_place(
    tx: &mut Transaction<'_, Postgres>,
    item: u32,
    place: Place,
) -> Result<(), StoreError> {
    let (kind, cell) = match place {
        Place::Contained {
            container,
            slot,
            pack_slot,
            equipped,
        } => {
            sqlx::query("INSERT INTO item_ownership(item_id,container_id,slot,pack_slot,equipped) VALUES($1,$2,$3,$4,$5) ON CONFLICT(item_id) DO UPDATE SET container_id=EXCLUDED.container_id,slot=EXCLUDED.slot,pack_slot=EXCLUDED.pack_slot,equipped=EXCLUDED.equipped").bind(i64::from(item)).bind(i64::from(container)).bind(i64::from(slot)).bind(pack_slot).bind(i64::from(equipped)).execute(&mut **tx).await?;
            (0i16, None)
        }
        Place::World { cell } => {
            sqlx::query("DELETE FROM item_ownership WHERE item_id=$1")
                .bind(i64::from(item))
                .execute(&mut **tx)
                .await?;
            (1, Some(i64::from(cell)))
        }
        Place::Removed => {
            sqlx::query("DELETE FROM item_ownership WHERE item_id=$1")
                .bind(i64::from(item))
                .execute(&mut **tx)
                .await?;
            (2, None)
        }
    };
    sqlx::query("INSERT INTO item_places(item_id,kind,cell_id) VALUES($1,$2,$3) ON CONFLICT(item_id) DO UPDATE SET kind=EXCLUDED.kind,cell_id=EXCLUDED.cell_id").bind(i64::from(item)).bind(kind).bind(cell).execute(&mut **tx).await?;
    Ok(())
}
async fn descends_from(
    tx: &mut Transaction<'_, Postgres>,
    item: u32,
    house: u32,
) -> Result<bool, StoreError> {
    let mut id = item;
    for _ in 0..64 {
        if id == house {
            return Ok(true);
        }
        match read_place(tx, id).await? {
            Some(Place::Contained { container, .. }) => id = container,
            _ => return Ok(false),
        }
    }
    Err(StoreError::Invalid("housing ancestry depth"))
}
async fn update_house(
    tx: &mut Transaction<'_, Postgres>,
    h: HouseOwnershipChange,
) -> Result<(), StoreError> {
    let row = sqlx::query(
        "SELECT house_id,owner_id,access_generation FROM house_ownership WHERE object_id=$1",
    )
    .bind(i64::from(h.house))
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => {
            if r.get::<i64, _>("house_id") != i64::from(h.house_id)
                || r.get::<Option<i64>, _>("owner_id") != h.expected_owner.map(i64::from)
                || r.get::<i64, _>("access_generation") != h.expected_generation as i64
            {
                return Err(StoreError::OwnershipConflict);
            }
            sqlx::query(
                "UPDATE house_ownership SET owner_id=$2,access_generation=$3 WHERE object_id=$1",
            )
            .bind(i64::from(h.house))
            .bind(h.owner.map(i64::from))
            .bind(h.generation as i64)
            .execute(&mut **tx)
            .await?;
        }
        None => {
            if h.expected_owner.is_some() || h.expected_generation != 0 {
                return Err(StoreError::OwnershipConflict);
            }
            sqlx::query("INSERT INTO house_ownership(object_id,house_id,owner_id,access_generation) VALUES($1,$2,$3,$4)").bind(i64::from(h.house)).bind(i64::from(h.house_id)).bind(h.owner.map(i64::from)).bind(h.generation as i64).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

pub(crate) fn decode_placed_snapshot(bytes: &[u8]) -> Result<(u32, ItemPlacementV2), StoreError> {
    let info = bace_storage_codec::inspect(
        bytes,
        bace_storage_codec::CodecLimits {
            max_payload_bytes: 2 * 1024 * 1024,
        },
    )
    .map_err(|_| StoreError::Invalid("placed snapshot envelope"))?;
    match (info.kind, info.schema_version) {
        (101, 5) => {
            let value = bace_storage_codec::ItemSaveV5::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed item V5 snapshot"))?;
            Ok((
                value.entity.object_id,
                value.previous.previous.previous.placement,
            ))
        }
        (101, 4) => {
            let value = bace_storage_codec::ItemSaveV4::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed item V4 snapshot"))?;
            Ok((value.entity.object_id, value.previous.previous.placement))
        }
        (101, 3) => {
            let value = bace_storage_codec::ItemSaveV3::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed item V3 snapshot"))?;
            Ok((value.entity.object_id, value.previous.placement))
        }
        (102, 5) => {
            let value = bace_storage_codec::CorpseSaveV5::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed corpse V5 snapshot"))?;
            Ok((
                value.corpse.entity.object_id,
                value.previous.previous.previous.placement,
            ))
        }
        (102, 4) => {
            let value = bace_storage_codec::CorpseSaveV4::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed corpse V4 snapshot"))?;
            Ok((
                value.corpse.entity.object_id,
                value.previous.previous.placement,
            ))
        }
        (102, 3) => {
            let value = bace_storage_codec::CorpseSaveV3::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed corpse V3 snapshot"))?;
            Ok((value.corpse.entity.object_id, value.previous.placement))
        }
        (101, 2) => {
            let value = ItemSaveV2::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed item V2 snapshot"))?;
            Ok((value.entity.object_id, value.placement))
        }
        (102, 2) => {
            let value = bace_storage_codec::CorpseSaveV2::decode(bytes)
                .map_err(|_| StoreError::Invalid("placed corpse V2 snapshot"))?;
            Ok((value.corpse.entity.object_id, value.placement))
        }
        _ => Err(StoreError::Invalid(
            "placed snapshot needs explicit item/corpse V2 migration",
        )),
    }
}

#[cfg(test)]
mod death_marker_tests {
    use super::*;
    use bace_persistence::{PlacementChange, SaveSnapshot};

    fn operation() -> PlacementOperation {
        let event_id = [7; 16];
        let marker = bace_storage_codec::PveDeathReceiptV1 {
            marker_object_id: 0x8000_0010,
            event_id,
            victim_object_id: 0x5000_0001,
            position: bace_content::Position {
                obj_cell_id: 0x1234_0100,
                position_x: 1.,
                position_y: 2.,
                position_z: 3.,
                rotation_w: 1.,
                rotation_x: 0.,
                rotation_y: 0.,
                rotation_z: 0.,
            },
            world_epoch: 9,
            killed: true,
        };
        PlacementOperation {
            operation_id: format!("native-no-corpse-{:032x}", u128::from_le_bytes(event_id)),
            snapshots: vec![SaveSnapshot {
                object_id: marker.marker_object_id,
                mutation_revision: 1,
                expected_version: 0,
                bytes: marker.encode().unwrap(),
            }],
            participants: vec![marker.marker_object_id],
            leases: vec![],
            changes: vec![],
            storage_views: vec![],
        }
    }

    #[test]
    fn unplaced_zero_drop_death_requires_exact_world_fence_and_identity() {
        let valid = operation();
        assert!(validate_death_receipt_marker(&valid, Some(9)).is_ok());
        assert!(validate_death_receipt_marker(&valid, Some(8)).is_err());
        assert!(validate_death_receipt_marker(&valid, None).is_err());
        let mut mismatched = valid.clone();
        mismatched.operation_id = "native-no-corpse-other".into();
        assert!(validate_death_receipt_marker(&mismatched, Some(9)).is_err());
        let mut placed = valid;
        placed.changes.push(PlacementChange {
            item: 0x8000_0010,
            expected: None,
            destination: Place::World { cell: 0x1234_0100 },
        });
        assert!(validate_death_receipt_marker(&placed, Some(9)).is_err());
    }
}
