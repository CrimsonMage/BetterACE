//! Freeze exactly the reserved allegiance patch and player reward snapshots.
//! This adapter does not recompute a tithe, obtain time or reroll confirmations.
use bace_allegiance::AllegiancePatch;
use bace_persistence::{
    AllegianceOperation, AllegianceWrite, CharacterLease, SaveSnapshot, StoredAllegiance,
};
use bace_storage_codec::SaveCodecError;
use std::collections::BTreeSet;

pub struct AllegianceFreezeInput<'a> {
    pub world_epoch: u64,
    pub operation: u64,
    pub patch: &'a AllegiancePatch,
    pub stored_nodes: &'a [StoredAllegiance],
    pub stored_metadata: &'a [StoredAllegiance],
    /// Exact character-owner reward proposals, already reserved. Empty for
    /// relationship, permission or metadata changes without a player XP award.
    pub players: &'a [SaveSnapshot],
    pub leases: &'a [CharacterLease],
}
pub fn freeze_allegiance(
    input: AllegianceFreezeInput<'_>,
) -> Result<AllegianceOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("allegiance patch/snapshot identity");
    let patch = input.patch;
    if input.world_epoch == 0
        || input.operation == 0
        || patch.after_revision > i64::MAX as u64
        || patch.before_revision.checked_add(1) != Some(patch.after_revision)
        || patch.nodes.len() + patch.metadata.len() > 1024
        || input.players.len() > 1024
        || input.leases.len() > 1024
        || input.stored_nodes.len() > 1024
        || input.stored_metadata.len() > 1024
    {
        return Err(invalid());
    }
    let mut nodes = Vec::with_capacity(patch.nodes.len());
    let mut metadata = Vec::with_capacity(patch.metadata.len());
    for (before, after) in &patch.nodes {
        let id = before
            .as_ref()
            .or(after.as_ref())
            .ok_or_else(invalid)?
            .character
            .0;
        let previous = before
            .as_ref()
            .map(crate::social_saves::freeze_allegiance_node)
            .transpose()?
            .map(|n| n.encode())
            .transpose()?;
        let bytes = after
            .as_ref()
            .map(crate::social_saves::freeze_allegiance_node)
            .transpose()?
            .map(|n| n.encode())
            .transpose()?;
        nodes.push(freeze_write(
            id,
            previous,
            bytes,
            patch.after_revision,
            input.stored_nodes,
        )?);
    }
    for (before, after) in &patch.metadata {
        let id = before
            .as_ref()
            .or(after.as_ref())
            .ok_or_else(invalid)?
            .monarch
            .0;
        let previous = before
            .as_ref()
            .map(crate::social_saves::freeze_allegiance_metadata)
            .transpose()?
            .map(|n| n.encode())
            .transpose()?;
        let bytes = after
            .as_ref()
            .map(crate::social_saves::freeze_allegiance_metadata)
            .transpose()?
            .map(|n| n.encode())
            .transpose()?;
        metadata.push(freeze_write(
            id,
            previous,
            bytes,
            patch.after_revision,
            input.stored_metadata,
        )?);
    }
    for rows in [&nodes, &metadata] {
        let mut ids = BTreeSet::new();
        if rows.iter().any(|w| !ids.insert(w.character)) {
            return Err(invalid());
        }
    }
    Ok(AllegianceOperation {
        operation_id: format!("allegiance:{}:{}", input.world_epoch, input.operation),
        nodes,
        metadata,
        players: input.players.to_vec(),
        leases: input.leases.to_vec(),
    })
}
fn freeze_write(
    id: u32,
    previous: Option<Vec<u8>>,
    bytes: Option<Vec<u8>>,
    revision: u64,
    stored: &[StoredAllegiance],
) -> Result<AllegianceWrite, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("allegiance prior snapshot mismatch");
    let mut found = stored.iter().filter(|row| row.character == id);
    let old = found.next();
    if found.next().is_some() {
        return Err(invalid());
    }
    let version = match (old, previous) {
        (Some(old), Some(before))
            if old.bytes == before
                && old.mutation_revision < revision
                && old.persisted_version > 0
                && old.persisted_version < i64::MAX =>
        {
            old.persisted_version
        }
        (None, None) => 0,
        _ => return Err(invalid()),
    };
    Ok(AllegianceWrite {
        character: id,
        expected_version: version,
        mutation_revision: revision,
        bytes,
    })
}
