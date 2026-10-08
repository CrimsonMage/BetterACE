//! ACE Creature_Death.CreateCorpse NoCorpse branch: each selected root becomes
//! a world item. The accepted death pose is copied into every root aggregate.
use super::{DeathPlayer, death_placements, set};
use bace_content::Position;
use bace_persistence::{
    CharacterLease, DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot,
};
use bace_simulation::DeathProposal;
use bace_storage_codec::{
    EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5, SaveCodecError,
};
use std::collections::{BTreeMap, BTreeSet};

pub struct NoCorpseFreezeInput<'a> {
    pub proposal: &'a DeathProposal,
    /// Authoritative accepted death pose, copied for every world root.
    pub position: Position,
    /// Fresh IDs and immutable templates in exact source-selected drop order.
    pub items: &'a [EntitySaveV1],
    pub players: &'a [DeathPlayer],
    pub leases: &'a [CharacterLease],
    /// Source CreateCorpse returns before GenerateTreasure for an Olthoi killer.
    pub olthoi_killer: bool,
}

pub fn freeze_native_no_corpse_death(
    input: NoCorpseFreezeInput<'_>,
) -> Result<PlacementOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("native NoCorpse death proposal");
    let proposal = input.proposal;
    let native = proposal.native.as_ref().ok_or_else(invalid)?;
    let no_drops = native.generated.is_empty()
        && native.source_items.as_ref().is_none_or(Vec::is_empty)
        && native.source_parents.as_ref().is_none_or(Vec::is_empty);
    if native.event_id == [0; 16]
        || !proposal.no_corpse
        || proposal.position.as_ref() != Some(&input.position)
        || proposal.olthoi_killer != input.olthoi_killer
        || native.key_version == 0
        || native.graph_id == 0
        || native.graph_revision == [0; 32]
        || native.content_generation == [0; 32]
        || native.rare_profile_revision == Some([0; 32])
        || native.generated.len() > 257
        || input.olthoi_killer && (!no_drops || native.rare.is_some())
        || native
            .source_items
            .as_ref()
            .is_some_and(|items| items.len() != native.generated.len())
        || input.items.len() != native.generated.len()
        || proposal.drops.len() != native.generated.len()
        || input.players.len() > 256
        || input.players.len() != proposal.experience_state.len()
        || input.leases.len() > 1024
        || (proposal.social.is_none() && input.leases.len() != input.players.len())
    {
        return Err(invalid());
    }
    ItemPlacementV2::World(input.position.clone()).validate(1)?;
    if let Some(rare) = &native.rare
        && (native.rare_profile_revision.is_none()
            || proposal.owner.map(|id| id.0) != Some(rare.character)
            || rare.profile_id == 0
            || rare.previous.key_version != native.key_version
            || rare.next.key_version != native.key_version)
    {
        return Err(invalid());
    }
    let mut rare_drops = native.generated.iter().filter(|drop| drop.node == "$rare");
    match native.rare.as_ref().and_then(|rare| rare.award) {
        Some(award) => {
            let item = rare_drops.next().ok_or_else(invalid)?;
            if item.template != award.template
                || item.stack != 1
                || !item.mutations.is_empty()
                || rare_drops.next().is_some()
            {
                return Err(invalid());
            }
        }
        None if rare_drops.next().is_some() => return Err(invalid()),
        None => {}
    }
    let operation_id = format!(
        "native-no-corpse-{:032x}",
        u128::from_le_bytes(native.event_id)
    );
    let placements = death_placements(native, input.items, 0)?;
    let mut ids = BTreeSet::new();
    let mut snapshots = Vec::with_capacity(input.items.len() + input.players.len());
    let mut changes = Vec::with_capacity(input.items.len());
    for (index, ((drop, legacy), source)) in native
        .generated
        .iter()
        .zip(&proposal.drops)
        .zip(input.items)
        .enumerate()
    {
        if drop.template != legacy.template
            || drop.stack != legacy.stack
            || source.object_id < 0x8000_0000
            || !ids.insert(source.object_id)
        {
            return Err(invalid());
        }
        let (container, slot, pack_slot) = placements[index];
        let mut entity = source.clone();
        entity.mutation_revision = 1;
        entity.state = if let Some(items) = &native.source_items {
            if source.state != items[index] || source.state.weenie_id != drop.template {
                return Err(invalid());
            }
            items[index].clone()
        } else {
            bace_loot::materialize_drop(drop, &source.state).map_err(|_| invalid())?
        };
        entity.state.properties.positions.retain(|p| p.id != 1);
        entity
            .state
            .properties
            .instance_ids
            .retain(|p| ![1, 2, 3].contains(&p.id));
        entity
            .state
            .properties
            .ints
            .retain(|p| ![10, 53].contains(&p.id));
        let placement = if container == 0 {
            // Pinned ACE sets GeneratorId on quest-bearing world roots only.
            if entity
                .state
                .properties
                .strings
                .iter()
                .any(|p| p.id == 33 && !p.value.is_empty())
            {
                set(
                    &mut entity.state.properties.instance_ids,
                    6,
                    proposal.victim.0,
                );
            }
            set(
                &mut entity.state.properties.positions,
                1,
                input.position.clone(),
            );
            ItemPlacementV2::World(input.position.clone())
        } else {
            set(&mut entity.state.properties.instance_ids, 1, container);
            set(&mut entity.state.properties.instance_ids, 2, container);
            set(&mut entity.state.properties.ints, 53, slot as i32);
            ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot,
                equipped: 0,
            }
        };
        // Native death loot carries a selected Weenie, not its CreateList row.
        // Preserve an unknown source destination until that exact origin is
        // threaded through NativeDeathLoot; world placement is not evidence.
        let item = ItemSaveV5::migrate_v4(ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity,
            placement: placement.clone(),
        })?)?;
        snapshots.push(SaveSnapshot {
            object_id: item.entity.object_id,
            mutation_revision: 1,
            expected_version: 0,
            bytes: item.encode()?,
        });
        changes.push(PlacementChange {
            item: item.entity.object_id,
            expected: None,
            destination: match placement {
                ItemPlacementV2::World(position) => DurableItemPlace::World {
                    cell: position.obj_cell_id,
                },
                ItemPlacementV2::Contained {
                    container,
                    slot,
                    pack_slot,
                    equipped,
                } => DurableItemPlace::Contained {
                    container,
                    slot,
                    pack_slot,
                    equipped,
                },
                ItemPlacementV2::Removed => return Err(invalid()),
            },
        });
    }
    let shared_players = if let Some(ticket) = &proposal.social {
        if ticket.rare != native.rare {
            return Err(invalid());
        }
        let inputs = input
            .players
            .iter()
            .map(|p| {
                Ok(crate::allegiance_players::AllegiancePlayerInput {
                    saved: &p.saved,
                    version: p.persisted_version,
                    lease: *input
                        .leases
                        .iter()
                        .find(|l| l.character_id == p.saved.player.entity.object_id)
                        .ok_or_else(invalid)?,
                })
            })
            .collect::<Result<Vec<_>, SaveCodecError>>()?;
        Some(
            crate::allegiance_players::freeze_allegiance_players(ticket, &inputs)?
                .players
                .into_iter()
                .map(|p| (p.player.entity.object_id, p))
                .collect::<BTreeMap<_, _>>(),
        )
    } else {
        None
    };
    for (actor, credit) in &proposal.experience_state {
        if !ids.insert(actor.0) {
            return Err(invalid());
        }
        let source = input
            .players
            .iter()
            .find(|p| p.saved.player.entity.object_id == actor.0)
            .ok_or_else(invalid)?;
        if !input.leases.iter().any(|l| l.character_id == actor.0)
            || source.persisted_version <= 0
            || source.saved.player.entity.mutation_revision != credit.before_revision
            || credit.after_available > i64::MAX as u64
            || credit.after_available < credit.before_available
            || source
                .saved
                .player
                .entity
                .state
                .properties
                .int64s
                .iter()
                .find(|p| p.id == 2)
                .map_or(0, |p| p.value)
                != credit.before_available as i64
        {
            return Err(invalid());
        }
        let next = if let Some(players) = &shared_players {
            let next = players.get(&actor.0).ok_or_else(invalid)?.clone();
            if next.player.entity.mutation_revision != credit.after_revision {
                return Err(invalid());
            }
            next
        } else {
            let mut next = source.saved.clone();
            let rare = native.rare.as_ref().filter(|r| r.character == actor.0);
            let changed = credit.after_available != credit.before_available
                || rare.is_some_and(|r| r.eligible);
            let revision = credit
                .before_revision
                .checked_add(u64::from(changed))
                .ok_or_else(invalid)?;
            if credit.after_revision != revision {
                return Err(invalid());
            }
            if let Some(rare) = rare {
                next = crate::rare_saves::freeze_rare_decision(&next, rare, revision)?;
            }
            next.player.entity.mutation_revision = revision;
            set(
                &mut next.player.entity.state.properties.int64s,
                2,
                credit.after_available as i64,
            );
            next
        };
        snapshots.push(SaveSnapshot {
            object_id: actor.0,
            mutation_revision: next.player.entity.mutation_revision,
            expected_version: source.persisted_version,
            bytes: next.encode()?,
        });
    }
    if native.rare.as_ref().is_some_and(|r| {
        !proposal
            .experience_state
            .iter()
            .any(|(id, _)| id.0 == r.character)
    }) {
        return Err(invalid());
    }
    Ok(PlacementOperation {
        operation_id,
        snapshots,
        participants: ids
            .into_iter()
            .chain(input.leases.iter().map(|l| l.character_id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        leases: input.leases.to_vec(),
        changes,
        storage_views: vec![],
    })
}
