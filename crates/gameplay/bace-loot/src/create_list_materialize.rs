//! Official WorldObjectFactory.CreateNewWorldObject(PropertiesCreateList),
//! 47edade3, AGPL-3.0-only. Selection probability is handled before this stage.
use crate::{TreasureError, set_treasure_stack};
use bace_content::{CreateListEntry, Property, WeenieV1};
pub fn materialize_create_list(
    row: &CreateListEntry,
    template: &WeenieV1,
) -> Result<WeenieV1, TreasureError> {
    if row.weenie_class_id == 0
        || row.weenie_class_id != template.weenie_id
        || !row.shade.is_finite()
    {
        return Err(TreasureError::InvalidTemplate(row.weenie_class_id));
    }
    let mut item = template.clone();
    if row.stack_size > 1 {
        set_treasure_stack(&mut item, row.stack_size)?;
    }
    if row.palette > 0 {
        set(&mut item.properties.ints, 3, i32::from(row.palette));
    }
    if row.destination_type & 8 == 0 {
        set(&mut item.properties.floats, 12, f64::from(row.shade));
    }
    // DestinationType is ephemeral behavior, not a fabricated persisted property.
    Ok(item)
}
fn set<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(property) = values.iter_mut().find(|p| p.id == id) {
        property.value = value;
    } else {
        values.push(Property { id, value });
        values.sort_by_key(|p| p.id);
    }
}
