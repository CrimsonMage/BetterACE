//! Frozen world-item sources for an exact scripted retirement. Captured live
//! placement/revisions fence cold rows; final tombstones discard no surviving data.
use crate::game_inventory::FrozenInventoryItem;
use bace_inventory::ItemPlace;
use bace_simulation::GeneratedRetirementTicket;
use bace_storage_codec::ItemPlacementV2;
pub async fn prepare_deletion_sources(
    store: &bace_db_postgres::PgStore,
    ticket: &GeneratedRetirementTicket,
    transient: Vec<FrozenInventoryItem>,
) -> Result<Vec<FrozenInventoryItem>, String> {
    if ticket.npc_source_ticket.is_none() || ticket.inventory.proposal.changes.len() > 1024 {
        return Err("NPC retirement source bounds".into());
    }
    let mut result = Vec::new();
    let mut bytes = 0usize;
    for change in &ticket.inventory.proposal.changes {
        let before = change
            .before
            .as_ref()
            .ok_or("NPC retirement source before-image")?;
        let placement = match before.place {
            ItemPlace::Contained {
                container,
                slot,
                equipped,
            } => ItemPlacementV2::Contained {
                container: container.0,
                slot,
                pack_slot: before.pack_slot,
                equipped,
            },
            ItemPlace::World => {
                let p = ticket
                    .positions
                    .get(&before.id)
                    .ok_or("NPC retirement accepted pose missing")?;
                ItemPlacementV2::World(bace_content::Position {
                    obj_cell_id: p.cell,
                    position_x: p.origin[0],
                    position_y: p.origin[1],
                    position_z: p.origin[2],
                    rotation_w: p.rotation[3],
                    rotation_x: p.rotation[0],
                    rotation_y: p.rotation[1],
                    rotation_z: p.rotation[2],
                })
            }
            ItemPlace::Removed => return Err("NPC retirement already removed input".into()),
        };
        let mut source = if ticket.transient.contains(&before.id) {
            transient
                .iter()
                .find(|f| f.entity.object_id == before.id.0)
                .cloned()
                .ok_or("NPC transient retirement source missing")?
        } else {
            let stored = store
                .load(before.id.0)
                .await
                .map_err(|e| e.to_string())?
                .ok_or("NPC retirement durable source missing")?;
            FrozenInventoryItem::decode(
                &stored.bytes,
                Some(placement.clone()),
                stored.persisted_version,
            )
            .map_err(|e| e.to_string())?
        };
        if source.corpse.is_some()
            || source.entity.object_id != before.id.0
            || source.entity.state.weenie_id != before.template
            || source.entity.mutation_revision > before.revision
        {
            return Err("NPC retirement source identity/revision mismatch".into());
        }
        if change.after.place != ItemPlace::Removed {
            if let Some(entries) = ticket.enchantments.get(&before.id) {
                if !ticket.registry_revisions.contains_key(&before.id) {
                    return Err("NPC surviving registry revision missing".into());
                }
                source.enchantments = entries
                    .iter()
                    .map(crate::enchantment_saves::freeze_enchantment)
                    .collect::<Result<_, _>>()
                    .map_err(|e| e.to_string())?;
            } else if !source.enchantments.is_empty() {
                return Err("NPC surviving registry snapshot missing".into());
            }
        }
        source.entity.mutation_revision = before.revision;
        source.placement = Some(placement);
        crate::game_inventory::set(
            &mut source.entity.state.properties.ints,
            12,
            i32::try_from(before.stack).map_err(|_| "NPC retirement stack bound")?,
        );
        if let Some(structure) = before.structure {
            crate::game_inventory::set(
                &mut source.entity.state.properties.ints,
                92,
                i32::try_from(structure).map_err(|_| "NPC retirement structure bound")?,
            );
        } else {
            source.entity.state.properties.ints.retain(|p| p.id != 92);
        }
        bytes = bytes
            .checked_add(
                source
                    .entity
                    .encode_item()
                    .map_err(|e| e.to_string())?
                    .len(),
            )
            .ok_or("NPC retirement byte overflow")?;
        if bytes > 64 * 1024 * 1024 {
            return Err("NPC retirement byte budget".into());
        }
        result.push(source);
    }
    Ok(result)
}
