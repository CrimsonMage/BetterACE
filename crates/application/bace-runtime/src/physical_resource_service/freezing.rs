use super::*;
use crate::game_inventory::{InventoryFreezeInput, freeze_inventory};
pub(super) fn freeze(
    ticket: &PhysicalResourceTicket,
    online: &OnlinePlayerSaveService,
    unix: u64,
) -> Result<PendingPlacementSave, String> {
    let (baseline, version, lease) = online
        .baseline(ticket.binding.actor.0)
        .ok_or("ammunition player baseline missing")?;
    let player = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        &ticket.snapshot,
        bace_simulation::PlayerSnapshotOperation::PhysicalAmmo(ticket.launch.operation),
        ticket.actor_revision,
        unix,
    )
    .map_err(|e| e.to_string())?;
    let mut items = online.operation_inventory_baselines(&ticket.snapshot)?;
    for change in &ticket.inventory.proposal.changes {
        let before = change
            .before
            .as_ref()
            .ok_or("ammunition cannot create inventory")?;
        let source = items
            .iter_mut()
            .find(|source| source.entity.object_id == before.id.0)
            .ok_or("ammunition item baseline missing")?;
        if source.entity.mutation_revision > before.revision
            || source.entity.state.weenie_id != before.template
        {
            return Err("ammunition source identity/revision mismatch".into());
        }
        source.entity.mutation_revision = before.revision;
        crate::game_inventory::set(
            &mut source.entity.state.properties.ints,
            12,
            i32::try_from(before.stack).map_err(|_| "ammunition stack overflow")?,
        );
    }
    let mut rows = online.operation_inventory_changes(&ticket.snapshot)?;
    rows.retain(|row| {
        !ticket
            .inventory
            .proposal
            .changes
            .iter()
            .any(|change| change.after.id.0 == row.object_id)
    });
    if &player != baseline {
        rows.push(SaveSnapshot {
            object_id: ticket.binding.actor.0,
            mutation_revision: player.player.entity.mutation_revision,
            expected_version: version,
            bytes: player.encode().map_err(|e| e.to_string())?,
        });
    }
    let id =
        ticket
            .launch
            .event_id
            .iter()
            .fold(String::from("physical-ammo:"), |mut text, byte| {
                use std::fmt::Write;
                write!(text, "{byte:02x}").expect("String write");
                text
            });
    let operation = freeze_inventory(InventoryFreezeInput {
        operation_id: &id,
        proposal: &ticket.inventory.proposal,
        items: &items,
        other_snapshots: &rows,
        leases: &[lease],
        storage_views: &[],
        admitted_positions: &Default::default(),
    })
    .map_err(|e| e.to_string())?;
    PendingPlacementSave::new(operation).map_err(|e| e.to_string())
}
