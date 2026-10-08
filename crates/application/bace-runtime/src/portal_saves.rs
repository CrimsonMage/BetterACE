//! Freeze portal destinations/links and resource debits before local completion.
//! The committed location is sufficient to recover after a process interruption.
use bace_persistence::{CharacterLease, OwnershipState, PlacementOperation, SaveSnapshot};
use bace_simulation::{
    PortalServiceEffect, PortalServiceOrigin, PortalServiceReceipt, PortalServiceTicket,
};
use bace_storage_codec::{PlayerSaveV6, SaveCodecError};
use std::collections::BTreeSet;

pub struct PortalSavePlayer<'a> {
    pub saved: &'a PlayerSaveV6,
    pub version: i64,
    pub lease: CharacterLease,
}
pub struct FrozenPortalSave {
    pub operation: PlacementOperation,
    pub receipt: PortalServiceReceipt,
    pub players: Vec<PlayerSaveV6>,
}

pub fn freeze_portal_service(
    epoch: u64,
    ticket: &PortalServiceTicket,
    players: &[PortalSavePlayer<'_>],
) -> Result<FrozenPortalSave, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("portal durable identity/revision");
    if epoch == 0
        || ticket.operation == 0
        || players.is_empty()
        || players.len() > 9
        || ticket.participants.len() != players.len()
        || ticket.origin == PortalServiceOrigin::Spell && ticket.cast == 0
        || ticket.origin != PortalServiceOrigin::Spell && ticket.cast != 0
        || ticket.before_revision.checked_add(1) != Some(ticket.after_revision)
    {
        return Err(invalid());
    }
    if ticket
        .mana
        .is_some_and(|m| m.vital != bace_entity::EntityVital::Mana)
        || !ticket.participants.contains(&(
            ticket.actor,
            ticket.before_revision,
            ticket.after_revision,
        ))
    {
        return Err(invalid());
    }
    if let PortalServiceEffect::Teleport(targets) = &ticket.effect {
        let mut destinations = BTreeSet::new();
        if targets.is_empty()
            || targets.len() > 9
            || targets.iter().any(|t| {
                !destinations.insert(t.actor) || !ticket.participants.iter().any(|p| p.0 == t.actor)
            })
        {
            return Err(invalid());
        }
    }
    let mut seen = BTreeSet::new();
    let mut snapshots = Vec::with_capacity(players.len());
    let mut frozen = Vec::with_capacity(players.len());
    for input in players {
        let id = input.saved.player.entity.object_id;
        if !seen.insert(id)
            || input.version <= 0
            || input.version == i64::MAX
            || input.lease.character_id != id
            || input.lease.epoch <= 0
            || input.lease.state != OwnershipState::Online
        {
            return Err(invalid());
        }
        input.saved.validate()?;
        let (_, before, after) = ticket
            .participants
            .iter()
            .find(|(actor, _, _)| actor.0 == id)
            .ok_or_else(invalid)?;
        if *before != input.saved.player.entity.mutation_revision
            || before.checked_add(1) != Some(*after)
        {
            return Err(invalid());
        }
        let mut next = input.saved.clone();
        if let Some(mana) = ticket.mana.filter(|m| m.actor.0 == id) {
            apply_vital(&mut next, mana)?;
        }
        match &ticket.effect {
            PortalServiceEffect::Sanctuary {
                link,
                character,
                stamina,
            } if ticket.actor.0 == id => {
                if crate::native_player::restore_services(&next)? != character.before {
                    return Err(invalid());
                }
                let contracts = crate::native_player::restore_contracts(&next)?;
                // Validate the link against its prior view without discarding
                // the resource debit or other service properties.
                let linked = crate::portal_preparation::freeze_portal_link(
                    &input.saved.player.entity.state,
                    link,
                )
                .map_err(|_| invalid())?;
                if link.position_slot != 4
                    || character.before_revision != *before
                    || character.after_revision != *after
                {
                    return Err(invalid());
                }
                crate::native_player::freeze_services(&mut next, &character.after, &contracts)?;
                if linked.properties.positions.iter().find(|p| p.id == 4)
                    != next
                        .player
                        .entity
                        .state
                        .properties
                        .positions
                        .iter()
                        .find(|p| p.id == 4)
                {
                    return Err(invalid());
                }
                apply_vital(&mut next, *stamina)?;
            }
            PortalServiceEffect::Link(change) if ticket.actor.0 == id => {
                next.player.entity.state = crate::portal_preparation::freeze_portal_link(
                    &next.player.entity.state,
                    change,
                )
                .map_err(|_| invalid())?;
            }
            PortalServiceEffect::Teleport(targets) => {
                if let Some(target) = targets.iter().find(|target| target.actor.0 == id) {
                    let mut death = crate::player_death_state::restore_player_death_state(&next)?;
                    death.protection_elapsed = None;
                    crate::player_death_state::freeze_player_death_state(&mut next, &death)?;
                    let half = target.heading * 0.5;
                    let position = bace_content::Position {
                        obj_cell_id: target.destination.0,
                        position_x: target.position.x,
                        position_y: target.position.y,
                        position_z: target.position.z,
                        rotation_w: half.cos(),
                        rotation_x: 0.0,
                        rotation_y: 0.0,
                        rotation_z: half.sin(),
                    };
                    crate::game_inventory::set(
                        &mut next.player.entity.state.properties.positions,
                        1,
                        position,
                    );
                }
            }
            PortalServiceEffect::Sanctuary { .. }
            | PortalServiceEffect::Link(_)
            | PortalServiceEffect::Summon { .. } => {}
        }
        next.player.entity.mutation_revision = *after;
        snapshots.push(SaveSnapshot {
            object_id: id,
            mutation_revision: *after,
            expected_version: input.version,
            bytes: next.encode()?,
        });
        frozen.push(next);
    }
    if !seen.contains(&ticket.actor.0)
        || match ticket.mana {
            Some(mana) => !seen.contains(&ticket.cast_actor.0) || !seen.contains(&mana.actor.0),
            None => {
                ticket.origin != PortalServiceOrigin::Spell || seen.contains(&ticket.cast_actor.0)
            }
        }
    {
        return Err(invalid());
    }
    let receipt = PortalServiceReceipt {
        operation: ticket.operation,
        actor: ticket.actor,
        after_revision: ticket.after_revision,
        revisions: ticket
            .participants
            .iter()
            .map(|(id, _, after)| (*id, *after))
            .collect(),
    };
    Ok(FrozenPortalSave {
        operation: PlacementOperation {
            operation_id: format!("portal:{epoch}:{}", ticket.operation),
            snapshots,
            participants: seen.into_iter().collect(),
            leases: players.iter().map(|p| p.lease).collect(),
            changes: Vec::new(),
            storage_views: Vec::new(),
        },
        receipt,
        players: frozen,
    })
}
pub(crate) fn apply_vital(
    saved: &mut PlayerSaveV6,
    change: bace_entity::VitalMutation,
) -> Result<(), SaveCodecError> {
    if saved.player.entity.object_id != change.actor.0 {
        return Err(SaveCodecError::Invalid("vital actor mismatch"));
    }
    let id = match change.vital {
        bace_entity::EntityVital::Health => 1,
        bace_entity::EntityVital::Stamina => 3,
        bace_entity::EntityVital::Mana => 5,
    };
    let vital = saved
        .player
        .entity
        .state
        .properties
        .secondary_attributes
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(SaveCodecError::Invalid("portal vital absent"))?;
    if vital.value.current_level != change.before {
        return Err(SaveCodecError::Invalid("portal vital revision"));
    }
    vital.value.current_level = change.after;
    Ok(())
}
