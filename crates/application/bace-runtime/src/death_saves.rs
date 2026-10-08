//! Freeze one native death ticket into one atomic placement transaction. This
//! adapter does not roll RNG, allocate identities, or acknowledge world success.
use crate::game_inventory::set;
use bace_content::Position;
use bace_persistence::{
    CharacterLease, DurableItemPlace, PlacementChange, PlacementOperation, SaveSnapshot,
};
use bace_simulation::DeathProposal;
use bace_storage_codec::{
    CorpseSaveV1, CorpseSaveV2, EntitySaveV1, ItemPlacementV2, ItemSaveV2, ItemSaveV4, ItemSaveV5,
    PlayerSaveV6, SaveCodecError,
};
use std::collections::{BTreeMap, BTreeSet};
mod no_corpse;
pub use no_corpse::{NoCorpseFreezeInput, freeze_native_no_corpse_death};

pub struct DeathPlayer {
    pub saved: PlayerSaveV6,
    pub persisted_version: i64,
}
pub struct DeathFreezeInput<'a> {
    pub proposal: &'a DeathProposal,
    /// Fresh allocated dynamic identity, admitted template and authoritative pose.
    pub corpse: EntitySaveV1,
    pub position: Position,
    pub expires_at: i64,
    /// Same order as the frozen generated drops; immutable template snapshots.
    pub items: &'a [EntitySaveV1],
    pub players: &'a [DeathPlayer],
    pub leases: &'a [CharacterLease],
}
pub fn freeze_native_death(
    input: DeathFreezeInput<'_>,
) -> Result<PlacementOperation, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("native death proposal identity/revision");
    let proposal = input.proposal;
    let native = proposal.native.as_ref().ok_or_else(invalid)?;
    if native.event_id == [0; 16]
        || proposal.no_corpse
        || proposal.position.as_ref() != Some(&input.position)
        || native.key_version == 0
        || native.graph_id == 0
        || native.rare_profile_revision == Some([0; 32])
        || native.graph_revision == [0; 32]
        || native.content_generation == [0; 32]
        || native.generated.len() > 257
        || native
            .source_items
            .as_ref()
            .is_some_and(|items| items.len() != native.generated.len())
        || input.items.len() != native.generated.len()
        || proposal.drops.len() != native.generated.len()
        || input.corpse.state.weenie_id != proposal.corpse_template
        || input.expires_at < 0
        || input.players.len() > 256
        || input.players.len() != proposal.experience_state.len()
        || input.leases.len() > 1024
        || (proposal.social.is_none() && input.leases.len() != input.players.len())
    {
        return Err(invalid());
    }
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
    // Event IDs are allocated by the server and retained across retries. The
    // database fingerprints the exact frozen player, corpse and generated items.
    let operation_id = format!("native-death-{:032x}", u128::from_le_bytes(native.event_id));
    let corpse_id = input.corpse.object_id;
    let mut ids = BTreeSet::new();
    ids.insert(corpse_id);
    let mut entity = input.corpse;
    entity.mutation_revision = 1;
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
    set(
        &mut entity.state.properties.positions,
        1,
        input.position.clone(),
    );
    let corpse = CorpseSaveV2 {
        corpse: CorpseSaveV1 {
            entity,
            owner: proposal.owner.map(|id| id.0),
            death_operation: operation_id.clone(),
            expires_at: input.expires_at,
        },
        placement: ItemPlacementV2::World(input.position.clone()),
    };
    let placements = death_placements(native, input.items, corpse_id)?;
    let mut snapshots = vec![SaveSnapshot {
        object_id: corpse_id,
        mutation_revision: 1,
        expected_version: 0,
        bytes: bace_storage_codec::CorpseSaveV5::migrate_v4(bace_storage_codec::CorpseSaveV4 {
            previous: bace_storage_codec::CorpseSaveV3::migrate_v2(corpse)?,
            source: Some(proposal.victim.0),
            operation: Some(proposal.operation),
        })
        .and_then(|mut corpse| {
            corpse.access.victim = Some(proposal.victim.0);
            corpse.access.killer = proposal.owner.map(|id| id.0);
            corpse.access.is_monster = true;
            corpse.access.generated_rare = native.rare.as_ref().is_some_and(|r| r.award.is_some());
            corpse.encode()
        })?,
    }];
    let mut changes = vec![PlacementChange {
        item: corpse_id,
        expected: None,
        destination: DurableItemPlace::World {
            cell: input.position.obj_cell_id,
        },
    }];
    for (index, ((drop, legacy), source)) in native
        .generated
        .iter()
        .zip(&proposal.drops)
        .zip(input.items)
        .enumerate()
    {
        if drop.template != legacy.template
            || drop.stack != legacy.stack
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
        set(&mut entity.state.properties.instance_ids, 1, container);
        set(&mut entity.state.properties.instance_ids, 2, container);
        set(&mut entity.state.properties.ints, 53, slot as i32);
        // NativeDeathLoot freezes the selected source Weenie, but not the
        // CreateList destination flags. Keep origin unknown on these new rows.
        let item = ItemSaveV5::migrate_v4(ItemSaveV4::migrate_v2(ItemSaveV2 {
            entity,
            placement: ItemPlacementV2::Contained {
                container,
                slot,
                pack_slot,
                equipped: 0,
            },
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
            destination: DurableItemPlace::Contained {
                container,
                slot,
                pack_slot,
                equipped: 0,
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
        let revision = next.player.entity.mutation_revision;
        snapshots.push(SaveSnapshot {
            object_id: actor.0,
            mutation_revision: revision,
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

fn death_placements(
    native: &bace_simulation::NativeDeathLoot,
    items: &[EntitySaveV1],
    corpse: u32,
) -> Result<Vec<(u32, u32, bool)>, SaveCodecError> {
    let invalid = || SaveCodecError::Invalid("native corpse item ancestry");
    if native
        .source_parents
        .as_ref()
        .is_some_and(|parents| parents.len() != items.len() || native.source_items.is_none())
    {
        return Err(invalid());
    }
    let mut slots = BTreeMap::<(u32, bool), u32>::new();
    let mut output = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pack = item
            .state
            .properties
            .bools
            .iter()
            .any(|p| p.id == 81 && p.value);
        let parent = native
            .source_parents
            .as_ref()
            .and_then(|parents| parents[index]);
        let container = match parent {
            Some(parent) if parent < index => items[parent].object_id,
            Some(_) => return Err(invalid()),
            None => corpse,
        };
        let slot = slots.entry((container, pack)).or_default();
        if let Some(parent) = parent {
            let props = &items[parent].state.properties;
            if !props.ints.iter().any(|p| p.id == 6) {
                return Err(invalid());
            }
            let capacity = props
                .ints
                .iter()
                .find(|p| p.id == if pack { 7 } else { 6 })
                .map_or(0, |p| p.value);
            if u32::try_from(capacity)
                .ok()
                .is_none_or(|capacity| *slot >= capacity)
            {
                return Err(invalid());
            }
        }
        output.push((container, *slot, pack));
        *slot = slot.checked_add(1).ok_or_else(invalid)?;
    }
    Ok(output)
}
