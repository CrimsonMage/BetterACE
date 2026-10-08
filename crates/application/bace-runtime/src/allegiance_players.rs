//! Freeze reserved player awards alongside the exact allegiance ledger operation.
use bace_persistence::{CharacterLease, OwnershipState, SaveSnapshot};
use bace_simulation::AllegianceTicket;
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
use std::collections::BTreeSet;
pub struct AllegiancePlayerInput<'a> {
    pub saved: &'a PlayerSaveV6,
    pub version: i64,
    pub lease: CharacterLease,
}
pub struct FrozenAllegiancePlayers {
    pub snapshots: Vec<SaveSnapshot>,
    pub players: Vec<PlayerSaveV6>,
    pub leases: Vec<CharacterLease>,
}
pub fn freeze_allegiance_players(
    ticket: &AllegianceTicket,
    inputs: &[AllegiancePlayerInput<'_>],
) -> Result<FrozenAllegiancePlayers, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("allegiance player award identity or before state");
    if inputs.len() != ticket.player_changes.len() || inputs.len() > 1024 || ticket.operation == 0 {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    let mut snapshots = Vec::with_capacity(inputs.len());
    let mut players = Vec::with_capacity(inputs.len());
    let mut leases = Vec::with_capacity(inputs.len());
    for input in inputs {
        let id = input.saved.player.entity.object_id;
        if !ids.insert(id)
            || input.version <= 0
            || input.version == i64::MAX
            || input.lease.character_id != id
            || input.lease.epoch <= 0
            || input.lease.state != OwnershipState::Online
        {
            return Err(invalid());
        }
        let mut matching = ticket
            .player_changes
            .iter()
            .filter(|(actor, _)| actor.0 == id);
        let (actor, change) = matching.next().ok_or_else(invalid)?;
        if matching.next().is_some() {
            return Err(invalid());
        }
        let mut frozen = crate::npc_persistence::freeze_player_effect(
            input.saved,
            &bace_simulation::NpcEffect::EarnedExperience {
                actor: *actor,
                change: change.clone(),
            },
        )?;
        let revision = ticket.player_revision(*actor).ok_or_else(invalid)?;
        if let Some(rare) = ticket.rare.as_ref().filter(|r| r.character == id) {
            frozen = crate::rare_saves::freeze_rare_decision(&frozen, rare, revision)?;
        }
        for patch in ticket
            .item_experience
            .iter()
            .flat_map(|(r, _)| &r.registries)
            .filter(|p| p.actor == *actor)
        {
            let before = patch
                .before
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
            if input.saved.enchantments != before {
                return Err(invalid());
            }
            frozen.enchantments = patch
                .after
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
        }
        if let Some(patch) = ticket.vitae.iter().find(|p| p.actor == *actor) {
            let before = patch
                .before_enchantments
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
            if input.saved.enchantments != before {
                return Err(invalid());
            }
            if crate::player_death_state::restore_player_death_state(input.saved)? != patch.before {
                return Err(invalid());
            }
            crate::player_death_state::freeze_player_death_state(&mut frozen, &patch.after)?;
            frozen.enchantments = patch
                .registry
                .entries()
                .iter()
                .map(crate::enchantment_saves::freeze_enchantment)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid())?;
        }
        for vital in ticket.vitals.iter().filter(|v| v.actor == *actor) {
            crate::portal_saves::apply_vital(&mut frozen, *vital)?;
        }
        frozen.player.entity.mutation_revision = revision;
        snapshots.push(SaveSnapshot {
            object_id: id,
            mutation_revision: frozen.player.entity.mutation_revision,
            expected_version: input.version,
            bytes: frozen.encode()?,
        });
        players.push(frozen);
        leases.push(input.lease);
    }
    Ok(FrozenAllegiancePlayers {
        snapshots,
        players,
        leases,
    })
}
