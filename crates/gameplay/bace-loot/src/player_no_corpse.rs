//! Fresh CreateList selection for ACE's player NoCorpse GenerateTreasure branch.
use crate::{
    PreparedCreatureEquipment, TreasureError, TreasureRandom, generate_create_list_selection,
    materialize_create_list_tree,
};
use bace_content::WeenieV1;
use std::{collections::BTreeMap, sync::Arc};

/// Select fresh player death rows with an explicit random cursor. The caller
/// owns DID35 loot and existing inventory/equipment selection separately.
/// Selected roots and their Contain children retain source order and parent
/// indices; failure leaves the caller's random cursor unchanged.
pub fn generate_player_no_corpse_create_list<R: TreasureRandom>(
    player: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
    random: &mut R,
) -> Result<Vec<PreparedCreatureEquipment>, TreasureError> {
    if player.properties.create_list.len() > 4096 {
        return Err(TreasureError::Capacity);
    }
    let rows: Vec<_> = player
        .properties
        .create_list
        .iter()
        .filter(|row| {
            row.destination_type & 1 != 0
                || row.destination_type & 8 != 0 && row.destination_type & 2 == 0
        })
        .cloned()
        .collect();
    let mut cursor = random.clone();
    let mut forest = Vec::new();
    for index in generate_create_list_selection(&rows, &mut cursor)? {
        let offset = forest.len();
        for mut item in materialize_create_list_tree(&rows[index], templates)? {
            item.parent_index = item
                .parent_index
                .map(|parent| parent.checked_add(offset).ok_or(TreasureError::Capacity))
                .transpose()?;
            forest.push(item);
            if forest.len() > 1024 {
                return Err(TreasureError::Capacity);
            }
        }
    }
    *random = cursor;
    Ok(forest)
}
