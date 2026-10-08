//! ACE Vendor.LoadInventory selects exact Shop CreateList rows on first Use.
use crate::{PreparedContainerItem, TreasureError, materialize_container_tree};
use bace_content::{Property, WeenieV1};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct PreparedVendorShopTree {
    /// ACE VendorShopCreateListStackSize. -1 means unlimited in ApproachVendor.
    pub display_quantity: i32,
    /// Source ordered root and Contain descendants. The root retains Shop origin.
    pub items: Vec<PreparedContainerItem>,
}

/// Pure, bounded first-Use preparation. ACE's lazy stock does not roll the
/// Creature create-list selector, merge duplicate WCIDs, or mutate StackSize.
/// A missing factory template is skipped as CreateNewWorldObject(null) is.
pub fn materialize_vendor_shop_create_list(
    vendor: &WeenieV1,
    templates: &BTreeMap<u32, Arc<WeenieV1>>,
) -> Result<Vec<PreparedVendorShopTree>, TreasureError> {
    if vendor.properties.create_list.len() > 4096 {
        return Err(TreasureError::Capacity);
    }
    let mut total = 0usize;
    let mut output = Vec::new();
    for row in vendor
        .properties
        .create_list
        .iter()
        .filter(|row| row.destination_type == 4)
    {
        if !row.shade.is_finite() || !(-1..=0x00ff_ffff).contains(&row.stack_size) {
            return Err(TreasureError::Bounds);
        }
        let Some(template) = templates.get(&row.weenie_class_id) else {
            continue;
        };
        let mut source = (**template).clone();
        if row.palette > 0 {
            set(&mut source.properties.ints, 3, i32::from(row.palette));
        }
        if row.shade > 0.0 {
            set(&mut source.properties.floats, 12, f64::from(row.shade));
        }
        let mut items = materialize_container_tree(source, templates)?;
        let root = items.first_mut().ok_or(TreasureError::Bounds)?;
        root.source_destination = Some(4);
        total = total
            .checked_add(items.len())
            .filter(|count| *count <= 1024)
            .ok_or(TreasureError::Capacity)?;
        output.push(PreparedVendorShopTree {
            display_quantity: row.stack_size,
            items,
        });
    }
    Ok(output)
}

fn set<T>(values: &mut Vec<Property<T>>, id: u32, value: T) {
    if let Some(property) = values.iter_mut().find(|property| property.id == id) {
        property.value = value;
    } else {
        values.push(Property { id, value });
        values.sort_by_key(|property| property.id);
    }
}
