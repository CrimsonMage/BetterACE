//! Prepared fresh factory state for inventory splits. Runs outside simulation;
//! callers pass an immutable accepted content generation and a reserved identity.
use crate::game_inventory::FrozenInventoryItem;
use bace_content::WeenieV1;
use bace_inventory::{InventoryItem, ItemPlace};
use bace_storage_codec::EntitySaveV1;
use bace_types::EntityId;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct PreparedStack {
    pub item: InventoryItem,
    pub frozen: FrozenInventoryItem,
}

pub fn prepare_split_stack(
    template: &WeenieV1,
    template_revision: u64,
    id: EntityId,
    amount: u32,
    destination: ItemPlace,
    wield_requirements_met: bool,
) -> Result<PreparedStack, String> {
    if !(0x80000000..=0xfffffffe).contains(&id.0)
        || template_revision == 0
        || !bace_loot::is_stackable(template.weenie_type)
        || destination == ItemPlace::Removed
    {
        return Err("invalid fresh split identity/template/destination".into());
    }
    let int = |id| {
        template
            .properties
            .ints
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.value)
    };
    let positive = |id, default| {
        u32::try_from(int(id).unwrap_or(default))
            .map_err(|_| "negative split template property".to_string())
    };
    let maximum_stack = positive(11, 1)?;
    if amount == 0 || amount > maximum_stack {
        return Err("invalid fresh split quantity".into());
    }
    let template_stack = positive(12, 1)?.max(1);
    let unit_burden = positive(13, int(5).unwrap_or(0) / template_stack as i32)?;
    let unit_value = positive(15, int(19).unwrap_or(0) / template_stack as i32)?;
    let unique = match int(279).unwrap_or(0) {
        0 => false,
        1 => true,
        _ => return Err("split unique count requires explicit inventory limit".into()),
    };
    let mut state = template.clone();
    state
        .properties
        .instance_ids
        .retain(|p| ![1, 2, 3, 6].contains(&p.id));
    state.properties.positions.retain(|p| p.id != 1);
    state.properties.ints.retain(|p| ![10, 53].contains(&p.id));
    let mut identity = state.clone();
    identity.class_name = "stack_identity".into();
    identity.last_modified = None;
    identity.properties.authoring_metadata = None;
    identity
        .properties
        .ints
        .retain(|p| ![5, 12, 19].contains(&p.id));
    let bytes = bace_content_tools::compile_template(&identity).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(bytes);
    let item = InventoryItem {
        structure: int(92)
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "negative structure")?,
        id,
        revision: 0,
        template: template.weenie_id,
        stack_key: u64::from_le_bytes(hash[..8].try_into().map_err(|_| "stack hash")?),
        place: destination,
        stack: amount,
        maximum_stack,
        unit_burden,
        unit_value,
        pack_slot: false,
        is_container: false,
        attuned: int(114).unwrap_or(0) != 0,
        trade_reserved: false,
        active_pet: false,
        unique,
        quest_allowed: true,
        valid_wield: positive(9, 0)?,
        incompatible_wield: 0,
        wield_requirements_met,
    };
    for (property, value) in [
        (12, u64::from(amount)),
        (5, u64::from(amount) * u64::from(unit_burden)),
        (19, u64::from(amount) * u64::from(unit_value)),
    ] {
        crate::game_inventory::set(
            &mut state.properties.ints,
            property,
            i32::try_from(value).map_err(|_| "split value overflow")?,
        );
    }
    Ok(PreparedStack {
        item,
        frozen: FrozenInventoryItem {
            corpse: None,
            construction: None,
            source_destination: None,
            enchantments: Vec::new(),
            entity: EntitySaveV1 {
                object_id: id.0,
                template_revision,
                mutation_revision: 0,
                state,
            },
            // Absence here means no prior durable row. The proposal always
            // supplies an explicit final world/container/equipment location.
            placement: None,
            persisted_version: 0,
        },
    })
}
