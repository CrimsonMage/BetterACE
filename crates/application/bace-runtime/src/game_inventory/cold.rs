//! Preserve the complete corpse descriptor while exposing the common item view.
use super::*;
use bace_storage_codec::{CorpseSaveV5, ItemSaveV3, ItemSaveV4, ItemSaveV5};
pub fn decode_inventory_item(
    bytes: &[u8],
    placement: Option<ItemPlacementV2>,
) -> Result<(ItemSaveV5, Option<CorpseSaveV5>), bace_storage_codec::SaveCodecError> {
    let header = bace_storage_codec::inspect(
        bytes,
        bace_storage_codec::CodecLimits {
            max_payload_bytes: 2 * 1024 * 1024,
        },
    )?;
    if header.kind == 102 {
        let corpse = CorpseSaveV5::decode_or_migrate(bytes, placement)?;
        let item = ItemSaveV4 {
            previous: ItemSaveV3 {
                previous: ItemSaveV2 {
                    entity: corpse.corpse.entity.clone(),
                    placement: corpse.placement.clone(),
                },
                enchantments: corpse.enchantments.clone(),
            },
            construction: None,
        };
        Ok((ItemSaveV5::migrate_v4(item)?, Some(corpse)))
    } else {
        Ok((ItemSaveV5::decode_or_migrate(bytes, placement)?, None))
    }
}
impl FrozenInventoryItem {
    pub fn decode(
        bytes: &[u8],
        placement: Option<ItemPlacementV2>,
        persisted_version: i64,
    ) -> Result<Self, bace_storage_codec::SaveCodecError> {
        let (saved, corpse) = decode_inventory_item(bytes, placement)?;
        Ok(Self {
            construction: saved.previous.construction,
            source_destination: saved.source_destination,
            corpse: corpse.map(Box::new),
            entity: saved.previous.previous.previous.entity,
            placement: Some(saved.previous.previous.previous.placement),
            enchantments: saved.previous.previous.enchantments,
            persisted_version,
        })
    }
}
