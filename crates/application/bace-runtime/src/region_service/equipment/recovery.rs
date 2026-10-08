//! Rebuild current durable custody with original GUIDs. The source head supplies
//! death-drop membership; newer item placements/tombstones were already read by DB.
use super::*;
pub(super) fn prepare(
    actor: EntityId,
    source: Arc<bace_content::WeenieV1>,
    pin: &crate::npc_region::PreparedNpcRegionSource,
) -> Result<RootEquipment, String> {
    let proof = pin
        .inventory
        .as_ref()
        .ok_or("NPC inventory proof missing")?;
    let mut indices = BTreeMap::new();
    let mut items = Vec::new();
    let mut restored = Vec::new();
    for row in pin.items.iter() {
        let saved = crate::region_service::world_items::decode(row)?;
        if saved.corpse.is_some() {
            return Err("NPC equipment cannot restore a corpse".into());
        }
        let bace_storage_codec::ItemPlacementV2::Contained {
            container,
            slot,
            equipped,
            ..
        } = saved.item.placement
        else {
            return Err("NPC saved gear custody missing".into());
        };
        let parent_index = if container == actor.0 {
            None
        } else {
            Some(
                *indices
                    .get(&container)
                    .ok_or("NPC saved gear parent order")?,
            )
        };
        let id = saved.item.entity.object_id;
        indices.insert(id, items.len());
        items.push(bace_loot::PreparedCreatureEquipment {
            source: saved.item.entity.state.clone(),
            source_destination: saved.source_destination,
            wielded_location: equipped,
            death_drop: proof.death_items.contains(&id),
            parent_index,
            inventory_slot: slot,
            equip_order: None,
        });
        restored.push(
            crate::game_inventory::FrozenInventoryItem::decode(
                &row.aggregate.bytes,
                Some(saved.item.placement.clone()),
                row.aggregate.persisted_version,
            )
            .map_err(|e| e.to_string())?,
        );
    }
    Ok(RootEquipment {
        actor,
        source,
        items,
        restored: Some(restored),
    })
}
pub(super) fn restore_revisions(
    loadout: &mut bace_simulation::PreparedNpcLoadout,
    saved: &[crate::game_inventory::FrozenInventoryItem],
) -> Result<(), String> {
    if loadout.items.len() != saved.len() {
        return Err("NPC restored inventory projection count".into());
    }
    for item in &mut loadout.items {
        let source = saved
            .iter()
            .find(|s| s.entity.object_id == item.id.0)
            .ok_or("NPC restored item identity")?;
        let bace_storage_codec::ItemPlacementV2::Contained {
            container,
            slot,
            equipped,
            pack_slot,
        } = source
            .placement
            .as_ref()
            .ok_or("NPC saved gear placement")?
        else {
            return Err("NPC saved gear world placement".into());
        };
        if item.place
            != (bace_inventory::ItemPlace::Contained {
                container: EntityId(*container),
                slot: *slot,
                equipped: *equipped,
            })
            || item.pack_slot != *pack_slot
        {
            return Err("NPC gear reconstruction changed custody".into());
        }
        item.revision = source.entity.mutation_revision;
    }
    for container in &mut loadout.containers {
        if let Some(item) = loadout.items.iter().find(|i| i.id == container.id) {
            container.revision = item.revision;
        }
    }
    for stamp in &mut Arc::make_mut(&mut loadout.profile).equipment {
        stamp.revision = loadout
            .items
            .iter()
            .find(|i| i.id.0 == stamp.entity)
            .ok_or("NPC restored equipped source")?
            .revision;
    }
    loadout.enchantments.clear();
    Ok(())
}
