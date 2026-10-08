//! Atomic player death checkpoint: corpse, item losses and final respawn state.
//! Presentation delays remain in simulation. Recovery loads the committed living
//! state instead of replaying item loss or leaving a dead character stranded.
use crate::game_inventory::{FrozenInventoryItem, InventoryFreezeInput, freeze_inventory};
use bace_persistence::{CharacterLease, OwnershipState, PlacementOperation, SaveSnapshot};
use bace_simulation::{PlayerDeathReceipt, PlayerDeathTicket};
use bace_storage_codec::{
    CorpseSaveV1, CorpseSaveV2, CorpseSaveV3, CorpseSaveV4, CorpseSaveV5, ItemSaveV5, PlayerSaveV6,
};
use std::collections::BTreeMap;

pub struct PlayerDeathFreezeInput<'a> {
    pub epoch: u64,
    pub ticket: &'a PlayerDeathTicket,
    pub player: &'a PlayerSaveV6,
    pub persisted_version: i64,
    pub lease: CharacterLease,
    pub items: &'a [FrozenInventoryItem],
    pub positions: &'a BTreeMap<u32, bace_content::Position>,
    /// Supplied by the runtime once and retained with this frozen operation.
    pub unix_seconds: i64,
}
pub struct FrozenPlayerDeath {
    pub expires_at: i64,
    pub operation: PlacementOperation,
    pub receipt: PlayerDeathReceipt,
    pub player: PlayerSaveV6,
}
pub fn freeze_player_death(input: PlayerDeathFreezeInput<'_>) -> Result<FrozenPlayerDeath, String> {
    let ticket = input.ticket;
    if input.epoch == 0
        || ticket.operation == 0
        || input.unix_seconds < 0
        || ticket.actor.0 != input.player.player.entity.object_id
        || ticket.actor.0 != input.lease.character_id
        || input.lease.epoch <= 0
        || input.lease.state != OwnershipState::Online
        || input.persisted_version <= 0
        || input.persisted_version == i64::MAX
        || ticket.inventory.actor != ticket.actor
        || ticket.inventory.operation == 0
        || ticket.before_revision != input.player.player.entity.mutation_revision
        || ticket.before_revision.checked_add(1) != Some(ticket.after_revision)
        || ticket.destination.actor != ticket.actor
        || ticket.vitals.len() != 3
    {
        return Err("invalid player death identity/revision".into());
    }
    input.player.validate().map_err(|e| e.to_string())?;
    if crate::player_death_state::restore_player_death_state(input.player)
        .map_err(|e| e.to_string())?
        != ticket.before
    {
        return Err("stale player death metadata".into());
    }
    let prior_entries = ticket
        .before_enchantments
        .iter()
        .map(crate::enchantment_saves::freeze_enchantment)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if prior_entries != input.player.enchantments {
        return Err("stale player death registry".into());
    }
    if ticket.no_corpse.is_none()
        && (!(0x80000000..=0xfffffffe).contains(&ticket.corpse.0)
            || !ticket.inventory.proposal.changes.iter().any(|c| {
                c.before.is_none()
                    && c.after.id == ticket.corpse
                    && c.after.is_container
                    && c.after.place == bace_inventory::ItemPlace::World
            }))
    {
        return Err("invalid death corpse template/identity".into());
    }
    let mut corpse_items: Vec<_> = ticket.inventory.proposal.changes.iter().filter(|c|
        matches!(c.after.place, bace_inventory::ItemPlace::Contained { container, .. } if container == ticket.corpse))
        .map(|c| c.after.id).collect();
    corpse_items.sort_unstable();
    let mut expected_items = ticket.corpse_items.clone();
    expected_items.sort_unstable();
    if corpse_items != expected_items || (ticket.no_corpse.is_some() && !corpse_items.is_empty()) {
        return Err("death corpse membership mismatch".into());
    }
    let operation_id = format!("player-death:{}:{}", input.epoch, ticket.operation);
    let mut player = input.player.clone();
    crate::player_death_state::freeze_player_death_state(&mut player, &ticket.after)
        .map_err(|e| e.to_string())?;
    for (index, vital) in [
        bace_entity::EntityVital::Health,
        bace_entity::EntityVital::Stamina,
        bace_entity::EntityVital::Mana,
    ]
    .into_iter()
    .enumerate()
    {
        let change = ticket
            .vitals
            .iter()
            .find(|c| c.vital == vital && c.actor == ticket.actor)
            .ok_or("missing player death vital")?;
        if ticket.post_death_maxima[index] == 0
            || change.after
                != bace_interactions::restored_death_vital(ticket.post_death_maxima[index])
        {
            return Err("invalid death restored vital".into());
        }
        crate::portal_saves::apply_vital(&mut player, *change).map_err(|e| e.to_string())?;
    }
    let entries = ticket
        .enchantments
        .iter()
        .map(crate::enchantment_saves::freeze_enchantment)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    bace_storage_codec::validate_enchantments_v1(&entries).map_err(|e| e.to_string())?;
    player.enchantments = entries;
    let destination = ticket.destination;
    let half = destination.heading * 0.5;
    crate::game_inventory::set(
        &mut player.player.entity.state.properties.positions,
        1,
        bace_content::Position {
            obj_cell_id: destination.destination.0,
            position_x: destination.position.x,
            position_y: destination.position.y,
            position_z: destination.position.z,
            rotation_w: half.cos(),
            rotation_x: 0.,
            rotation_y: 0.,
            rotation_z: half.sin(),
        },
    );
    player.player.entity.mutation_revision = ticket.after_revision;
    let snapshot = SaveSnapshot {
        object_id: ticket.actor.0,
        mutation_revision: ticket.after_revision,
        expected_version: input.persisted_version,
        bytes: player.encode().map_err(|e| e.to_string())?,
    };
    let mut operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &operation_id,
        proposal: &ticket.inventory.proposal,
        items: input.items,
        other_snapshots: &[snapshot],
        leases: &[input.lease],
        storage_views: &[],
        admitted_positions: input.positions,
    })
    .map_err(|e| e.to_string())?;
    if let Some(plan) = &ticket.no_corpse {
        if ticket.corpse.0 != 0 || ticket.corpse_decay_seconds != 0 || ticket.olthoi.is_some() {
            return Err("NoCorpse has corpse obligations".into());
        }
        let roots: std::collections::BTreeSet<_> = plan.world_roots.iter().copied().collect();
        if roots.len() != plan.world_roots.len() || roots.len() > 1024 {
            return Err("NoCorpse root identity bound".into());
        }
        let selected: std::collections::BTreeSet<_> = ticket
            .inventory
            .proposal
            .changes
            .iter()
            .filter(|c| c.after.place == bace_inventory::ItemPlace::World)
            .map(|c| c.after.id)
            .collect();
        if roots != selected
            || plan
                .world_roots
                .iter()
                .any(|id| input.positions.get(&id.0) != Some(&plan.accepted_position))
        {
            return Err("NoCorpse world-root position/selection mismatch".into());
        }
        for id in &plan.world_roots {
            let source = input
                .items
                .iter()
                .find(|item| item.entity.object_id == id.0)
                .ok_or("NoCorpse world-root source absent")?;
            let row = operation
                .snapshots
                .iter_mut()
                .find(|row| row.object_id == id.0)
                .ok_or("NoCorpse world-root snapshot absent")?;
            let mut saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
            if saved.entity.object_id != id.0
                || saved.source_destination != source.source_destination
                || !matches!(&saved.placement, bace_storage_codec::ItemPlacementV2::World(position) if position == &plan.accepted_position)
            {
                return Err("NoCorpse world-root frozen identity/position mismatch".into());
            }
            let quest = source
                .entity
                .state
                .properties
                .strings
                .iter()
                .any(|p| p.id == 33 && !p.value.is_empty());
            if quest {
                crate::game_inventory::set(
                    &mut saved.entity.state.properties.instance_ids,
                    6,
                    ticket.actor.0,
                );
                row.bytes = saved.encode().map_err(|e| e.to_string())?;
            }
        }
        let mut revisions: Vec<_> = ticket
            .inventory
            .proposal
            .changes
            .iter()
            .map(|c| (c.after.id, c.after.revision))
            .collect();
        revisions.sort_unstable_by_key(|v| v.0);
        return Ok(FrozenPlayerDeath {
            expires_at: 0,
            operation,
            player,
            receipt: PlayerDeathReceipt {
                operation: ticket.operation,
                actor: ticket.actor,
                after_revision: ticket.after_revision,
                inventory: bace_simulation::InventoryReceipt {
                    operation: ticket.inventory.operation,
                    revisions,
                },
            },
        });
    }
    if ticket.olthoi.is_some() {
        // Inventory's ordinary fresh-stack rule clears GeneratorId. Olthoi
        // death treasure is a constructed source forest: nested Contain
        // children retain their exact generating parent after the same durable
        // death operation. Restore only source-proven new rows here.
        for change in &ticket.inventory.proposal.changes {
            if change.before.is_some() || change.after.id == ticket.corpse {
                continue;
            }
            let source = input
                .items
                .iter()
                .find(|item| item.entity.object_id == change.after.id.0)
                .ok_or("Olthoi source item missing")?;
            if source.persisted_version != 0 || source.placement.is_some() {
                return Err("Olthoi source item is not fresh".into());
            }
            let Some(parent) = source
                .entity
                .state
                .properties
                .instance_ids
                .iter()
                .find(|property| property.id == 6)
                .map(|property| property.value)
            else {
                continue;
            };
            let row = operation
                .snapshots
                .iter_mut()
                .find(|row| row.object_id == change.after.id.0)
                .ok_or("Olthoi generated snapshot missing")?;
            let mut saved = ItemSaveV5::decode(&row.bytes).map_err(|e| e.to_string())?;
            if saved.entity.object_id != change.after.id.0
                || saved.entity.mutation_revision != change.after.revision
                || saved.entity.state.weenie_id != source.entity.state.weenie_id
            {
                return Err("Olthoi generated snapshot mismatch".into());
            }
            crate::game_inventory::set(&mut saved.entity.state.properties.instance_ids, 6, parent);
            row.bytes = saved.encode().map_err(|e| e.to_string())?;
        }
    }
    let corpse_snapshot = operation
        .snapshots
        .iter_mut()
        .find(|s| s.object_id == ticket.corpse.0)
        .ok_or("death corpse snapshot missing")?;
    if corpse_snapshot.expected_version != 0 {
        return Err("death corpse identity already persisted".into());
    }
    let mut fresh = ItemSaveV5::decode(&corpse_snapshot.bytes).map_err(|e| e.to_string())?;
    let level = input
        .player
        .player
        .entity
        .state
        .properties
        .ints
        .iter()
        .find(|p| p.id == 25)
        .map_or(1, |p| p.value);
    crate::player_death_preparation::freeze_corpse_metadata(
        &mut fresh.entity.state,
        u32::try_from(level).map_err(|_| "invalid death player level")?,
        ticket.corpse_decay_seconds,
        input.unix_seconds,
        if ticket.olthoi.is_some() {
            bace_interactions::PlayerDeathKind::Pk
        } else {
            ticket.kind
        },
    )?;
    let expiry = input
        .unix_seconds
        .checked_add(
            i64::try_from(ticket.corpse_decay_seconds).map_err(|_| "corpse expiry overflow")?,
        )
        .ok_or("corpse expiry overflow")?;
    let corpse = CorpseSaveV3 {
        previous: CorpseSaveV2 {
            corpse: CorpseSaveV1 {
                entity: fresh.entity.clone(),
                owner: Some(ticket.actor.0),
                death_operation: operation_id,
                expires_at: expiry,
            },
            placement: fresh.placement.clone(),
        },
        enchantments: fresh.previous.previous.enchantments,
    };
    let corpse = CorpseSaveV4 {
        previous: corpse,
        source: Some(ticket.actor.0),
        operation: Some(ticket.operation),
    };
    corpse_snapshot.bytes = CorpseSaveV5::migrate_v4(corpse)
        .and_then(|corpse| corpse.encode())
        .map_err(|e| e.to_string())?;
    let mut revisions: Vec<_> = ticket
        .inventory
        .proposal
        .changes
        .iter()
        .map(|c| (c.after.id, c.after.revision))
        .collect();
    revisions.sort_unstable_by_key(|v| v.0);
    Ok(FrozenPlayerDeath {
        expires_at: expiry,
        operation,
        player,
        receipt: PlayerDeathReceipt {
            operation: ticket.operation,
            actor: ticket.actor,
            after_revision: ticket.after_revision,
            inventory: bace_simulation::InventoryReceipt {
                operation: ticket.inventory.operation,
                revisions,
            },
        },
    })
}

/// Convert the exact persisted deadline using one captured Unix/tick pair at
/// committed-result admission. Retry retains the saved deadline unchanged.
pub fn corpse_expiry_tick(expires_at: i64, unix_seconds: i64, tick: u64) -> Result<u64, String> {
    if expires_at < 0 || unix_seconds < 0 {
        return Err("invalid corpse expiry clock".into());
    }
    let remaining = u64::try_from(expires_at.saturating_sub(unix_seconds).max(0))
        .map_err(|_| "corpse expiry clock overflow")?;
    remaining
        .checked_mul(30)
        .and_then(|v| tick.checked_add(v))
        .ok_or_else(|| "corpse expiry tick overflow".into())
}
