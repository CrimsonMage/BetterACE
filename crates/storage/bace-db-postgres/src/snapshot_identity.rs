//! Final identity gate shared by every aggregate write path. Callers acquire
//! sorted object/owner locks before this check; no snapshot is written until the
//! complete batch agrees with existing relational player and house identities.
use crate::StoreError;
use bace_persistence::SaveSnapshot;
use bace_storage_codec::{HouseSaveV3, PVE_DEATH_RECEIPT_KIND, PlayerSaveV6, PveDeathReceiptV1};
use sqlx::{Postgres, Row, Transaction};
use std::collections::BTreeMap;

pub(crate) async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    snapshots: &[SaveSnapshot],
) -> Result<(), StoreError> {
    let snapshots: BTreeMap<_, _> = snapshots.iter().map(|s| (s.object_id, s)).collect();
    let ids: Vec<_> = snapshots.keys().map(|id| i64::from(*id)).collect();
    let vendor_marker_collision: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vendor_stock_markers WHERE marker_id=ANY($1))",
    )
    .bind(&ids)
    .fetch_one(&mut **tx)
    .await?;
    if vendor_marker_collision {
        return Err(StoreError::OwnershipConflict);
    }
    let mut death_markers = Vec::new();
    for snapshot in snapshots.values() {
        if snapshot.bytes.len() < 12
            || !snapshot.bytes.starts_with(b"ACERBIN\0")
            || u16::from_le_bytes([snapshot.bytes[10], snapshot.bytes[11]])
                != PVE_DEATH_RECEIPT_KIND
        {
            continue;
        }
        let marker = PveDeathReceiptV1::decode(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("PVE death marker payload"))?;
        if marker.marker_object_id != snapshot.object_id
            || snapshot.expected_version != 0
            || snapshot.mutation_revision != 1
        {
            return Err(StoreError::Invalid("PVE death marker identity"));
        }
        death_markers.push(i64::from(snapshot.object_id));
    }
    if !death_markers.is_empty() {
        let prior: Option<i64> = sqlx::query_scalar(
            "SELECT object_id FROM entity_snapshots WHERE object_id=ANY($1) LIMIT 1",
        )
        .bind(&death_markers)
        .fetch_optional(&mut **tx)
        .await?;
        if prior.is_some() {
            return Err(StoreError::OwnershipConflict);
        }
        let registered: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM players WHERE object_id=ANY($1) UNION ALL SELECT 1 FROM character_ownership WHERE character_id=ANY($1) UNION ALL SELECT 1 FROM house_ownership WHERE object_id=ANY($1) UNION ALL SELECT 1 FROM item_places WHERE item_id=ANY($1) UNION ALL SELECT 1 FROM item_ownership WHERE item_id=ANY($1) UNION ALL SELECT 1 FROM npc_source_heads WHERE source_id=ANY($1))",
        )
        .bind(&death_markers)
        .fetch_one(&mut **tx)
        .await?;
        if registered {
            return Err(StoreError::OwnershipConflict);
        }
    }
    // Read only bounded headers from prior registered gameplay records. A small
    // replacement cannot force loading the entire old world or erase V2 fields.
    let prior=sqlx::query("SELECT object_id,substring(payload from 1 for 16) AS header FROM entity_snapshots s WHERE object_id=ANY($1) AND (EXISTS(SELECT 1 FROM players p WHERE p.object_id=s.object_id) OR EXISTS(SELECT 1 FROM house_ownership h WHERE h.object_id=s.object_id) OR EXISTS(SELECT 1 FROM item_places i WHERE i.item_id=s.object_id))").bind(&ids).fetch_all(&mut **tx).await?;
    let mut corpse_ids = Vec::new();
    let mut item_identity_ids = Vec::new();
    for row in prior {
        let id = row.get::<i64, _>("object_id") as u32;
        let header: Vec<u8> = row.get("header");
        if header.len() == 16 && &header[..8] == b"ACERBIN\0" {
            let kind = u16::from_le_bytes([header[10], header[11]]);
            let schema = u16::from_le_bytes([header[12], header[13]]);
            if kind == 101 {
                item_identity_ids.push(i64::from(id));
            }
            if kind == 102 {
                corpse_ids.push(i64::from(id));
            }
            if (100..=103).contains(&kind) {
                let snapshot = snapshots
                    .get(&id)
                    .ok_or(StoreError::Invalid("missing registered snapshot"))?;
                let next = bace_storage_codec::inspect(
                    &snapshot.bytes,
                    bace_storage_codec::CodecLimits {
                        max_payload_bytes: 17 * 1024 * 1024,
                    },
                )
                .map_err(|_| StoreError::Invalid("registered gameplay snapshot envelope"))?;
                if next.kind != kind || next.schema_version < schema {
                    return Err(StoreError::Invalid(
                        "gameplay snapshot kind change or schema downgrade",
                    ));
                }
            }
        }
    }
    let players =
        sqlx::query("SELECT p.object_id,p.account_id,p.name,octet_length(s.payload) AS payload_bytes FROM players p JOIN entity_snapshots s ON s.object_id=p.object_id WHERE p.object_id=ANY($1)")
            .bind(&ids)
            .fetch_all(&mut **tx)
            .await?;
    let mut total_prior = 0usize;
    for row in &players {
        let n = usize::try_from(row.get::<i32, _>("payload_bytes"))
            .map_err(|_| StoreError::Invalid("prior player bytes"))?;
        if n > 2 * 1024 * 1024 + 52 {
            return Err(StoreError::Invalid("prior player bytes"));
        }
        total_prior = total_prior
            .checked_add(n)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or(StoreError::Invalid("prior player batch bytes"))?;
    }
    let player_ids: Vec<i64> = players.iter().map(|r| r.get("object_id")).collect();
    let old_rows =
        sqlx::query("SELECT object_id,payload FROM entity_snapshots WHERE object_id=ANY($1)")
            .bind(&player_ids)
            .fetch_all(&mut **tx)
            .await?;
    let mut old_players: BTreeMap<u32, Vec<u8>> = old_rows
        .into_iter()
        .map(|r| (r.get::<i64, _>("object_id") as u32, r.get("payload")))
        .collect();
    let mut required_keys = std::collections::BTreeSet::new();
    for row in players {
        let id = u32::try_from(row.get::<i64, _>("object_id"))
            .map_err(|_| StoreError::Invalid("registered player identity"))?;
        let snapshot = snapshots
            .get(&id)
            .ok_or(StoreError::Invalid("missing player snapshot"))?;
        let player = PlayerSaveV6::decode_or_migrate(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("registered player save payload"))?;
        let old = PlayerSaveV6::decode_or_migrate(
            &old_players
                .remove(&id)
                .ok_or(StoreError::Invalid("missing prior player"))?,
        )
        .map_err(|_| StoreError::Invalid("prior player save payload"))?;
        bace_storage_codec::validate_recovery_transition(
            old.combat_recovery,
            player.combat_recovery,
        )
        .map_err(|_| StoreError::Invalid("cast recovery was cleared or rewound"))?;
        bace_storage_codec::validate_physical_recovery_transition(
            old.physical_recovery,
            player.physical_recovery,
            old.player.entity.mutation_revision != player.player.entity.mutation_revision,
        )
        .map_err(|_| StoreError::Invalid("physical recovery was cleared or rewound"))?;
        if let Some(before) = old.rares {
            let after = player
                .rares
                .ok_or(StoreError::Invalid("cannot erase character rare state"))?;
            if before.character != after.character
                || before.random_identity != after.random_identity
                || before.key_version != after.key_version
                || after.attempt_ordinal < before.attempt_ordinal
                || after.timer_ordinal < before.timer_ordinal
                || after.last_effective_time < before.last_effective_time
            {
                return Err(StoreError::Invalid(
                    "character rare identity or ordinal rewind",
                ));
            }
        }
        if let Some(rare) = player.rares {
            required_keys.insert(i64::from(rare.key_version));
        }
        if player.player.entity.object_id != id
            || player.player.account_id != row.get::<i64, _>("account_id") as u64
            || player.player.name != row.get::<String, _>("name")
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    let keys: Vec<i64> = required_keys.iter().copied().collect();
    let bound: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM random_key_fingerprints WHERE version=ANY($1)")
            .bind(keys)
            .fetch_all(&mut **tx)
            .await?;
    if bound.into_iter().collect::<std::collections::BTreeSet<_>>() != required_keys {
        return Err(StoreError::Invalid("character rare RNG key is not bound"));
    }
    let houses = sqlx::query(
        "SELECT object_id,owner_id,house_id,access_generation FROM house_ownership WHERE object_id=ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    for row in houses {
        let id = u32::try_from(row.get::<i64, _>("object_id"))
            .map_err(|_| StoreError::Invalid("registered house identity"))?;
        let snapshot = snapshots
            .get(&id)
            .ok_or(StoreError::Invalid("missing house snapshot"))?;
        let house = HouseSaveV3::decode_migrate(&snapshot.bytes)
            .map_err(|_| StoreError::Invalid("registered house save payload"))?;
        if house.entity.object_id != id
            || house.owner_id.map(i64::from) != row.get::<Option<i64>, _>("owner_id")
            || house.access_generation as i64 != row.get::<i64, _>("access_generation")
            || i64::from(house.house_id) != row.get::<i64, _>("house_id")
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    let item_ids: Vec<i64> =
        sqlx::query_scalar("SELECT item_id FROM item_places WHERE item_id=ANY($1)")
            .bind(&ids)
            .fetch_all(&mut **tx)
            .await?;
    for id in item_ids {
        let id = id as u32;
        let snapshot = snapshots
            .get(&id)
            .ok_or(StoreError::Invalid("missing located item snapshot"))?;
        let (embedded, placement) = crate::placements::decode_placed_snapshot(&snapshot.bytes)?;
        if embedded != id
            || Some(crate::placements::dto_place(&placement))
                != crate::placements::read_place(tx, id).await?
        {
            return Err(StoreError::OwnershipConflict);
        }
    }
    // Bound old corpse payloads before loading any; the immutable deadline and
    // source identity must survive routine unload/expiry and enchantment writes.
    let corpse_sizes=sqlx::query("SELECT object_id,octet_length(payload) AS payload_bytes FROM entity_snapshots WHERE object_id=ANY($1)").bind(&corpse_ids).fetch_all(&mut **tx).await?;
    let mut corpse_bytes = 0usize;
    for row in corpse_sizes {
        let n = usize::try_from(row.get::<i32, _>("payload_bytes"))
            .map_err(|_| StoreError::Invalid("corpse payload bytes"))?;
        if n > 2 * 1024 * 1024 + 52 {
            return Err(StoreError::Invalid("corpse payload bytes"));
        }
        corpse_bytes = corpse_bytes
            .checked_add(n)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or(StoreError::Invalid("corpse batch bytes"))?;
    }
    let corpses =
        sqlx::query("SELECT object_id,payload FROM entity_snapshots WHERE object_id=ANY($1)")
            .bind(&corpse_ids)
            .fetch_all(&mut **tx)
            .await?;
    for row in corpses {
        let id = u32::try_from(row.get::<i64, _>("object_id"))
            .map_err(|_| StoreError::Invalid("corpse identity"))?;
        let snapshot = snapshots
            .get(&id)
            .ok_or(StoreError::Invalid("missing corpse snapshot"))?;
        let after = bace_storage_codec::CorpseSaveV5::decode_or_migrate(&snapshot.bytes, None)
            .map_err(|_| StoreError::Invalid("next corpse payload"))?;
        let before = bace_storage_codec::CorpseSaveV5::decode_or_migrate(
            &row.get::<Vec<u8>, _>("payload"),
            Some(after.placement.clone()),
        )
        .map_err(|_| StoreError::Invalid("prior corpse payload"))?;
        bace_storage_codec::validate_corpse_transition_v5(&before, &after)
            .map_err(|_| StoreError::Invalid("corpse death identity/expiry changed"))?;
    }
    construction::validate(tx, &item_identity_ids, &snapshots).await?;
    // Other aggregates stay opaque here. A foundation character without a
    // players row, or a corpse/container, is not assumed to be an item DTO.
    Ok(())
}

mod construction;
