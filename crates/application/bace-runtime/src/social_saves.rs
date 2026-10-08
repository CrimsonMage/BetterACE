//! Explicit frozen DTO adapters; gameplay state never defines the persisted schema.
use bace_allegiance::{AllegianceMetadata, AllegianceNode};
use bace_gameplay_api::social::{AllegianceSanctuary, SocialSquelch};
use bace_social::SocialPreferences;
use bace_storage_codec::{
    AllegianceMetadataV1, AllegianceNodeV1, AllegianceSanctuaryV1, SaveCodecError, SocialSaveV1,
    SocialSquelchV1,
};
use bace_types::{AccountId, EntityId};

pub fn freeze_social(value: &SocialPreferences) -> Result<SocialSaveV1, SaveCodecError> {
    value
        .validate()
        .map_err(|_| SaveCodecError::Invalid("social domain state"))?;
    let saved = SocialSaveV1 {
        friends: value.friends.iter().map(|id| id.0).collect(),
        squelches: value
            .squelches
            .iter()
            .map(|s| SocialSquelchV1 {
                character: s.character.0,
                account: s.account.map(|id| id.0),
                name: s.name.clone(),
                mask: s.mask,
            })
            .collect(),
        global_mask: value.global_mask,
        afk_message: value.afk_message.clone(),
        channels: value.channels.iter().copied().collect(),
    };
    saved.validate()?;
    Ok(saved)
}
pub fn restore_social(value: &SocialSaveV1) -> Result<SocialPreferences, SaveCodecError> {
    value.validate()?;
    let state = SocialPreferences {
        friends: value.friends.iter().copied().map(EntityId).collect(),
        squelches: value
            .squelches
            .iter()
            .map(|s| SocialSquelch {
                character: EntityId(s.character),
                account: s.account.map(AccountId),
                name: s.name.clone(),
                mask: s.mask,
            })
            .collect(),
        global_mask: value.global_mask,
        afk_message: value.afk_message.clone(),
        channels: value.channels.iter().copied().collect(),
    };
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("social domain state"))?;
    Ok(state)
}
pub fn freeze_allegiance_node(n: &AllegianceNode) -> Result<AllegianceNodeV1, SaveCodecError> {
    let saved = AllegianceNodeV1 {
        character: n.character.0,
        account: n.account.0,
        name: n.name.clone(),
        gender: n.gender,
        heritage: n.heritage,
        patron: n.patron.map(|id| id.0),
        monarch: n.monarch.0,
        vassals: n.vassals.iter().map(|id| id.0).collect(),
        rank: n.rank,
        followers: n.followers,
        level: n.level,
        leadership: n.leadership,
        loyalty: n.loyalty,
        sworn_at: n.sworn_at,
        online_seconds: n.online_seconds,
        may_pass_up: n.may_pass_up,
        received_total: n.received_total,
        tithed_total: n.tithed_total,
        unclaimed: n.unclaimed,
    };
    saved.validate()?;
    Ok(saved)
}
pub fn restore_allegiance_node(n: &AllegianceNodeV1) -> Result<AllegianceNode, SaveCodecError> {
    n.validate()?;
    let state = AllegianceNode {
        character: EntityId(n.character),
        account: AccountId(n.account),
        name: n.name.clone(),
        gender: n.gender,
        heritage: n.heritage,
        patron: n.patron.map(EntityId),
        monarch: EntityId(n.monarch),
        vassals: n.vassals.iter().copied().map(EntityId).collect(),
        rank: n.rank,
        followers: n.followers,
        level: n.level,
        leadership: n.leadership,
        loyalty: n.loyalty,
        sworn_at: n.sworn_at,
        online_seconds: n.online_seconds,
        may_pass_up: n.may_pass_up,
        received_total: n.received_total,
        tithed_total: n.tithed_total,
        unclaimed: n.unclaimed,
    };
    state
        .validate()
        .map_err(|_| SaveCodecError::Invalid("allegiance domain node"))?;
    Ok(state)
}
pub fn freeze_allegiance_metadata(
    m: &AllegianceMetadata,
) -> Result<AllegianceMetadataV1, SaveCodecError> {
    let saved = AllegianceMetadataV1 {
        monarch: m.monarch.0,
        chat_room: m.chat_room,
        name: m.name.clone(),
        motd: m.motd.clone(),
        motd_set_by: m.motd_set_by.clone(),
        officer_titles: m.officer_titles.clone(),
        officers: m
            .officers
            .iter()
            .map(|(id, level)| (id.0, *level))
            .collect(),
        locked: m.locked,
        approved: m.approved.iter().map(|id| id.0).collect(),
        banned_characters: m
            .banned_characters
            .iter()
            .map(|(id, name)| (id.0, name.clone()))
            .collect(),
        chat_gags: m
            .chat_gags
            .iter()
            .map(|(id, until)| (id.0, *until))
            .collect(),
        sanctuary: m.sanctuary.map(|p| AllegianceSanctuaryV1 {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }),
    };
    saved.validate()?;
    Ok(saved)
}
pub fn restore_allegiance_metadata(
    m: &AllegianceMetadataV1,
) -> Result<AllegianceMetadata, SaveCodecError> {
    m.validate()?;
    Ok(AllegianceMetadata {
        monarch: EntityId(m.monarch),
        chat_room: m.chat_room,
        name: m.name.clone(),
        motd: m.motd.clone(),
        motd_set_by: m.motd_set_by.clone(),
        officer_titles: m.officer_titles.clone(),
        officers: m
            .officers
            .iter()
            .map(|(id, level)| (EntityId(*id), *level))
            .collect(),
        locked: m.locked,
        approved: m.approved.iter().copied().map(EntityId).collect(),
        banned_characters: m
            .banned_characters
            .iter()
            .map(|(id, name)| (EntityId(*id), name.clone()))
            .collect(),
        chat_gags: m
            .chat_gags
            .iter()
            .map(|(id, until)| (EntityId(*id), *until))
            .collect(),
        sanctuary: m.sanctuary.map(|p| AllegianceSanctuary {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }),
    })
}
/// Restore one complete cold forest after bounded DB paging. Durable row revisions
/// survive process restarts; the next proposal starts above every accepted row.
pub fn restore_allegiances(
    nodes: &[bace_persistence::StoredAllegiance],
    metadata: &[bace_persistence::StoredAllegiance],
    capacity: usize,
) -> Result<bace_allegiance::AllegianceRegistry, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("stored allegiance forest identity or revision");
    if nodes.len() > capacity || metadata.len() > nodes.len() {
        return Err(invalid());
    }
    let mut revision = 0;
    let mut restored_nodes = Vec::with_capacity(nodes.len());
    for row in nodes {
        if row.persisted_version <= 0 || row.mutation_revision > i64::MAX as u64 {
            return Err(invalid());
        }
        let saved = AllegianceNodeV1::decode(&row.bytes)?;
        if saved.character != row.character {
            return Err(invalid());
        }
        revision = revision.max(row.mutation_revision);
        restored_nodes.push(restore_allegiance_node(&saved)?);
    }
    let mut restored_metadata = Vec::with_capacity(metadata.len());
    for row in metadata {
        if row.persisted_version <= 0 || row.mutation_revision > i64::MAX as u64 {
            return Err(invalid());
        }
        let saved = AllegianceMetadataV1::decode(&row.bytes)?;
        if saved.monarch != row.character {
            return Err(invalid());
        }
        revision = revision.max(row.mutation_revision);
        restored_metadata.push(restore_allegiance_metadata(&saved)?);
    }
    bace_allegiance::AllegianceRegistry::restore(
        restored_nodes,
        restored_metadata,
        revision,
        capacity,
    )
    .map_err(|_| invalid())
}
