use super::*;
use crate::{game_inventory::InventoryFreezeInput, skill_saves};
use std::collections::BTreeMap;
pub(super) fn freeze(
    identity: SkillOperationId,
    owner: &SkillSaveOwner,
    snapshot: &PlayerReadSnapshot,
    unix_millis: u64,
    online: &OnlinePlayerSaveService,
) -> Result<PendingSkillSave, String> {
    let ticket = reference(owner);
    if snapshot.binding() != binding(ticket) {
        return Err("skill capture binding mismatch".into());
    }
    let (baseline, version, lease) = online
        .baseline(ticket.context.actor.0)
        .ok_or("missing skill durable baseline")?;
    let saved = crate::player_saves::freeze_player_operation_baseline(
        baseline,
        snapshot,
        match owner {
            SkillSaveOwner::AttributeTransfer(_) => {
                PlayerSnapshotOperation::AttributeTransfer(ticket.operation)
            }
            _ => PlayerSnapshotOperation::Skill(ticket.operation),
        },
        ticket.expected_revision,
        unix_millis,
    )
    .map_err(|e| e.to_string())?;
    let mut pending = match owner {
        SkillSaveOwner::Plain(ticket) => {
            skill_saves::freeze_skill_ticket(identity, *ticket, &saved, version, lease)
        }
        SkillSaveOwner::Device(ticket) => {
            if let Some(cooldown) = &ticket.cooldown
                && snapshot
                    .enchantments()
                    .is_none_or(|registry| registry.revision() != cooldown.before_revision)
            {
                return Err("skill device cooldown capture revision mismatch".into());
            }
            let items = online.operation_inventory_baselines(snapshot)?;
            skill_saves::freeze_skill_device(
                identity,
                ticket.clone(),
                &saved,
                version,
                lease,
                InventoryFreezeInput {
                    operation_id: "replaced-by-skill-operation-id",
                    proposal: &ticket.inventory.proposal,
                    items: &items,
                    other_snapshots: &[],
                    leases: &[lease],
                    storage_views: &[],
                    admitted_positions: &BTreeMap::new(),
                },
            )
        }
        SkillSaveOwner::AttributeTransfer(ticket) => {
            let items = online.operation_inventory_baselines(snapshot)?;
            skill_saves::freeze_attribute_transfer_device(
                identity,
                ticket.clone(),
                &saved,
                version,
                lease,
                InventoryFreezeInput {
                    operation_id: "replaced-by-attribute-transfer-id",
                    proposal: &ticket.inventory.proposal,
                    items: &items,
                    other_snapshots: &[],
                    leases: &[lease],
                    storage_views: &[],
                    admitted_positions: &BTreeMap::new(),
                },
            )
        }
    }
    .map_err(|e| e.to_string())?;
    pending
        .join_captured_inventory(online.operation_inventory_changes(snapshot)?)
        .map_err(|e| e.to_string())?;
    Ok(pending)
}
