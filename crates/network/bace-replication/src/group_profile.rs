//! Lossless bounded domain snapshots into the source wire structures.
use crate::SessionProjectionError;
use bace_gameplay_api::social::{
    AllegianceNodeSnapshot, AllegianceProfileSnapshot, FellowshipSnapshot,
};
use bace_wire::{
    AllegianceMemberData, AllegianceProfileData, FellowData, FellowshipData, WirePosition,
};
pub(crate) fn fellowship(
    source: &FellowshipSnapshot,
) -> Result<FellowshipData, SessionProjectionError> {
    if source.members.len() > 9 || source.departed.len() > 4096 || source.locks.len() > 4096 {
        return Err(SessionProjectionError::Limit);
    }
    Ok(FellowshipData {
        name: source.name.clone(),
        leader: source.leader.0,
        members: source
            .members
            .iter()
            .map(|m| FellowData {
                actor: m.actor.0,
                name: m.name.clone(),
                level: m.level,
                maximum: m.maximum,
                current: m.current,
            })
            .collect(),
        share_xp: source.share_xp,
        even_share: source.even_share,
        open: source.open,
        locked: source.locked,
        departed: source
            .departed
            .iter()
            .map(|(id, time)| {
                Ok((
                    id.0,
                    i32::try_from(*time).map_err(|_| SessionProjectionError::InvalidProjection)?,
                ))
            })
            .collect::<Result<_, SessionProjectionError>>()?,
        locks: source
            .locks
            .iter()
            .map(|(name, time, seq)| {
                Ok((
                    name.clone(),
                    u32::try_from(*time).map_err(|_| SessionProjectionError::InvalidProjection)?,
                    *seq,
                ))
            })
            .collect::<Result<_, SessionProjectionError>>()?,
    })
}
fn node(n: &AllegianceNodeSnapshot) -> AllegianceMemberData {
    AllegianceMemberData {
        actor: n.actor.0,
        cached: n.cached,
        tithed: n.tithed,
        online: n.online,
        may_pass_up: n.may_pass_up,
        gender: n.gender,
        heritage: n.heritage,
        rank: n.rank,
        level: n.level,
        loyalty: n.loyalty,
        leadership: n.leadership,
        name: n.name.clone(),
    }
}
pub(crate) fn allegiance(
    source: &AllegianceProfileSnapshot,
) -> Result<AllegianceProfileData, SessionProjectionError> {
    if source.records.len() > 12 {
        return Err(SessionProjectionError::Limit);
    }
    Ok(AllegianceProfileData {
        total_members: source.total_members,
        total_vassals: source.total_vassals,
        chat_room: source.chat_room,
        name: source.name.clone(),
        sanctuary: source.sanctuary.map(|p| WirePosition {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }),
        monarch: source.monarch.as_ref().map(node),
        records: source.records.iter().map(|(p, n)| (p.0, node(n))).collect(),
    })
}
