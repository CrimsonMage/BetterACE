use super::*;
use crate::{
    allegiance_pending::{AllegianceResolution, PendingAllegianceSave},
    placement_saves::{PendingPlacementSave, PlacementResolution},
};
use bace_persistence::{AllegiancePlacementOperation, PlacementOperation};
pub(super) enum Frozen {
    Plain(PendingAllegianceSave),
    Placement(Box<PendingPlacementSave>),
}
impl Frozen {
    pub(super) fn retry_rejected_npc(&self) -> Result<Self, String> {
        let Self::Placement(saved) = self else {
            return Err("NPC composite cannot be plain allegiance".into());
        };
        let operation = saved
            .allegiance_operation()
            .ok_or("NPC allegiance composite missing")?
            .clone();
        if operation.workflow.is_none() {
            return Err("NPC composite missing checkpoint".into());
        }
        Ok(Self::Placement(Box::new(
            PendingPlacementSave::new_allegiance(operation).map_err(|e| e.to_string())?,
        )))
    }

    pub(super) fn operation(&self) -> &AllegianceOperation {
        match self {
            Self::Plain(p) => p.operation(),
            Self::Placement(p) => {
                &p.allegiance_operation()
                    .expect("allegiance placement")
                    .allegiance
            }
        }
    }
    pub(super) fn submit(&mut self, saves: &SaveHandle) -> Result<(), SaveSubmitError> {
        match self {
            Self::Plain(p) => p.submit(saves),
            Self::Placement(p) => p.submit(saves),
        }
    }
    pub(super) fn poll(&mut self) -> Option<Result<bool, String>> {
        Some(match self {
            Self::Plain(p) => match p.poll()? {
                AllegianceResolution::Committed { .. } => Ok(true),
                AllegianceResolution::Rejected(_) => Ok(false),
                AllegianceResolution::Uncertain(e) => Err(e),
            },
            Self::Placement(p) => match p.poll()? {
                PlacementResolution::Committed(_) => Ok(true),
                PlacementResolution::Rejected(_) => Ok(false),
                PlacementResolution::Uncertain(e) => Err(e),
            },
        })
    }
    pub(super) fn committed_rows(&self) -> Result<Vec<SaveSnapshot>, String> {
        let mut rows = match self {
            Self::Plain(p) => p.operation().players.clone(),
            Self::Placement(p) => p.operation().snapshots.clone(),
        };
        for row in &mut rows {
            row.expected_version = row
                .expected_version
                .checked_add(1)
                .ok_or("allegiance version overflow")?;
        }
        Ok(rows)
    }
}
pub(super) fn freeze(
    epoch: u64,
    ledger: &ledger::Ledger,
    ticket: &AllegianceTicket,
    captures: &BTreeMap<u32, (Arc<PlayerReadSnapshot>, u64)>,
    leases: &[bace_persistence::CharacterLease],
    online: &OnlinePlayerSaveService,
    npc: Option<&super::NpcSharedCheckpoint>,
) -> Result<Frozen, String> {
    let mut baselines = Vec::with_capacity(captures.len());
    for (&actor, (capture, unix)) in captures {
        let (source, version, lease) = online
            .baseline(actor)
            .ok_or("allegiance player baseline missing")?;
        if !leases.contains(&lease) {
            return Err("allegiance player lease changed during preparation".into());
        }
        let revision = ticket
            .player_changes
            .iter()
            .find(|(id, _)| id.0 == actor)
            .ok_or("allegiance capture actor")?
            .1
            .experience
            .before_revision;
        let saved = crate::player_saves::freeze_player_operation_baseline(
            source,
            capture,
            PlayerSnapshotOperation::Allegiance(ticket.operation),
            revision,
            *unix,
        )
        .map_err(|e| e.to_string())?;
        baselines.push((saved, version, lease));
    }
    let inputs: Vec<_> = baselines
        .iter()
        .map(
            |(saved, version, lease)| crate::allegiance_players::AllegiancePlayerInput {
                saved,
                version: *version,
                lease: *lease,
            },
        )
        .collect();
    let players = crate::allegiance_players::freeze_allegiance_players(ticket, &inputs)
        .map_err(|e| e.to_string())?;
    let (nodes, metadata) = ledger.before(ticket);
    let mut operation = crate::allegiance_saves::freeze_allegiance(
        crate::allegiance_saves::AllegianceFreezeInput {
            world_epoch: epoch,
            operation: ticket.operation,
            patch: &ticket.patch,
            stored_nodes: &nodes,
            stored_metadata: &metadata,
            players: &players.snapshots,
            leases,
        },
    )
    .map_err(|e| e.to_string())?;
    ledger.validate(&operation)?;
    let mut item_operations = vec![];
    for (reward, inventory) in &ticket.item_experience {
        let Some(inventory) = inventory else { continue };
        let (capture, _) = captures
            .get(&reward.actor.0)
            .ok_or("item XP player capture missing")?;
        let items = online.operation_inventory_baselines(capture)?;
        let vitae = ticket
            .vitae
            .iter()
            .find(|p| p.actor == reward.actor)
            .map(|p| &p.registry);
        let op = crate::item_experience::freeze_item_experience_with_vitae(
            crate::game_inventory::InventoryFreezeInput {
                operation_id: &operation.operation_id,
                proposal: &inventory.proposal,
                items: &items,
                other_snapshots: &players.snapshots,
                leases: &players.leases,
                storage_views: &[],
                admitted_positions: &BTreeMap::new(),
            },
            reward,
            inventory,
            vitae,
        )
        .map_err(|e| e.to_string())?;
        item_operations.push((inventory.operation, op));
    }
    if let Some(npc) = npc {
        let mut joined = crate::npc_shared_saves::freeze_npc_shared_experience(
            crate::npc_shared_saves::NpcSharedFreezeInput {
                ticket,
                binding: npc.token.binding,
                world_epoch: epoch,
                workflow_version: npc.token.workflow_version,
                checkpoint: npc.checkpoint.clone(),
                players: &inputs,
                nodes: &nodes,
                metadata: &metadata,
                item_operations: &item_operations,
            },
        )
        .map_err(|e| e.to_string())?;
        if let Some(source) = &npc.source_inventory {
            let mut stage = bace_persistence::NpcStageOperation {
                inventory: joined.placement,
                workflow: joined
                    .workflow
                    .take()
                    .ok_or("NPC shared workflow missing")?,
            };
            source.attach(&mut stage).map_err(|e| e.to_string())?;
            joined.placement = stage.inventory;
            joined.workflow = Some(stage.workflow);
        }
        // Offline allegiance participants use the same lease fences as the
        // ordinary lane; source checkpoint CAS is part of this exact operation.
        joined.placement.leases = leases.to_vec();
        joined.allegiance.leases = leases.to_vec();
        for (capture, _) in captures.values() {
            for row in online.operation_inventory_changes(capture)? {
                if !joined
                    .placement
                    .snapshots
                    .iter()
                    .any(|s| s.object_id == row.object_id)
                {
                    joined.placement.participants.push(row.object_id);
                    joined.placement.snapshots.push(row);
                }
            }
        }
        joined
            .placement
            .participants
            .extend(leases.iter().map(|l| l.character_id));
        joined.placement.participants.sort_unstable();
        joined.placement.participants.dedup();
        if joined.placement.snapshots.len() > 1024
            || joined.placement.participants.len() > 1024
            || joined
                .placement
                .snapshots
                .iter()
                .map(|s| s.bytes.len())
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            return Err("NPC allegiance frozen snapshot budget".into());
        }
        return Ok(Frozen::Placement(Box::new(
            PendingPlacementSave::new_allegiance(joined).map_err(|e| e.to_string())?,
        )));
    }
    let mut placement = crate::item_reward_join::join_allegiance_item_operations(
        PlacementOperation {
            operation_id: operation.operation_id.clone(),
            snapshots: players.snapshots,
            participants: leases.iter().map(|l| l.character_id).collect(),
            leases: leases.to_vec(),
            changes: vec![],
            storage_views: vec![],
        },
        ticket,
        &item_operations,
    )
    .map_err(|e| e.to_string())?;
    for (capture, _) in captures.values() {
        for row in online.operation_inventory_changes(capture)? {
            // Item-XP rows were frozen from this exact captured before image and
            // already contain its unrelated changes plus the accepted XP patch.
            if !placement
                .snapshots
                .iter()
                .any(|s| s.object_id == row.object_id)
            {
                placement.participants.push(row.object_id);
                placement.snapshots.push(row);
            }
        }
    }
    if placement
        .snapshots
        .iter()
        .map(|s| s.bytes.len())
        .sum::<usize>()
        > 64 * 1024 * 1024
        || placement.snapshots.len() > 1024
    {
        return Err("allegiance frozen snapshot budget".into());
    }
    if placement.snapshots.len() == operation.players.len() && item_operations.is_empty() {
        Ok(Frozen::Plain(
            PendingAllegianceSave::new(operation).map_err(|e| e.to_string())?,
        ))
    } else {
        operation.players.clear();
        placement.participants.sort_unstable();
        placement.participants.dedup();
        Ok(Frozen::Placement(Box::new(
            PendingPlacementSave::new_allegiance(AllegiancePlacementOperation {
                placement,
                allegiance: operation,
                world_epoch: epoch,
                workflow: None,
            })
            .map_err(|e| e.to_string())?,
        )))
    }
}
