//! Pinned AttributeTransferDevice Int189/190 and four wield slots.
use bace_content::WeenieV1;
use bace_gameplay_api::AttributeId;
use bace_simulation::PreparedAttributeTransfer;

pub(super) fn device(weenie: &WeenieV1) -> Result<Option<PreparedAttributeTransfer>, String> {
    if weenie.weenie_type != 63 {
        return Ok(None);
    }
    // Generic emote/cooldown/target side effects require the separate activation
    // owner. A simple transfer cannot silently discard those authored effects.
    let properties = &weenie.properties;
    if properties.ints.iter().any(|property| {
        (property.id == 83 && property.value != 2)
            || (property.id == 119 && property.value == 0)
            || (property.id == 280 && property.value != 0)
    }) || properties
        .floats
        .iter()
        .any(|property| property.id == 167 && property.value > 0.)
        || properties
            .instance_ids
            .iter()
            .any(|property| property.id == 16 && property.value != 0)
        || !properties.emotes.is_empty()
    {
        return Err("attribute-transfer device requires generic activation effect owner".into());
    }
    let value = |id| {
        properties
            .ints
            .iter()
            .find(|property| property.id == id)
            .map_or(0, |property| property.value)
    };
    PreparedAttributeTransfer::source(value(189), value(190))
        .map(Some)
        .map_err(|error| format!("attribute-transfer source: {error:?}"))
}

pub(super) fn wielded_attribute_requirement(weenie: &WeenieV1) -> bool {
    [158, 270, 273, 276].into_iter().any(|id| {
        weenie
            .properties
            .ints
            .iter()
            .any(|property| property.id == id && matches!(property.value, 3 | 4))
    })
}

pub(super) fn name(id: AttributeId) -> &'static str {
    match id {
        AttributeId::Strength => "Strength",
        AttributeId::Endurance => "Endurance",
        AttributeId::Quickness => "Quickness",
        AttributeId::Coordination => "Coordination",
        AttributeId::Focus => "Focus",
        AttributeId::SelfAttribute => "Self",
    }
}
pub(super) fn prompt(device: PreparedAttributeTransfer) -> String {
    format!(
        "This action will transfer 10 points from your {} to your {}.",
        name(device.from),
        name(device.to)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::Property;
    #[test]
    fn pinned_int_properties_and_all_four_wield_slots() {
        let mut item = WeenieV1 {
            schema_version: 1,
            weenie_id: 1,
            class_name: "attribute device".into(),
            weenie_type: 63,
            last_modified: None,
            properties: Default::default(),
        };
        item.properties.ints.push(Property { id: 189, value: 1 });
        item.properties.ints.push(Property { id: 190, value: 2 });
        assert_eq!(
            device(&item).unwrap(),
            Some(PreparedAttributeTransfer::source(1, 2).unwrap())
        );
        for id in [158, 270, 273, 276] {
            item.properties.ints.push(Property { id, value: 4 });
            assert!(wielded_attribute_requirement(&item));
            item.properties.ints.pop();
        }
        assert!(!wielded_attribute_requirement(&item));
        item.properties.ints.retain(|property| property.id != 189);
        assert!(device(&item).is_err());
    }
}
