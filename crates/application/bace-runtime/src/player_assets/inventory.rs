//! Reconstruct immutable inventory projections from exact durable ownership rows.
use super::*;
use crate::game_login::LoadedPlayer;
use bace_inventory::{InventoryContainer, InventoryItem, ItemPlace};
use bace_types::EntityId;
use sha2::{Digest, Sha256};
pub struct PreparedPlayerInventory {
    pub items: Vec<InventoryItem>,
    pub containers: Vec<InventoryContainer>,
}
fn integer(w: &bace_content::WeenieV1, id: u32, default: i32) -> i32 {
    w.properties
        .ints
        .iter()
        .find(|p| p.id == id)
        .map_or(default, |p| p.value)
}
fn positive(w: &bace_content::WeenieV1, id: u32, default: i32) -> Result<u32, String> {
    u32::try_from(integer(w, id, default)).map_err(|_| format!("negative inventory property {id}"))
}
pub fn prepare_player_inventory(
    loaded: &LoadedPlayer,
    state: &bace_simulation::OwnedPlayerState,
    stats: &PlayerStatProjection,
) -> Result<PreparedPlayerInventory, String> {
    if loaded.inventory.len() > 1023 {
        return Err("player inventory capacity".into());
    }
    let actor = loaded.binding.actor;
    let source = &loaded.player.player.entity.state;
    let capacity = (150u64 + 30 * u64::from(positive(source, 230, 0)?))
        .checked_mul(u64::from(stats.current_attributes[0]))
        .and_then(|v| v.checked_mul(3))
        .ok_or("player burden capacity overflow")?;
    let container =
        |id, revision, w: &bace_content::WeenieV1, root| -> Result<InventoryContainer, String> {
            Ok(InventoryContainer {
                id,
                revision,
                root_owner: Some(actor),
                slots: positive(w, 6, 0)?,
                pack_slots: positive(w, 7, 0)?,
                burden_limit: if root { capacity } else { u64::MAX },
                accessible: true,
                open: true,
                generation: u64::try_from(loaded.lease.epoch)
                    .map_err(|_| "invalid inventory lease generation")?,
            })
        };
    let mut containers = vec![container(
        actor,
        loaded.player.player.entity.mutation_revision,
        source,
        true,
    )?];
    let mut items = Vec::with_capacity(loaded.inventory.len());
    for saved in &loaded.inventory {
        let w = &saved.entity.state;
        let id = EntityId(saved.entity.object_id);
        let bace_storage_codec::ItemPlacementV2::Contained {
            container: parent,
            slot,
            pack_slot,
            equipped,
        } = &saved.placement
        else {
            return Err("unowned player inventory placement".into());
        };
        if saved.location.container != *parent || saved.location.slot != *slot {
            return Err("player inventory relational mismatch".into());
        }
        let mut identity = w.clone();
        identity.class_name = "stack_identity".into();
        identity.last_modified = None;
        identity.properties.authoring_metadata = None;
        identity
            .properties
            .ints
            .retain(|p| !matches!(p.id, 5 | 12 | 19 | 10 | 53));
        identity
            .properties
            .instance_ids
            .retain(|p| !matches!(p.id, 1 | 2 | 3 | 6));
        identity.properties.positions.retain(|p| p.id != 1);
        let hash = Sha256::digest(
            bace_content_tools::compile_template(&identity).map_err(|e| e.to_string())?,
        );
        let stack = positive(w, 12, 1)?;
        let denominator = i32::try_from(stack.max(1)).map_err(|_| "item stack width")?;
        let is_container = w.properties.ints.iter().any(|p| p.id == 6);
        let unique = positive(w, 279, 0)?;
        if unique > 1 {
            return Err("unsupported item unique count".into());
        }
        let requirements =
            super::wield::wield_requirements(loaded.binding.actor, w, source, state, stats)?;
        items.push(InventoryItem {
            structure: w
                .properties
                .ints
                .iter()
                .find(|p| p.id == 92)
                .map(|p| u32::try_from(p.value))
                .transpose()
                .map_err(|_| "invalid item structure")?,
            id,
            revision: saved.entity.mutation_revision,
            template: w.weenie_id,
            stack_key: u64::from_le_bytes(hash[..8].try_into().map_err(|_| "stack hash")?),
            place: ItemPlace::Contained {
                container: EntityId(*parent),
                slot: *slot,
                equipped: *equipped,
            },
            stack,
            maximum_stack: positive(w, 11, 1)?,
            unit_burden: positive(w, 13, integer(w, 5, 0) / denominator)?,
            unit_value: positive(w, 15, integer(w, 19, 0) / denominator)?,
            pack_slot: *pack_slot,
            is_container,
            attuned: integer(w, 114, 0) != 0,
            trade_reserved: false,
            active_pet: false,
            unique: unique == 1,
            quest_allowed: true,
            valid_wield: integer(w, 9, 0) as u32,
            incompatible_wield: 0,
            wield_requirements_met: requirements,
        });
        if is_container {
            containers.push(container(id, saved.entity.mutation_revision, w, false)?);
        }
    }
    Ok(PreparedPlayerInventory { items, containers })
}
